use std::{
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use ureq::{
    tls::{TlsConfig, TlsProvider},
    ResponseExt,
};

const RELEASE_OWNER: &str = "lch5518";
const RELEASE_REPOSITORY: &str = "CodexPeek";
const CHECKSUM_ASSET_NAME: &str = "SHA256SUMS.txt";
const USER_AGENT: &str = "CodexPeek self-update";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HELPER_WAIT_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_METADATA_BYTES: usize = 256 * 1024;
const MAX_CHECKSUM_BYTES: usize = 64 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 64 * 1024 * 1024;
const HELPER_MODE_ARGUMENT: &str = "--self-update-helper";
const RESTART_READY_ARGUMENT: &str = "--self-update-restart-ready";
const HELPER_READY_TIMEOUT: Duration = Duration::from_secs(5);
const RESTART_READY_TIMEOUT: Duration = Duration::from_secs(15);
static UPDATE_NONCE: AtomicU64 = AtomicU64::new(0);

/// 자동 업데이트 준비 또는 적용이 안전하게 완료되지 못한 이유입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfUpdateError {
    /// 네트워크 요청이 실패했거나 성공 상태가 아니었습니다.
    Network,
    /// 공식 릴리스 메타데이터가 예상한 계약과 일치하지 않습니다.
    InvalidMetadata,
    /// 사용자가 선택한 버전과 다시 조회한 공식 latest release가 일치하지 않습니다.
    SelectionMismatch,
    /// 응답이나 릴리스 파일이 허용 크기를 초과했습니다.
    TooLarge,
    /// 다운로드한 실행 파일의 길이 또는 SHA-256이 일치하지 않습니다.
    Integrity,
    /// 파일 준비 또는 정리 작업이 실패했습니다.
    Io,
    /// 헬퍼가 받은 경로 또는 프로세스 정보가 안전한 형태가 아닙니다.
    InvalidPlan,
    /// 기존 프로세스가 제한 시간 안에 종료되지 않았습니다.
    WaitFailed,
    /// 검증된 실행 파일로 원자 교체하지 못했습니다.
    ReplaceFailed,
    /// 새 실행 파일을 시작하지 못했습니다.
    RelaunchFailed,
    /// 별도 업데이트 헬퍼를 시작하거나 준비 완료를 확인하지 못했습니다.
    HelperStartFailed,
    /// 새 실행 파일 시작 실패 후 기존 실행 파일 복원에 실패했습니다.
    RollbackFailed,
    /// readiness 실패 후 새 프로세스의 종료를 확인하지 못했습니다.
    TerminationFailed,
}

/// 크기 제한을 적용해 내려받은 HTTPS 응답입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadResponse {
    /// HTTP 상태 코드입니다.
    pub status: u16,
    /// redirect가 끝난 최종 HTTPS URL입니다.
    pub final_url: String,
    /// 제한된 크기로 읽은 응답 본문입니다.
    pub body: Vec<u8>,
}

/// self-update에 필요한 최소 HTTP 경계입니다.
pub trait SelfUpdateHttpClient: Send + Sync {
    /// HTTPS URL을 제한된 시간과 크기로 내려받습니다.
    fn get(&self, url: &str, max_bytes: usize) -> Result<DownloadResponse, SelfUpdateError>;
}

/// Windows 신뢰 저장소를 사용하는 실제 self-update HTTP 클라이언트입니다.
#[derive(Clone, Copy, Debug, Default)]
pub struct UreqSelfUpdateHttpClient;

impl SelfUpdateHttpClient for UreqSelfUpdateHttpClient {
    fn get(&self, url: &str, max_bytes: usize) -> Result<DownloadResponse, SelfUpdateError> {
        if !is_https_url(url) || max_bytes == 0 {
            return Err(SelfUpdateError::Network);
        }
        let max_bytes = u64::try_from(max_bytes).map_err(|_| SelfUpdateError::TooLarge)?;
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .max_redirects(5)
            .max_redirects_will_error(true)
            .user_agent(USER_AGENT)
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .build(),
            )
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent
            .get(url)
            .call()
            .map_err(|_| SelfUpdateError::Network)?;
        let status = response.status().as_u16();
        let final_url = response.get_uri().to_string();
        if status / 100 != 2 || !is_trusted_download_url(&final_url) {
            return Err(SelfUpdateError::Network);
        }
        let body = response
            .body_mut()
            .with_config()
            .limit(max_bytes)
            .read_to_vec()
            .map_err(|_| SelfUpdateError::TooLarge)?;
        if u64::try_from(body.len()).map_err(|_| SelfUpdateError::TooLarge)? > max_bytes {
            return Err(SelfUpdateError::TooLarge);
        }
        Ok(DownloadResponse {
            status,
            final_url,
            body,
        })
    }
}

/// 검증된 GitHub Release asset 메타데이터입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    /// 릴리스에 게시된 정확한 파일명입니다.
    pub name: String,
    /// 공식 GitHub release download HTTPS URL입니다.
    pub download_url: String,
    /// GitHub 메타데이터에 게시된 바이트 길이입니다.
    pub size: u64,
}

/// 다운로드 가능한 공식 CodexPeek self-update 릴리스입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfUpdateRelease {
    /// 현재 실행 버전보다 높은 안정 SemVer입니다.
    pub version: Version,
    /// `v<version>` 형식의 GitHub 태그입니다.
    pub tag_name: String,
    /// 브라우저에서 표시할 공식 릴리스 페이지입니다.
    pub release_url: String,
    /// 교체에 사용할 raw Windows x64 실행 파일입니다.
    pub executable: ReleaseAsset,
    /// 실행 파일 해시를 제공하는 manifest입니다.
    pub checksums: ReleaseAsset,
}

/// 현재 버전과 비교할 공식 GitHub latest-release API URL을 반환합니다.
pub fn release_api_url() -> String {
    format!("https://api.github.com/repos/{RELEASE_OWNER}/{RELEASE_REPOSITORY}/releases/latest")
}

/// 공식 latest-release 메타데이터를 조회하고 더 높은 버전의 self-update만 반환합니다.
pub fn fetch_available_self_update(
    client: &dyn SelfUpdateHttpClient,
    current_version: &str,
) -> Result<Option<SelfUpdateRelease>, SelfUpdateError> {
    let current_version =
        Version::parse(current_version).map_err(|_| SelfUpdateError::InvalidMetadata)?;
    let api_url = release_api_url();
    let response = client.get(&api_url, MAX_METADATA_BYTES)?;
    if response.status / 100 != 2 || response.final_url != api_url {
        return Err(SelfUpdateError::InvalidMetadata);
    }
    validate_release_metadata(&current_version, &response.body)
}

#[derive(Deserialize)]
struct ReleaseDto {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAssetDto>,
}

#[derive(Deserialize)]
struct ReleaseAssetDto {
    name: String,
    browser_download_url: String,
    size: u64,
    state: String,
}

fn validate_release_metadata(
    current_version: &Version,
    body: &[u8],
) -> Result<Option<SelfUpdateRelease>, SelfUpdateError> {
    let release: ReleaseDto =
        serde_json::from_slice(body).map_err(|_| SelfUpdateError::InvalidMetadata)?;
    if release.draft || release.prerelease {
        return Err(SelfUpdateError::InvalidMetadata);
    }
    let version_text = release
        .tag_name
        .strip_prefix('v')
        .ok_or(SelfUpdateError::InvalidMetadata)?;
    let version = Version::parse(version_text).map_err(|_| SelfUpdateError::InvalidMetadata)?;
    if version.pre.is_empty()
        && version.build.is_empty()
        && release.tag_name == format!("v{version}")
        && release.html_url
            == format!(
                "https://github.com/{RELEASE_OWNER}/{RELEASE_REPOSITORY}/releases/tag/{}",
                release.tag_name
            )
    {
        if version <= *current_version {
            return Ok(None);
        }
    } else {
        return Err(SelfUpdateError::InvalidMetadata);
    }

    let executable_name = raw_executable_asset_name(&version);
    let executable = exact_asset(&release.assets, &release.tag_name, &executable_name)?;
    let checksums = exact_asset(&release.assets, &release.tag_name, CHECKSUM_ASSET_NAME)?;
    if executable.size == 0
        || executable.size > MAX_EXECUTABLE_BYTES
        || checksums.size == 0
        || checksums.size > MAX_CHECKSUM_BYTES as u64
    {
        return Err(SelfUpdateError::TooLarge);
    }
    Ok(Some(SelfUpdateRelease {
        version,
        tag_name: release.tag_name,
        release_url: release.html_url,
        executable,
        checksums,
    }))
}

fn exact_asset(
    assets: &[ReleaseAssetDto],
    tag_name: &str,
    expected_name: &str,
) -> Result<ReleaseAsset, SelfUpdateError> {
    let matching = assets
        .iter()
        .filter(|asset| asset.name == expected_name)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(SelfUpdateError::InvalidMetadata);
    }
    let asset = matching[0];
    let expected_url = format!(
        "https://github.com/{RELEASE_OWNER}/{RELEASE_REPOSITORY}/releases/download/{tag_name}/{expected_name}"
    );
    if asset.state != "uploaded" || asset.browser_download_url != expected_url {
        return Err(SelfUpdateError::InvalidMetadata);
    }
    Ok(ReleaseAsset {
        name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        size: asset.size,
    })
}

fn raw_executable_asset_name(version: &Version) -> String {
    format!("codex-peek-v{version}-windows-x86_64.exe")
}

/// 검증을 마치고 대상 경로에 동기화한 교체 후보 실행 파일입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedExecutable {
    /// 새로 만든 staging 파일 경로입니다.
    pub path: PathBuf,
    /// 헬퍼가 교체 직전에 다시 확인할 SHA-256입니다.
    pub sha256: [u8; 32],
    /// 검증된 파일 길이입니다.
    pub size: u64,
}

/// 다운로드·검증과 임시 파일 준비를 끝내고 헬퍼 시작만 남은 업데이트입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedUpdateHelper {
    /// 다시 검증한 공식 최신 버전입니다.
    pub version: Version,
    /// 현재 실행 파일을 복사해 만든 임시 helper EXE입니다.
    pub helper_path: PathBuf,
    /// helper mode에 전달할 원자 교체 계획입니다.
    pub plan: HelperPlan,
}

/// 준비 완료를 확인한 별도 업데이트 헬퍼입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnedUpdateHelper {
    /// helper가 설치할 버전입니다.
    pub version: Version,
    /// 종료 후 정리 대상으로 예약할 임시 helper EXE 경로입니다.
    pub helper_path: PathBuf,
}

/// UI가 선택한 업데이트를 공식 latest release와 다시 대조하고 모든 교체 파일을 준비합니다.
///
/// 네트워크와 파일 I/O는 호출 스레드에서 실행되므로 UI는 이 함수를 worker thread에서 호출해야
/// 합니다. 성공하더라도 현재 프로세스나 기존 실행 파일은 변경하지 않습니다.
pub fn prepare_update_helper(
    client: &dyn SelfUpdateHttpClient,
    selected: &crate::AvailableUpdate,
    current_version: &str,
    current_executable: &Path,
    relaunch_args: Vec<OsString>,
) -> Result<PreparedUpdateHelper, SelfUpdateError> {
    let release = fetch_available_self_update(client, current_version)?
        .ok_or(SelfUpdateError::SelectionMismatch)?;
    if release.version != selected.version || release.release_url != selected.release_url {
        return Err(SelfUpdateError::SelectionMismatch);
    }
    if !current_executable.is_absolute() || !current_executable.is_file() {
        return Err(SelfUpdateError::InvalidPlan);
    }
    let target_parent = current_executable
        .parent()
        .ok_or(SelfUpdateError::InvalidPlan)?;
    let target_name = current_executable
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(SelfUpdateError::InvalidPlan)?;
    let nonce = update_nonce();
    let staged = target_parent.join(format!(".{target_name}.update-{nonce}.tmp"));
    let backup = target_parent.join(format!(".{target_name}.backup-{nonce}.exe"));
    let helper_root = std::env::temp_dir().join("CodexPeek").join("updates");
    fs::create_dir_all(&helper_root).map_err(|_| SelfUpdateError::Io)?;
    let helper_path = helper_root.join(format!("codex-peek-update-helper-{nonce}.exe"));
    let ready_path = helper_root.join(format!("codex-peek-update-helper-{nonce}.ready"));
    let restart_ready_path = helper_root.join(format!("codex-peek-update-restart-{nonce}.ready"));

    let prepared = (|| {
        let verified = download_verified_executable(client, &release, &staged)?;
        copy_file_exclusively(current_executable, &helper_path)?;
        Ok(PreparedUpdateHelper {
            version: release.version,
            helper_path: helper_path.clone(),
            plan: HelperPlan {
                parent_pid: std::process::id(),
                target: current_executable.to_path_buf(),
                staged: verified.path,
                backup,
                ready: ready_path.clone(),
                restart_ready: restart_ready_path.clone(),
                expected_sha256: verified.sha256,
                expected_size: verified.size,
                relaunch_args,
            },
        })
    })();
    if prepared.is_err() {
        remove_file_if_present(&staged);
        remove_file_if_present(&helper_path);
        remove_file_if_present(&ready_path);
        remove_file_if_present(&restart_ready_path);
    }
    prepared
}

/// 준비된 helper mode 프로세스를 시작하고 준비 완료 신호를 제한 시간 동안 기다립니다.
///
/// 성공한 뒤 호출자는 기존 앱을 정상 종료해야 합니다. helper가 기존 프로세스 종료를 기다린 뒤
/// 교체·rollback·재실행을 수행합니다.
pub fn spawn_prepared_update_helper(
    prepared: PreparedUpdateHelper,
) -> Result<SpawnedUpdateHelper, SelfUpdateError> {
    let arguments = prepared.plan.encode_arguments();
    let mut helper = match Command::new(&prepared.helper_path).args(arguments).spawn() {
        Ok(helper) => helper,
        Err(_) => {
            cleanup_unstarted_update(&prepared);
            return Err(SelfUpdateError::HelperStartFailed);
        }
    };
    let deadline = std::time::Instant::now() + HELPER_READY_TIMEOUT;
    while std::time::Instant::now() < deadline {
        match helper.try_wait() {
            Ok(Some(_)) => {
                cleanup_unstarted_update(&prepared);
                return Err(SelfUpdateError::HelperStartFailed);
            }
            Ok(None) => {}
            Err(_) => {
                let _ = helper.kill();
                let _ = helper.wait();
                cleanup_unstarted_update(&prepared);
                return Err(SelfUpdateError::HelperStartFailed);
            }
        }
        if ready_file_exists(&prepared.plan.ready) {
            return Ok(SpawnedUpdateHelper {
                version: prepared.version,
                helper_path: prepared.helper_path,
            });
        }
        thread::sleep(Duration::from_millis(25));
    }
    let _ = helper.kill();
    let _ = helper.wait();
    cleanup_unstarted_update(&prepared);
    Err(SelfUpdateError::HelperStartFailed)
}

/// 선택 검증, 다운로드, helper 복사와 helper 시작을 한 번에 수행합니다.
pub fn prepare_and_spawn_update_helper(
    client: &dyn SelfUpdateHttpClient,
    selected: &crate::AvailableUpdate,
    current_version: &str,
    current_executable: &Path,
    relaunch_args: Vec<OsString>,
) -> Result<SpawnedUpdateHelper, SelfUpdateError> {
    let prepared = prepare_update_helper(
        client,
        selected,
        current_version,
        current_executable,
        relaunch_args,
    )?;
    spawn_prepared_update_helper(prepared)
}

/// checksum manifest와 raw EXE를 제한된 크기로 내려받아 새 staging 파일로 저장합니다.
pub fn download_verified_executable(
    client: &dyn SelfUpdateHttpClient,
    release: &SelfUpdateRelease,
    staging_path: &Path,
) -> Result<VerifiedExecutable, SelfUpdateError> {
    if !valid_staging_path(staging_path) {
        return Err(SelfUpdateError::InvalidPlan);
    }
    let manifest = client.get(&release.checksums.download_url, MAX_CHECKSUM_BYTES)?;
    validate_asset_response(&manifest, &release.checksums)?;
    let expected_sha256 = checksum_for_asset(&manifest.body, &release.executable.name)?;

    let executable_limit =
        usize::try_from(release.executable.size).map_err(|_| SelfUpdateError::TooLarge)?;
    let executable = client.get(&release.executable.download_url, executable_limit)?;
    validate_asset_response(&executable, &release.executable)?;
    let actual_sha256 = sha256_bytes(&executable.body);
    if actual_sha256 != expected_sha256 {
        return Err(SelfUpdateError::Integrity);
    }

    let write_result = (|| {
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(staging_path)
            .map_err(|_| SelfUpdateError::Io)?;
        file.write_all(&executable.body)
            .map_err(|_| SelfUpdateError::Io)?;
        file.flush().map_err(|_| SelfUpdateError::Io)?;
        file.sync_all().map_err(|_| SelfUpdateError::Io)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(staging_path);
        return Err(SelfUpdateError::Io);
    }
    Ok(VerifiedExecutable {
        path: staging_path.to_path_buf(),
        sha256: expected_sha256,
        size: release.executable.size,
    })
}

fn validate_asset_response(
    response: &DownloadResponse,
    asset: &ReleaseAsset,
) -> Result<(), SelfUpdateError> {
    if response.status / 100 != 2 || !is_trusted_download_url(&response.final_url) {
        return Err(SelfUpdateError::Network);
    }
    let actual_size = u64::try_from(response.body.len()).map_err(|_| SelfUpdateError::TooLarge)?;
    if actual_size != asset.size {
        return Err(SelfUpdateError::Integrity);
    }
    Ok(())
}

fn checksum_for_asset(body: &[u8], expected_name: &str) -> Result<[u8; 32], SelfUpdateError> {
    let text = std::str::from_utf8(body).map_err(|_| SelfUpdateError::Integrity)?;
    let mut result = None;
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let Some((hash, name)) = line.split_once("  ") else {
            return Err(SelfUpdateError::Integrity);
        };
        if hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || name.is_empty()
            || name.contains(['/', '\\'])
        {
            return Err(SelfUpdateError::Integrity);
        }
        if name == expected_name {
            if result.is_some() {
                return Err(SelfUpdateError::Integrity);
            }
            result = Some(parse_sha256(hash)?);
        }
    }
    result.ok_or(SelfUpdateError::Integrity)
}

fn parse_sha256(value: &str) -> Result<[u8; 32], SelfUpdateError> {
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| SelfUpdateError::Integrity)?;
    }
    Ok(bytes)
}

fn sha256_bytes(body: &[u8]) -> [u8; 32] {
    Sha256::digest(body).into()
}

fn sha256_file(path: &Path) -> Result<([u8; 32], u64), SelfUpdateError> {
    let mut file = File::open(path).map_err(|_| SelfUpdateError::Io)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| SelfUpdateError::Io)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(u64::try_from(read).map_err(|_| SelfUpdateError::TooLarge)?)
            .ok_or(SelfUpdateError::TooLarge)?;
        if total > MAX_EXECUTABLE_BYTES {
            return Err(SelfUpdateError::TooLarge);
        }
        hasher.update(&buffer[..read]);
    }
    Ok((hasher.finalize().into(), total))
}

fn valid_staging_path(path: &Path) -> bool {
    path.is_absolute()
        && path.file_name().is_some()
        && path.parent().is_some_and(Path::is_absolute)
        && !path.exists()
}

/// 별도 헬퍼 프로세스가 수행할 원자 교체 계획입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperPlan {
    /// 종료를 기다릴 기존 CodexPeek 프로세스 ID입니다.
    pub parent_pid: u32,
    /// 기존 실행 파일의 절대 경로입니다.
    pub target: PathBuf,
    /// 이미 내려받고 검증한 동일 디렉터리의 staging 파일입니다.
    pub staged: PathBuf,
    /// 교체 실패 시 복원할 동일 디렉터리의 backup 파일입니다.
    pub backup: PathBuf,
    /// helper가 인수를 검증한 뒤 부모에게 준비 완료를 알릴 전용 파일입니다.
    pub ready: PathBuf,
    /// 새 실행 파일이 UI 초기화를 마쳤음을 helper에 알리는 전용 파일입니다.
    pub restart_ready: PathBuf,
    /// staging 파일의 검증된 SHA-256입니다.
    pub expected_sha256: [u8; 32],
    /// staging 파일의 검증된 바이트 길이입니다.
    pub expected_size: u64,
    /// 업데이트 후 다시 전달할 안전한 앱 실행 인수입니다.
    pub relaunch_args: Vec<OsString>,
}

impl HelperPlan {
    /// Unicode 경로를 손실 없이 전달하는 내부 helper mode 인수로 직렬화합니다.
    pub fn encode_arguments(&self) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from(HELPER_MODE_ARGUMENT),
            OsString::from("--parent-pid"),
            OsString::from(self.parent_pid.to_string()),
            OsString::from("--target"),
            self.target.as_os_str().to_owned(),
            OsString::from("--staged"),
            self.staged.as_os_str().to_owned(),
            OsString::from("--backup"),
            self.backup.as_os_str().to_owned(),
            OsString::from("--ready"),
            self.ready.as_os_str().to_owned(),
            OsString::from("--restart-ready"),
            self.restart_ready.as_os_str().to_owned(),
            OsString::from("--sha256"),
            OsString::from(format_sha256(self.expected_sha256)),
            OsString::from("--size"),
            OsString::from(self.expected_size.to_string()),
            OsString::from("--"),
        ];
        arguments.extend(self.relaunch_args.iter().cloned());
        arguments
    }

    /// 앱의 원시 `OsString` 실행 인수에서 내부 helper mode 계획만 엄격하게 복원합니다.
    ///
    /// 첫 인수가 helper marker가 아니면 `Ok(None)`이며, marker 뒤 형식이 조금이라도 다르면
    /// `InvalidPlan`입니다. 따라서 일반 앱 인수와 helper 인수를 안전하게 구분할 수 있습니다.
    pub fn decode_arguments(
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<Option<Self>, SelfUpdateError> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        if arguments.first().and_then(|value| value.to_str()) != Some(HELPER_MODE_ARGUMENT) {
            return Ok(None);
        }
        if arguments.len() < 18
            || arguments[1] != "--parent-pid"
            || arguments[3] != "--target"
            || arguments[5] != "--staged"
            || arguments[7] != "--backup"
            || arguments[9] != "--ready"
            || arguments[11] != "--restart-ready"
            || arguments[13] != "--sha256"
            || arguments[15] != "--size"
            || arguments[17] != "--"
        {
            return Err(SelfUpdateError::InvalidPlan);
        }
        let parent_pid = parse_os_number::<u32>(&arguments[2])?;
        let expected_sha256 =
            parse_sha256(arguments[14].to_str().ok_or(SelfUpdateError::InvalidPlan)?)
                .map_err(|_| SelfUpdateError::InvalidPlan)?;
        let expected_size = parse_os_number::<u64>(&arguments[16])?;
        let plan = Self {
            parent_pid,
            target: PathBuf::from(&arguments[4]),
            staged: PathBuf::from(&arguments[6]),
            backup: PathBuf::from(&arguments[8]),
            ready: PathBuf::from(&arguments[10]),
            restart_ready: PathBuf::from(&arguments[12]),
            expected_sha256,
            expected_size,
            relaunch_args: arguments[18..].to_vec(),
        };
        plan.validate()?;
        Ok(Some(plan))
    }

    fn validate(&self) -> Result<(), SelfUpdateError> {
        let parent = self.target.parent().ok_or(SelfUpdateError::InvalidPlan)?;
        let relaunch_args_are_valid = self.relaunch_args.is_empty()
            || (self.relaunch_args.len() == 1
                && self.relaunch_args[0].to_str() == Some("--startup"));
        if self.parent_pid == 0
            || !self.target.is_absolute()
            || !self.staged.is_absolute()
            || !self.backup.is_absolute()
            || !self.ready.is_absolute()
            || !self.restart_ready.is_absolute()
            || self.staged.parent() != Some(parent)
            || self.backup.parent() != Some(parent)
            || self.target == self.staged
            || self.target == self.backup
            || self.staged == self.backup
            || self.ready == self.target
            || self.ready == self.staged
            || self.ready == self.backup
            || self.restart_ready == self.target
            || self.restart_ready == self.staged
            || self.restart_ready == self.backup
            || self.restart_ready == self.ready
            || self.restart_ready.exists()
            || !relaunch_args_are_valid
            || self.expected_size == 0
            || self.expected_size > MAX_EXECUTABLE_BYTES
        {
            return Err(SelfUpdateError::InvalidPlan);
        }
        Ok(())
    }
}

/// helper 교체 순서를 운영체제와 분리해 실패 경로를 단위 테스트할 수 있게 하는 경계입니다.
pub trait SelfUpdateProcess {
    /// Waits for the exact child process to report UI readiness while confirming it stays alive.
    fn wait_for_restart_ready(
        &mut self,
        restart_ready: &Path,
        timeout: Duration,
    ) -> Result<(), SelfUpdateError>;
    /// Terminates the exact child process and returns only after its exit is confirmed.
    fn terminate_and_wait(&mut self) -> Result<(), SelfUpdateError>;
}

pub trait SelfUpdatePlatform {
    /// 기존 프로세스가 종료될 때까지 제한된 시간 동안 기다립니다.
    fn wait_for_exit(&self, process_id: u32, timeout: Duration) -> Result<(), SelfUpdateError>;
    /// staging 파일을 target으로 교체하면서 기존 target을 backup으로 보존합니다.
    fn replace_with_backup(
        &self,
        target: &Path,
        staged: &Path,
        backup: &Path,
    ) -> Result<(), SelfUpdateError>;
    /// 새 파일 시작 실패 뒤 backup을 target으로 복원합니다.
    fn rollback(&self, target: &Path, backup: &Path) -> Result<(), SelfUpdateError>;
    /// 대상 실행 파일을 비동기로 다시 시작합니다.
    fn relaunch(
        &self,
        target: &Path,
        arguments: &[OsString],
        restart_ready: Option<&Path>,
    ) -> Result<Box<dyn SelfUpdateProcess>, SelfUpdateError>;
    /// 성공한 교체 뒤 더 이상 필요 없는 backup을 제거합니다.
    fn remove_backup(&self, backup: &Path) -> Result<(), SelfUpdateError>;
}

/// 헬퍼가 완료한 최종 상태입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperOutcome {
    /// 새 실행 파일로 교체하고 재실행했습니다.
    Updated,
    /// 새 실행 파일 재실행 실패 뒤 기존 실행 파일을 복원하고 재실행했습니다.
    RolledBack,
}

/// 검증된 계획을 실제 Windows 헬퍼 순서로 적용합니다.
pub fn native_update_helper(plan: &HelperPlan) -> Result<HelperOutcome, SelfUpdateError> {
    apply_update_helper(&NativeSelfUpdatePlatform, plan)
}

/// 원시 앱 인수에서 helper mode를 처리합니다.
///
/// 일반 실행이면 `Ok(None)`입니다. helper mode이면 준비 완료 파일을 내구성 있게 만든 뒤 기존
/// 프로세스 종료를 기다리고 교체를 적용합니다. 앱의 `OsString` 손실 변환과 single-instance 획득
/// 전에 호출해야 합니다.
pub fn run_update_helper_mode(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Option<HelperOutcome>, SelfUpdateError> {
    let Some(plan) = HelperPlan::decode_arguments(arguments)? else {
        return Ok(None);
    };
    verify_helper_plan(&plan)?;
    write_ready_file(&plan.ready)?;
    let result = apply_verified_update_helper(&NativeSelfUpdatePlatform, &plan);
    remove_file_if_present(&plan.ready);
    remove_file_if_present(&plan.restart_ready);
    if let Ok(helper_path) = std::env::current_exe() {
        schedule_helper_cleanup(&helper_path);
    }
    result.map(Some)
}

/// 주입된 플랫폼 경계로 wait, replace, rollback, relaunch 순서를 수행합니다.
pub fn apply_update_helper(
    platform: &dyn SelfUpdatePlatform,
    plan: &HelperPlan,
) -> Result<HelperOutcome, SelfUpdateError> {
    verify_helper_plan(plan)?;
    apply_verified_update_helper(platform, plan)
}

fn verify_helper_plan(plan: &HelperPlan) -> Result<(), SelfUpdateError> {
    plan.validate()?;
    let (actual_sha256, actual_size) = sha256_file(&plan.staged)?;
    if actual_sha256 != plan.expected_sha256 || actual_size != plan.expected_size {
        return Err(SelfUpdateError::Integrity);
    }
    Ok(())
}

fn apply_verified_update_helper(
    platform: &dyn SelfUpdatePlatform,
    plan: &HelperPlan,
) -> Result<HelperOutcome, SelfUpdateError> {
    platform.wait_for_exit(plan.parent_pid, HELPER_WAIT_TIMEOUT)?;
    // The staged file remains writable while the parent is shutting down, so bind the bytes
    // again immediately before the irreversible replacement step.
    if verify_helper_plan(plan).is_err() {
        platform
            .relaunch(&plan.target, &plan.relaunch_args, None)
            .map_err(|_| SelfUpdateError::RelaunchFailed)?;
        return Ok(HelperOutcome::RolledBack);
    }
    if platform
        .replace_with_backup(&plan.target, &plan.staged, &plan.backup)
        .is_err()
    {
        platform
            .relaunch(&plan.target, &plan.relaunch_args, None)
            .map_err(|_| SelfUpdateError::RelaunchFailed)?;
        return Ok(HelperOutcome::RolledBack);
    }
    if let Ok(mut process) =
        platform.relaunch(&plan.target, &plan.relaunch_args, Some(&plan.restart_ready))
    {
        if process
            .wait_for_restart_ready(&plan.restart_ready, RESTART_READY_TIMEOUT)
            .is_ok()
        {
            let _ = platform.remove_backup(&plan.backup);
            remove_file_if_present(&plan.restart_ready);
            return Ok(HelperOutcome::Updated);
        }
        process
            .terminate_and_wait()
            .map_err(|_| SelfUpdateError::TerminationFailed)?;
    }
    platform
        .rollback(&plan.target, &plan.backup)
        .map_err(|_| SelfUpdateError::RollbackFailed)?;
    platform
        .relaunch(&plan.target, &plan.relaunch_args, None)
        .map_err(|_| SelfUpdateError::RelaunchFailed)?;
    remove_file_if_present(&plan.restart_ready);
    Ok(HelperOutcome::RolledBack)
}

fn parse_os_number<T>(value: &OsString) -> Result<T, SelfUpdateError>
where
    T: std::str::FromStr,
{
    value
        .to_str()
        .ok_or(SelfUpdateError::InvalidPlan)?
        .parse()
        .map_err(|_| SelfUpdateError::InvalidPlan)
}

fn format_sha256(value: [u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut text = String::with_capacity(64);
    for byte in value {
        let _ = write!(&mut text, "{byte:02x}");
    }
    text
}

fn update_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = UPDATE_NONCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{timestamp:x}-{counter:x}", std::process::id())
}

fn copy_file_exclusively(source: &Path, destination: &Path) -> Result<(), SelfUpdateError> {
    let mut source = File::open(source).map_err(|_| SelfUpdateError::Io)?;
    let mut destination_file = File::options()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|_| SelfUpdateError::Io)?;
    let copy_result = (|| {
        std::io::copy(&mut source, &mut destination_file).map_err(|_| SelfUpdateError::Io)?;
        destination_file.flush().map_err(|_| SelfUpdateError::Io)?;
        destination_file.sync_all().map_err(|_| SelfUpdateError::Io)
    })();
    if copy_result.is_err() {
        remove_file_if_present(destination);
    }
    copy_result
}

fn write_ready_file(path: &Path) -> Result<(), SelfUpdateError> {
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| SelfUpdateError::Io)?;
    file.write_all(b"ready\n")
        .map_err(|_| SelfUpdateError::Io)?;
    file.flush().map_err(|_| SelfUpdateError::Io)?;
    file.sync_all().map_err(|_| SelfUpdateError::Io)
}

fn ready_file_exists(path: &Path) -> bool {
    fs::read(path).is_ok_and(|contents| contents == b"ready\n")
}

/// Removes the private restart handshake argument before normal launch-mode parsing.
pub fn take_restart_ready_argument(
    arguments: &mut Vec<OsString>,
) -> Result<Option<PathBuf>, SelfUpdateError> {
    let marker_positions = arguments
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (value == RESTART_READY_ARGUMENT).then_some(index))
        .collect::<Vec<_>>();
    if marker_positions.is_empty() {
        return Ok(None);
    }
    if marker_positions.len() != 1 || marker_positions[0] + 2 != arguments.len() {
        return Err(SelfUpdateError::InvalidPlan);
    }
    let path = PathBuf::from(arguments.pop().ok_or(SelfUpdateError::InvalidPlan)?);
    let marker = arguments.pop().ok_or(SelfUpdateError::InvalidPlan)?;
    if marker != RESTART_READY_ARGUMENT || !valid_restart_ready_path(&path) {
        return Err(SelfUpdateError::InvalidPlan);
    }
    Ok(Some(path))
}

/// Signals that the relaunched app has created its tray/UI successfully.
pub fn signal_restart_ready(path: &Path) -> Result<(), SelfUpdateError> {
    if !valid_restart_ready_path(path) {
        return Err(SelfUpdateError::InvalidPlan);
    }
    write_ready_file(path)
}

fn valid_restart_ready_path(path: &Path) -> bool {
    let expected_parent = std::env::temp_dir().join("CodexPeek").join("updates");
    path.is_absolute()
        && path.parent() == Some(expected_parent.as_path())
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("codex-peek-update-restart-") && name.ends_with(".ready")
            })
        && !path.exists()
}

fn cleanup_unstarted_update(prepared: &PreparedUpdateHelper) {
    remove_file_if_present(&prepared.plan.staged);
    remove_file_if_present(&prepared.plan.ready);
    remove_file_if_present(&prepared.plan.restart_ready);
    remove_file_if_present(&prepared.helper_path);
}

fn remove_file_if_present(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

#[cfg(windows)]
fn schedule_helper_cleanup(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT},
    };

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let _ = unsafe {
        MoveFileExW(
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            MOVEFILE_DELAY_UNTIL_REBOOT,
        )
    };
}

#[cfg(not(windows))]
fn schedule_helper_cleanup(_path: &Path) {}

#[derive(Clone, Copy, Debug, Default)]
struct NativeSelfUpdatePlatform;

impl SelfUpdatePlatform for NativeSelfUpdatePlatform {
    fn wait_for_exit(&self, process_id: u32, timeout: Duration) -> Result<(), SelfUpdateError> {
        wait_for_process_exit(process_id, timeout)
    }

    fn replace_with_backup(
        &self,
        target: &Path,
        staged: &Path,
        backup: &Path,
    ) -> Result<(), SelfUpdateError> {
        if !target.is_file() || !staged.is_file() || backup.exists() {
            return Err(SelfUpdateError::ReplaceFailed);
        }
        replace_file(target, staged, Some(backup)).map_err(|_| SelfUpdateError::ReplaceFailed)
    }

    fn rollback(&self, target: &Path, backup: &Path) -> Result<(), SelfUpdateError> {
        if !target.is_file() || !backup.is_file() {
            return Err(SelfUpdateError::RollbackFailed);
        }
        replace_file(target, backup, None).map_err(|_| SelfUpdateError::RollbackFailed)
    }

    fn relaunch(
        &self,
        target: &Path,
        arguments: &[OsString],
        restart_ready: Option<&Path>,
    ) -> Result<Box<dyn SelfUpdateProcess>, SelfUpdateError> {
        let mut command = Command::new(target);
        command.args(arguments);
        if let Some(path) = restart_ready {
            command.arg(RESTART_READY_ARGUMENT).arg(path);
        }
        command
            .spawn()
            .map(|child| Box::new(NativeSelfUpdateProcess { child }) as Box<dyn SelfUpdateProcess>)
            .map_err(|_| SelfUpdateError::RelaunchFailed)
    }

    fn remove_backup(&self, backup: &Path) -> Result<(), SelfUpdateError> {
        fs::remove_file(backup).map_err(|_| SelfUpdateError::Io)
    }
}

struct NativeSelfUpdateProcess {
    child: Child,
}

impl SelfUpdateProcess for NativeSelfUpdateProcess {
    fn wait_for_restart_ready(
        &mut self,
        restart_ready: &Path,
        timeout: Duration,
    ) -> Result<(), SelfUpdateError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if self
                .child
                .try_wait()
                .map_err(|_| SelfUpdateError::RelaunchFailed)?
                .is_some()
            {
                return Err(SelfUpdateError::RelaunchFailed);
            }
            if ready_file_exists(restart_ready) {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(SelfUpdateError::RelaunchFailed);
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn terminate_and_wait(&mut self) -> Result<(), SelfUpdateError> {
        if self
            .child
            .try_wait()
            .map_err(|_| SelfUpdateError::TerminationFailed)?
            .is_some()
        {
            return Ok(());
        }
        self.child
            .kill()
            .map_err(|_| SelfUpdateError::TerminationFailed)?;
        self.child
            .wait()
            .map(|_| ())
            .map_err(|_| SelfUpdateError::TerminationFailed)
    }
}

#[cfg(windows)]
fn wait_for_process_exit(process_id: u32, timeout: Duration) -> Result<(), SelfUpdateError> {
    use windows::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };

    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, process_id) }
        .map_err(|_| SelfUpdateError::WaitFailed)?;
    let milliseconds = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    let result = unsafe { WaitForSingleObject(handle, milliseconds) };
    let _ = unsafe { CloseHandle(handle) };
    (result == WAIT_OBJECT_0)
        .then_some(())
        .ok_or(SelfUpdateError::WaitFailed)
}

#[cfg(not(windows))]
fn wait_for_process_exit(_process_id: u32, _timeout: Duration) -> Result<(), SelfUpdateError> {
    Err(SelfUpdateError::WaitFailed)
}

#[cfg(windows)]
fn replace_file(target: &Path, replacement: &Path, backup: Option<&Path>) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH},
    };

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let target = wide_path(target);
    let replacement = wide_path(replacement);
    let backup = backup.map(wide_path);
    let backup_pointer = backup
        .as_ref()
        .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));
    unsafe {
        ReplaceFileW(
            PCWSTR(target.as_ptr()),
            PCWSTR(replacement.as_ptr()),
            backup_pointer,
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
        .map_err(|error| std::io::Error::from_raw_os_error(error.code().0))
    }
}

#[cfg(not(windows))]
fn replace_file(target: &Path, replacement: &Path, backup: Option<&Path>) -> std::io::Result<()> {
    if let Some(backup) = backup {
        fs::rename(target, backup)?;
        if let Err(error) = fs::rename(replacement, target) {
            let _ = fs::rename(backup, target);
            return Err(error);
        }
        Ok(())
    } else {
        fs::rename(replacement, target)
    }
}

fn is_https_url(value: &str) -> bool {
    value
        .parse::<ureq::http::Uri>()
        .ok()
        .is_some_and(|uri| uri.scheme_str() == Some("https") && uri.host().is_some())
}

fn is_trusted_download_url(value: &str) -> bool {
    value.parse::<ureq::http::Uri>().ok().is_some_and(|uri| {
        uri.scheme_str() == Some("https")
            && matches!(
                uri.host(),
                Some("api.github.com")
                    | Some("github.com")
                    | Some("release-assets.githubusercontent.com")
                    | Some("objects.githubusercontent.com")
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_parser_requires_one_exact_safe_asset_name() {
        let body = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  other.zip\n\
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  app.exe\n";
        assert_eq!(checksum_for_asset(body, "app.exe"), Ok([0xbb; 32]));
        assert_eq!(
            checksum_for_asset(b"bad  app.exe\n", "app.exe"),
            Err(SelfUpdateError::Integrity)
        );
        assert_eq!(
            checksum_for_asset(
                b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  dir/app.exe\n",
                "dir/app.exe"
            ),
            Err(SelfUpdateError::Integrity)
        );
    }

    #[test]
    fn trusted_download_urls_never_allow_http_or_unrelated_hosts() {
        assert!(is_trusted_download_url("https://github.com/a/b"));
        assert!(is_trusted_download_url(
            "https://api.github.com/repos/a/b/releases/latest"
        ));
        assert!(is_trusted_download_url(
            "https://release-assets.githubusercontent.com/file?token=redacted"
        ));
        assert!(!is_trusted_download_url("http://github.com/a/b"));
        assert!(!is_trusted_download_url("https://github.com.evil.test/a/b"));
    }
}
