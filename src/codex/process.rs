use std::{
    io::{BufRead, BufReader, Read, Write},
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
        CandidateKind::Command => LaunchPlan {
            program: PathBuf::from("cmd.exe"),
            arguments: vec![
                "/D".into(),
                "/S".into(),
                "/C".into(),
                format!("\"\"{}\" {}\"", path.to_string_lossy(), suffix.join(" ")),
            ],
        },
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
    pub(crate) fn start(candidate: CliCandidate, deadline: Instant) -> Result<Self, UsageError> {
        let plan = launch_plan(candidate.kind, candidate.path);
        Self::start_plan(plan, deadline)
    }

    pub(crate) fn start_plan(plan: LaunchPlan, deadline: Instant) -> Result<Self, UsageError> {
        if Instant::now() >= deadline {
            return Err(UsageError::RpcTimeout);
        }
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
            const CREATE_SUSPENDED: u32 = 0x0000_0004;
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        }
        let mut child = command
            .spawn()
            .map_err(|_| UsageError::AppServerStartFailed)?;
        #[cfg(windows)]
        let job = match WindowsJob::attach(&child) {
            Ok(job) => job,
            Err(_) => {
                let _ = child.kill();
                return Err(UsageError::AppServerStartFailed);
            }
        };
        #[cfg(windows)]
        if resume_suspended_process(child.id()).is_err() {
            let _ = child.kill();
            return Err(UsageError::AppServerStartFailed);
        }
        if Instant::now() >= deadline {
            #[cfg(windows)]
            job.terminate();
            #[cfg(not(windows))]
            {
                let _ = child.kill();
            }
            return Err(UsageError::RpcTimeout);
        }
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

    pub(crate) fn version_output(
        plan: LaunchPlan,
        deadline: Instant,
    ) -> Result<Vec<u8>, UsageError> {
        let mut guard = Self::start_plan(plan, deadline)?;
        let mut stdout = guard
            .child
            .stdout
            .take()
            .ok_or(UsageError::AppServerStartFailed)?;
        guard.wait_until(deadline)?;
        let mut output = Vec::new();
        stdout
            .read_to_end(&mut output)
            .map_err(|_| UsageError::RequestFailed)?;
        Ok(output)
    }

    pub(crate) fn wait_until(&mut self, deadline: Instant) -> Result<(), UsageError> {
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) if Instant::now() >= deadline => {
                    self.terminate_tree();
                    return Err(UsageError::RpcTimeout);
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => return Err(UsageError::RequestFailed),
            }
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.shutdown_until(Instant::now() + GRACEFUL_EXIT_TIMEOUT);
    }

    pub(crate) fn shutdown_until(&mut self, deadline: Instant) {
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

        let job = Self {
            handle: unsafe { CreateJobObjectW(None, PCWSTR::null()) }?,
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            AssignProcessToJobObject(job.handle, HANDLE(child.as_raw_handle() as _))?;
        }
        Ok(job)
    }

    fn terminate(&self) {
        let _ = unsafe { windows::Win32::System::JobObjects::TerminateJobObject(self.handle, 1) };
    }
}

#[cfg(windows)]
fn resume_suspended_process(process_id: u32) -> windows::core::Result<()> {
    use std::mem::size_of;
    use windows::Win32::{
        Foundation::CloseHandle,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }?;
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let thread_id: windows::core::Result<u32> = (|| {
        unsafe { Thread32First(snapshot, &mut entry)? };
        loop {
            if entry.th32OwnerProcessID == process_id {
                return Ok(entry.th32ThreadID);
            }
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            unsafe { Thread32Next(snapshot, &mut entry)? };
        }
    })();
    let _ = unsafe { CloseHandle(snapshot) };
    let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, thread_id?) }?;
    let result = unsafe { ResumeThread(thread) };
    let _ = unsafe { CloseHandle(thread) };
    if result == u32::MAX {
        Err(windows::core::Error::from_win32())
    } else {
        Ok(())
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
                    r#"""C:/Program Files/Codex/codex.cmd" app-server --stdio""#.into(),
                ],
            }
        );
    }

    #[test]
    fn command_wrapper_quotes_a_space_containing_script_as_one_command() {
        let plan = launch_plan(
            CandidateKind::Command,
            PathBuf::from("C:/Program Files/Codex/codex.cmd"),
        );

        assert_eq!(
            plan.arguments,
            vec![
                "/D",
                "/S",
                "/C",
                r#"""C:/Program Files/Codex/codex.cmd" app-server --stdio""#,
            ]
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
            vec!["/D", "/S", "/C", r#"""C:/bin/codex.cmd" --version""#]
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
