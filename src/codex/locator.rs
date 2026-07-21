use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{
    codex::process::{version_plan, ProcessGuard},
    UsageError,
};

const MINIMUM_VERSION: (u64, u64, u64) = (0, 141, 0);
const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateKind {
    NativeExe,
    Direct,
    Command,
    PowerShell,
}

impl CandidateKind {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        let kind = match name.as_str() {
            "codex.exe" => Some(Self::NativeExe),
            "codex" => Some(Self::Direct),
            "codex.cmd" => Some(Self::Command),
            "codex.ps1" => Some(Self::PowerShell),
            _ => None,
        }?;
        if kind == Self::Command && !is_safe_command_path(path) {
            return None;
        }
        Some(kind)
    }
}

fn is_safe_command_path(path: &Path) -> bool {
    !path
        .to_string_lossy()
        .chars()
        .any(|character| matches!(character, '"' | '%' | '!' | '&' | '^' | '(' | ')'))
}

#[derive(Clone, Debug)]
pub(crate) struct CliCandidate {
    pub(crate) path: PathBuf,
    pub(crate) kind: CandidateKind,
}

pub(crate) fn locate_cli(deadline: Instant) -> Result<CliCandidate, UsageError> {
    let mut candidates = gather_candidates(deadline)?;
    candidates.sort_by_key(|candidate| candidate_priority(candidate.kind));

    let mut saw_unsupported_version = false;
    for candidate in candidates {
        match verify_version(&candidate, deadline)? {
            Some(version) if version >= MINIMUM_VERSION => return Ok(candidate),
            Some(_) => saw_unsupported_version = true,
            None => {}
        }
    }

    if saw_unsupported_version {
        Err(UsageError::UnsupportedCli)
    } else {
        Err(UsageError::CliNotFound)
    }
}

pub(crate) const fn candidate_priority(kind: CandidateKind) -> u8 {
    match kind {
        CandidateKind::NativeExe => 0,
        CandidateKind::Direct => 1,
        CandidateKind::Command => 2,
        CandidateKind::PowerShell => 3,
    }
}

pub(crate) fn parse_version(output: &str) -> Option<(u64, u64, u64)> {
    let token = output.split_whitespace().find(|token| {
        token
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_digit())
    })?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn gather_candidates(deadline: Instant) -> Result<Vec<CliCandidate>, UsageError> {
    let mut paths = HashSet::new();
    let mut candidates = Vec::new();
    for path in where_candidates(deadline)?
        .into_iter()
        .chain(path_candidates())
    {
        ensure_deadline(deadline)?;
        let Some(kind) = CandidateKind::from_path(&path) else {
            continue;
        };
        let key = path.to_string_lossy().to_ascii_lowercase();
        if paths.insert(key) {
            candidates.push(CliCandidate { path, kind });
        }
    }
    Ok(candidates)
}

fn where_candidates(deadline: Instant) -> Result<Vec<PathBuf>, UsageError> {
    let mut paths = Vec::new();
    for name in ["codex.exe", "codex", "codex.cmd", "codex.ps1"] {
        ensure_deadline(deadline)?;
        let mut command = Command::new("where.exe");
        command
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let Ok(mut child) = command.spawn() else {
            continue;
        };
        while child.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                return Err(UsageError::RpcTimeout);
            }
            thread::sleep(Duration::from_millis(10));
        }
        let Ok(output) = child.wait_with_output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        paths.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from),
        );
    }
    Ok(paths)
}

fn path_candidates() -> Vec<PathBuf> {
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .flat_map(|directory| {
            ["codex.exe", "codex", "codex.cmd", "codex.ps1"]
                .into_iter()
                .map(move |name| directory.join(name))
        })
        .filter(|path| path.is_file())
        .collect()
}

fn verify_version(
    candidate: &CliCandidate,
    deadline: Instant,
) -> Result<Option<(u64, u64, u64)>, UsageError> {
    ensure_deadline(deadline)?;
    let probe_deadline = deadline.min(Instant::now() + VERSION_TIMEOUT);
    let plan = version_plan(candidate.kind, candidate.path.clone());
    match ProcessGuard::version_output(plan, probe_deadline) {
        Ok(output) => Ok(String::from_utf8(output)
            .ok()
            .and_then(|output| parse_version(&output))),
        Err(UsageError::RpcTimeout) if Instant::now() < deadline => Ok(None),
        Err(UsageError::RpcTimeout) => Err(UsageError::RpcTimeout),
        Err(_) => Ok(None),
    }
}

fn ensure_deadline(deadline: Instant) -> Result<(), UsageError> {
    if Instant::now() >= deadline {
        Err(UsageError::RpcTimeout)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{candidate_priority, parse_version, CandidateKind};

    #[test]
    fn candidates_prefer_native_then_direct_then_command_then_powershell() {
        assert!(
            candidate_priority(CandidateKind::NativeExe)
                < candidate_priority(CandidateKind::Direct)
        );
        assert!(
            candidate_priority(CandidateKind::Direct) < candidate_priority(CandidateKind::Command)
        );
        assert!(
            candidate_priority(CandidateKind::Command)
                < candidate_priority(CandidateKind::PowerShell)
        );
    }

    #[test]
    fn version_parser_accepts_codex_prefix_and_rejects_invalid_output() {
        assert_eq!(parse_version("codex 0.141.0\n"), Some((0, 141, 0)));
        assert_eq!(parse_version("0.142.1"), Some((0, 142, 1)));
        assert_eq!(parse_version("codex development"), None);
    }

    #[test]
    fn candidate_kind_is_derived_from_the_supported_file_names() {
        assert_eq!(
            CandidateKind::from_path(&PathBuf::from("C:/bin/codex.exe")),
            Some(CandidateKind::NativeExe)
        );
        assert_eq!(
            CandidateKind::from_path(&PathBuf::from("C:/bin/codex.cmd")),
            Some(CandidateKind::Command)
        );
        assert_eq!(
            CandidateKind::from_path(&PathBuf::from("C:/bin/codex.ps1")),
            Some(CandidateKind::PowerShell)
        );
        assert_eq!(
            CandidateKind::from_path(&PathBuf::from("C:/bin&unsafe/codex.cmd")),
            None
        );
    }
}
