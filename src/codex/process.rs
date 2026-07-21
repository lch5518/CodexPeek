use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{codex::locator::CliCandidate, UsageError};

use super::locator::CandidateKind;

const GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaunchPlan {
    pub(crate) program: PathBuf,
    pub(crate) arguments: Vec<String>,
}

pub(crate) fn launch_plan(kind: CandidateKind, path: PathBuf) -> LaunchPlan {
    fixed_plan(kind, path, &["app-server", "--stdio"])
}

pub(crate) fn version_plan(kind: CandidateKind, path: PathBuf) -> LaunchPlan {
    fixed_plan(kind, path, &["--version"])
}

fn fixed_plan(kind: CandidateKind, path: PathBuf, suffix: &[&str]) -> LaunchPlan {
    match kind {
        CandidateKind::NativeExe | CandidateKind::Direct => LaunchPlan {
            program: path,
            arguments: suffix
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect(),
        },
        CandidateKind::Command => {
            let mut arguments = vec![
                "/D".into(),
                "/S".into(),
                "/C".into(),
                path.to_string_lossy().into_owned(),
            ];
            arguments.extend(suffix.iter().map(|argument| (*argument).to_owned()));
            LaunchPlan {
                program: PathBuf::from("cmd.exe"),
                arguments,
            }
        }
        CandidateKind::PowerShell => {
            let mut arguments = vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                path.to_string_lossy().into_owned(),
            ];
            arguments.extend(suffix.iter().map(|argument| (*argument).to_owned()));
            LaunchPlan {
                program: PathBuf::from("powershell.exe"),
                arguments,
            }
        }
    }
}

pub(crate) struct ProcessGuard {
    child: Child,
    #[cfg(windows)]
    job: WindowsJob,
}

impl ProcessGuard {
    pub(crate) fn start(candidate: CliCandidate) -> Result<Self, UsageError> {
        let plan = launch_plan(candidate.kind, candidate.path);
        let mut command = Command::new(plan.program);
        command
            .args(plan.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command
            .spawn()
            .map_err(|_| UsageError::AppServerStartFailed)?;
        #[cfg(windows)]
        let job = match WindowsJob::attach(&child) {
            Ok(job) => job,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(UsageError::AppServerStartFailed);
            }
        };
        Ok(Self {
            child,
            #[cfg(windows)]
            job,
        })
    }

    pub(crate) fn take_transport(&mut self) -> Result<ChildTransport, UsageError> {
        let stdin = self
            .child
            .stdin
            .take()
            .ok_or(UsageError::AppServerStartFailed)?;
        let stdout = self
            .child
            .stdout
            .take()
            .ok_or(UsageError::AppServerStartFailed)?;
        Ok(ChildTransport {
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub(crate) fn shutdown(&mut self) {
        let deadline = Instant::now() + GRACEFUL_EXIT_TIMEOUT;
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.terminate_tree();
    }

    pub(crate) fn terminate_tree(&mut self) {
        #[cfg(windows)]
        self.job.terminate();
        #[cfg(not(windows))]
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) struct ChildTransport {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ChildTransport {
    pub(crate) fn write_line(&mut self, line: &str) -> Result<(), ()> {
        self.stdin.write_all(line.as_bytes()).map_err(|_| ())?;
        self.stdin.write_all(b"\n").map_err(|_| ())?;
        self.stdin.flush().map_err(|_| ())
    }

    pub(crate) fn read_line(&mut self) -> Result<Option<String>, ()> {
        let mut line = String::new();
        let count = self.stdout.read_line(&mut line).map_err(|_| ())?;
        if count == 0 {
            Ok(None)
        } else {
            Ok(Some(line))
        }
    }
}

#[cfg(windows)]
struct WindowsJob {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsJob {
    fn attach(child: &Child) -> windows::core::Result<Self> {
        use std::{mem::size_of, os::windows::io::AsRawHandle};
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::HANDLE,
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
            },
        };

        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            AssignProcessToJobObject(handle, HANDLE(child.as_raw_handle() as _))?;
        }
        Ok(Self { handle })
    }

    fn terminate(&self) {
        let _ = unsafe { windows::Win32::System::JobObjects::TerminateJobObject(self.handle, 1) };
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{launch_plan, version_plan, LaunchPlan};
    use crate::codex::locator::CandidateKind;

    #[test]
    fn command_wrapper_uses_a_fixed_cmd_launch_plan() {
        let plan = launch_plan(
            CandidateKind::Command,
            PathBuf::from("C:/Program Files/Codex/codex.cmd"),
        );

        assert_eq!(
            plan,
            LaunchPlan {
                program: PathBuf::from("cmd.exe"),
                arguments: vec![
                    "/D".into(),
                    "/S".into(),
                    "/C".into(),
                    "C:/Program Files/Codex/codex.cmd".into(),
                    "app-server".into(),
                    "--stdio".into(),
                ],
            }
        );
    }

    #[test]
    fn powershell_wrapper_uses_a_fixed_noninteractive_launch_plan() {
        let plan = launch_plan(CandidateKind::PowerShell, PathBuf::from("C:/bin/codex.ps1"));

        assert_eq!(plan.program, PathBuf::from("powershell.exe"));
        assert_eq!(
            plan.arguments,
            vec![
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "C:/bin/codex.ps1",
                "app-server",
                "--stdio",
            ]
        );
    }

    #[test]
    fn wrapper_version_checks_use_the_same_fixed_wrapper_arguments() {
        assert_eq!(
            version_plan(CandidateKind::Command, PathBuf::from("C:/bin/codex.cmd")).arguments,
            vec!["/D", "/S", "/C", "C:/bin/codex.cmd", "--version"]
        );
        assert_eq!(
            version_plan(CandidateKind::PowerShell, PathBuf::from("C:/bin/codex.ps1")).arguments,
            vec![
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "C:/bin/codex.ps1",
                "--version",
            ]
        );
    }
}
