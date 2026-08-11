use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use codex_usage_monitor::{
    apply_update_helper, fetch_available_self_update, prepare_update_helper, release_api_url,
    signal_restart_ready, take_restart_ready_argument, AvailableUpdate, DownloadResponse,
    HelperOutcome, HelperPlan, SelfUpdateError, SelfUpdateHttpClient, SelfUpdatePlatform,
    SelfUpdateProcess,
};
use semver::Version;
use sha2::{Digest, Sha256};

struct TestRoot(PathBuf);

impl TestRoot {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "codex-peek-self-update-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct FakeHttpClient {
    responses: HashMap<String, DownloadResponse>,
}

impl SelfUpdateHttpClient for FakeHttpClient {
    fn get(&self, url: &str, max_bytes: usize) -> Result<DownloadResponse, SelfUpdateError> {
        let response = self
            .responses
            .get(url)
            .cloned()
            .ok_or(SelfUpdateError::Network)?;
        if response.body.len() > max_bytes {
            return Err(SelfUpdateError::TooLarge);
        }
        Ok(response)
    }
}

fn release_fixture(executable: &[u8]) -> (FakeHttpClient, AvailableUpdate) {
    let version = Version::parse("1.2.3").unwrap();
    let tag = "v1.2.3";
    let release_url = "https://github.com/lch5518/CodexPeek/releases/tag/v1.2.3";
    let executable_name = "codex-peek-v1.2.3-windows-x86_64.exe";
    let executable_url =
        format!("https://github.com/lch5518/CodexPeek/releases/download/{tag}/{executable_name}");
    let checksum_url =
        "https://github.com/lch5518/CodexPeek/releases/download/v1.2.3/SHA256SUMS.txt";
    let digest: [u8; 32] = Sha256::digest(executable).into();
    let checksum_hex = digest
        .iter()
        .fold(String::with_capacity(64), |mut checksum_hex, byte| {
            write!(&mut checksum_hex, "{byte:02x}").unwrap();
            checksum_hex
        });
    let checksum = format!("{checksum_hex}  {executable_name}\n").into_bytes();
    let metadata = serde_json::to_vec(&serde_json::json!({
        "tag_name": tag,
        "html_url": release_url,
        "draft": false,
        "prerelease": false,
        "assets": [
            {
                "name": executable_name,
                "browser_download_url": executable_url,
                "size": executable.len(),
                "state": "uploaded"
            },
            {
                "name": "SHA256SUMS.txt",
                "browser_download_url": checksum_url,
                "size": checksum.len(),
                "state": "uploaded"
            }
        ]
    }))
    .unwrap();
    let mut responses = HashMap::new();
    let api_url = release_api_url();
    responses.insert(
        api_url.clone(),
        DownloadResponse {
            status: 200,
            final_url: api_url,
            body: metadata,
        },
    );
    responses.insert(
        executable_url.clone(),
        DownloadResponse {
            status: 200,
            final_url: executable_url,
            body: executable.to_vec(),
        },
    );
    responses.insert(
        checksum_url.to_string(),
        DownloadResponse {
            status: 200,
            final_url: checksum_url.to_string(),
            body: checksum,
        },
    );
    (
        FakeHttpClient { responses },
        AvailableUpdate {
            version,
            release_url: release_url.to_string(),
        },
    )
}

#[test]
fn official_metadata_and_checksum_prepare_a_unicode_safe_helper_plan() {
    let root = TestRoot::new("prepare-한글");
    let executable = b"new codex-peek executable";
    let (client, selected) = release_fixture(executable);
    let release = fetch_available_self_update(&client, "1.0.0")
        .unwrap()
        .unwrap();
    assert_eq!(release.version, selected.version);

    let current = root.0.join("codex-peek.exe");
    fs::write(&current, b"old codex-peek executable").unwrap();
    let prepared = prepare_update_helper(
        &client,
        &selected,
        "1.0.0",
        &current,
        vec![OsString::from("--startup")],
    )
    .unwrap();

    assert_eq!(fs::read(&prepared.plan.staged).unwrap(), executable);
    assert_eq!(
        fs::read(&prepared.helper_path).unwrap(),
        b"old codex-peek executable"
    );
    assert_eq!(
        HelperPlan::decode_arguments(prepared.plan.encode_arguments()).unwrap(),
        Some(prepared.plan.clone())
    );
    let mut invalid_relaunch = prepared.plan.clone();
    invalid_relaunch.relaunch_args = vec![OsString::from("--diagnose")];
    assert_eq!(
        HelperPlan::decode_arguments(invalid_relaunch.encode_arguments()),
        Err(SelfUpdateError::InvalidPlan)
    );
    assert!(!prepared.plan.ready.exists());
    assert!(!prepared.plan.restart_ready.exists());

    fs::remove_file(&prepared.plan.staged).unwrap();
    fs::remove_file(&prepared.helper_path).unwrap();
}

#[test]
fn selected_release_must_still_match_the_official_latest_release() {
    let root = TestRoot::new("selection");
    let (client, mut selected) = release_fixture(b"new executable");
    selected.version = Version::parse("1.2.4").unwrap();
    let current = root.0.join("codex-peek.exe");
    fs::write(&current, b"old executable").unwrap();

    assert_eq!(
        prepare_update_helper(&client, &selected, "1.0.0", &current, Vec::new()),
        Err(SelfUpdateError::SelectionMismatch)
    );
}

#[test]
fn official_metadata_rejects_a_same_named_asset_from_an_untrusted_url() {
    let (mut client, _) = release_fixture(b"new executable");
    let api_url = release_api_url();
    let response = client.responses.get_mut(&api_url).unwrap();
    let mut metadata: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    metadata["assets"][0]["browser_download_url"] =
        serde_json::Value::String("https://example.test/codex-peek.exe".to_string());
    response.body = serde_json::to_vec(&metadata).unwrap();

    assert_eq!(
        fetch_available_self_update(&client, "1.0.0"),
        Err(SelfUpdateError::InvalidMetadata)
    );
}

#[test]
fn checksum_mismatch_never_creates_a_staging_file() {
    let root = TestRoot::new("bad-checksum");
    let (mut client, selected) = release_fixture(b"new executable");
    let release = fetch_available_self_update(&client, "1.0.0")
        .unwrap()
        .unwrap();
    client
        .responses
        .get_mut(&release.executable.download_url)
        .unwrap()
        .body = b"bad executable".to_vec();
    let staging = root.0.join("codex-peek.update");

    assert_eq!(
        codex_usage_monitor::download_verified_executable(&client, &release, &staging),
        Err(SelfUpdateError::Integrity)
    );
    assert!(!staging.exists());
    assert_eq!(selected.version, release.version);
}

struct RecordingPlatform {
    calls: Arc<Mutex<Vec<&'static str>>>,
    relaunch_count: AtomicUsize,
    fail_first_relaunch: bool,
    fail_replace: bool,
    fail_readiness: bool,
    fail_termination: bool,
}

impl RecordingPlatform {
    fn success_fixture() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            relaunch_count: AtomicUsize::new(0),
            fail_first_relaunch: false,
            fail_replace: false,
            fail_readiness: false,
            fail_termination: false,
        }
    }

    fn rollback_fixture() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            relaunch_count: AtomicUsize::new(0),
            fail_first_relaunch: true,
            fail_replace: false,
            fail_readiness: false,
            fail_termination: false,
        }
    }

    fn replace_failure_fixture() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            relaunch_count: AtomicUsize::new(0),
            fail_first_relaunch: false,
            fail_replace: true,
            fail_readiness: false,
            fail_termination: false,
        }
    }

    fn readiness_failure_fixture() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            relaunch_count: AtomicUsize::new(0),
            fail_first_relaunch: false,
            fail_replace: false,
            fail_readiness: true,
            fail_termination: false,
        }
    }

    fn termination_failure_fixture() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            relaunch_count: AtomicUsize::new(0),
            fail_first_relaunch: false,
            fail_replace: false,
            fail_readiness: true,
            fail_termination: true,
        }
    }
}

struct RecordingProcess {
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail_readiness: bool,
    fail_termination: bool,
}

impl SelfUpdateProcess for RecordingProcess {
    fn wait_for_restart_ready(
        &mut self,
        _restart_ready: &Path,
        _timeout: Duration,
    ) -> Result<(), SelfUpdateError> {
        self.calls.lock().unwrap().push("wait_ready");
        if self.fail_readiness {
            Err(SelfUpdateError::RelaunchFailed)
        } else {
            Ok(())
        }
    }

    fn terminate_and_wait(&mut self) -> Result<(), SelfUpdateError> {
        self.calls.lock().unwrap().push("terminate");
        if self.fail_termination {
            Err(SelfUpdateError::TerminationFailed)
        } else {
            Ok(())
        }
    }
}

impl SelfUpdatePlatform for RecordingPlatform {
    fn wait_for_exit(&self, _process_id: u32, _timeout: Duration) -> Result<(), SelfUpdateError> {
        self.calls.lock().unwrap().push("wait");
        Ok(())
    }

    fn replace_with_backup(
        &self,
        _target: &Path,
        _staged: &Path,
        _backup: &Path,
    ) -> Result<(), SelfUpdateError> {
        self.calls.lock().unwrap().push("replace");
        if self.fail_replace {
            Err(SelfUpdateError::ReplaceFailed)
        } else {
            Ok(())
        }
    }

    fn rollback(&self, _target: &Path, _backup: &Path) -> Result<(), SelfUpdateError> {
        self.calls.lock().unwrap().push("rollback");
        Ok(())
    }

    fn relaunch(
        &self,
        _target: &Path,
        _arguments: &[OsString],
        _restart_ready: Option<&Path>,
    ) -> Result<Box<dyn SelfUpdateProcess>, SelfUpdateError> {
        self.calls.lock().unwrap().push("relaunch");
        let count = self.relaunch_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_first_relaunch && count == 0 {
            Err(SelfUpdateError::RelaunchFailed)
        } else {
            Ok(Box::new(RecordingProcess {
                calls: self.calls.clone(),
                fail_readiness: self.fail_readiness,
                fail_termination: self.fail_termination,
            }))
        }
    }

    fn remove_backup(&self, _backup: &Path) -> Result<(), SelfUpdateError> {
        self.calls.lock().unwrap().push("remove_backup");
        Ok(())
    }
}

#[test]
fn helper_rolls_back_and_relaunches_the_old_binary_when_new_launch_fails() {
    let root = TestRoot::new("rollback");
    let target = root.0.join("codex-peek.exe");
    let staged = root.0.join("codex-peek.update");
    let backup = root.0.join("codex-peek.backup.exe");
    let ready = root.0.join("helper.ready");
    let bytes = b"verified replacement";
    fs::write(&staged, bytes).unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target,
        staged,
        backup,
        ready,
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: Sha256::digest(bytes).into(),
        expected_size: bytes.len() as u64,
        relaunch_args: vec![OsString::from("--startup")],
    };
    let platform = RecordingPlatform::rollback_fixture();

    assert_eq!(
        apply_update_helper(&platform, &plan),
        Ok(HelperOutcome::RolledBack)
    );
    assert_eq!(
        *platform.calls.lock().unwrap(),
        ["wait", "replace", "relaunch", "rollback", "relaunch"]
    );
}

#[test]
fn helper_relaunches_the_old_binary_when_atomic_replacement_fails() {
    let root = TestRoot::new("replace-failure");
    let staged = root.0.join("codex-peek.update");
    let bytes = b"verified replacement";
    fs::write(&staged, bytes).unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target: root.0.join("codex-peek.exe"),
        staged,
        backup: root.0.join("codex-peek.backup.exe"),
        ready: root.0.join("helper.ready"),
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: Sha256::digest(bytes).into(),
        expected_size: bytes.len() as u64,
        relaunch_args: Vec::new(),
    };
    let platform = RecordingPlatform::replace_failure_fixture();

    assert_eq!(
        apply_update_helper(&platform, &plan),
        Ok(HelperOutcome::RolledBack)
    );
    assert_eq!(
        *platform.calls.lock().unwrap(),
        ["wait", "replace", "relaunch"]
    );
}

#[test]
fn restart_handshake_is_private_trailing_state_and_preserves_startup_mode() {
    let ready_root = std::env::temp_dir().join("CodexPeek").join("updates");
    fs::create_dir_all(&ready_root).unwrap();
    let ready = ready_root.join(format!(
        "codex-peek-update-restart-test-{}-{}.ready",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut arguments = vec![
        OsString::from("--startup"),
        OsString::from("--self-update-restart-ready"),
        ready.as_os_str().to_owned(),
    ];

    assert_eq!(
        take_restart_ready_argument(&mut arguments),
        Ok(Some(ready.clone()))
    );
    assert_eq!(arguments, [OsString::from("--startup")]);
    assert_eq!(signal_restart_ready(&ready), Ok(()));
    assert_eq!(fs::read(&ready).unwrap(), b"ready\n");
    assert_eq!(
        signal_restart_ready(&ready),
        Err(SelfUpdateError::InvalidPlan)
    );
    fs::remove_file(ready).unwrap();
}

#[test]
fn helper_keeps_backup_until_the_new_app_reports_ui_readiness() {
    let root = TestRoot::new("restart-ready");
    let staged = root.0.join("codex-peek.update");
    let bytes = b"verified replacement";
    fs::write(&staged, bytes).unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target: root.0.join("codex-peek.exe"),
        staged,
        backup: root.0.join("codex-peek.backup.exe"),
        ready: root.0.join("helper.ready"),
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: Sha256::digest(bytes).into(),
        expected_size: bytes.len() as u64,
        relaunch_args: vec![OsString::from("--startup")],
    };
    let platform = RecordingPlatform::success_fixture();

    assert_eq!(
        apply_update_helper(&platform, &plan),
        Ok(HelperOutcome::Updated)
    );
    assert_eq!(
        *platform.calls.lock().unwrap(),
        ["wait", "replace", "relaunch", "wait_ready", "remove_backup"]
    );
}

#[test]
fn helper_terminates_and_rolls_back_when_the_new_app_never_becomes_ready() {
    let root = TestRoot::new("restart-not-ready");
    let staged = root.0.join("codex-peek.update");
    let bytes = b"verified replacement";
    fs::write(&staged, bytes).unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target: root.0.join("codex-peek.exe"),
        staged,
        backup: root.0.join("codex-peek.backup.exe"),
        ready: root.0.join("helper.ready"),
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: Sha256::digest(bytes).into(),
        expected_size: bytes.len() as u64,
        relaunch_args: Vec::new(),
    };
    let platform = RecordingPlatform::readiness_failure_fixture();

    assert_eq!(
        apply_update_helper(&platform, &plan),
        Ok(HelperOutcome::RolledBack)
    );
    assert_eq!(
        *platform.calls.lock().unwrap(),
        [
            "wait",
            "replace",
            "relaunch",
            "wait_ready",
            "terminate",
            "rollback",
            "relaunch"
        ]
    );
}

#[test]
fn helper_preserves_backup_and_stops_when_new_process_termination_is_unconfirmed() {
    let root = TestRoot::new("termination-unconfirmed");
    let staged = root.0.join("codex-peek.update");
    let bytes = b"verified replacement";
    fs::write(&staged, bytes).unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target: root.0.join("codex-peek.exe"),
        staged,
        backup: root.0.join("codex-peek.backup.exe"),
        ready: root.0.join("helper.ready"),
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: Sha256::digest(bytes).into(),
        expected_size: bytes.len() as u64,
        relaunch_args: Vec::new(),
    };
    let platform = RecordingPlatform::termination_failure_fixture();

    assert_eq!(
        apply_update_helper(&platform, &plan),
        Err(SelfUpdateError::TerminationFailed)
    );
    assert_eq!(
        *platform.calls.lock().unwrap(),
        ["wait", "replace", "relaunch", "wait_ready", "terminate"]
    );
}

#[test]
fn helper_refuses_a_changed_staging_file_before_waiting_for_the_parent() {
    let root = TestRoot::new("integrity");
    let staged = root.0.join("codex-peek.update");
    fs::write(&staged, b"changed").unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target: root.0.join("codex-peek.exe"),
        staged,
        backup: root.0.join("codex-peek.backup.exe"),
        ready: root.0.join("helper.ready"),
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: [0; 32],
        expected_size: 7,
        relaunch_args: Vec::new(),
    };
    let platform = RecordingPlatform::rollback_fixture();

    assert_eq!(
        apply_update_helper(&platform, &plan),
        Err(SelfUpdateError::Integrity)
    );
    assert!(platform.calls.lock().unwrap().is_empty());
}

struct TamperingPlatform {
    staged: PathBuf,
    relaunched: AtomicUsize,
}

struct UnusedProcess;

impl SelfUpdateProcess for UnusedProcess {
    fn wait_for_restart_ready(
        &mut self,
        _restart_ready: &Path,
        _timeout: Duration,
    ) -> Result<(), SelfUpdateError> {
        panic!("old-binary relaunch must not wait for readiness")
    }

    fn terminate_and_wait(&mut self) -> Result<(), SelfUpdateError> {
        panic!("old-binary relaunch must not be terminated")
    }
}

impl SelfUpdatePlatform for TamperingPlatform {
    fn wait_for_exit(&self, _process_id: u32, _timeout: Duration) -> Result<(), SelfUpdateError> {
        fs::write(&self.staged, b"tampered after first verification").unwrap();
        Ok(())
    }

    fn replace_with_backup(
        &self,
        _target: &Path,
        _staged: &Path,
        _backup: &Path,
    ) -> Result<(), SelfUpdateError> {
        panic!("replacement must not run after staging tampering")
    }

    fn rollback(&self, _target: &Path, _backup: &Path) -> Result<(), SelfUpdateError> {
        panic!("rollback must not run before replacement")
    }

    fn relaunch(
        &self,
        _target: &Path,
        _arguments: &[OsString],
        restart_ready: Option<&Path>,
    ) -> Result<Box<dyn SelfUpdateProcess>, SelfUpdateError> {
        assert!(restart_ready.is_none());
        self.relaunched.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(UnusedProcess))
    }

    fn remove_backup(&self, _backup: &Path) -> Result<(), SelfUpdateError> {
        panic!("backup must not be removed after staging tampering")
    }
}

#[test]
fn helper_rechecks_staging_integrity_after_the_parent_exits() {
    let root = TestRoot::new("integrity-after-wait");
    let staged = root.0.join("codex-peek.update");
    let bytes = b"verified replacement";
    fs::write(&staged, bytes).unwrap();
    let plan = HelperPlan {
        parent_pid: 42,
        target: root.0.join("codex-peek.exe"),
        staged: staged.clone(),
        backup: root.0.join("codex-peek.backup.exe"),
        ready: root.0.join("helper.ready"),
        restart_ready: root.0.join("restart.ready"),
        expected_sha256: Sha256::digest(bytes).into(),
        expected_size: bytes.len() as u64,
        relaunch_args: Vec::new(),
    };

    let platform = TamperingPlatform {
        staged,
        relaunched: AtomicUsize::new(0),
    };
    assert_eq!(
        apply_update_helper(&platform, &plan),
        Ok(HelperOutcome::RolledBack)
    );
    assert_eq!(platform.relaunched.load(Ordering::Relaxed), 1);
}
