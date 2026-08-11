use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use semver::Version;
use serde::Deserialize;
use ureq::tls::{TlsConfig, TlsProvider};

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const USER_AGENT: &str = "CodexUsageMonitor/0.1 update-check";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// 제한된 HTTP 응답 정보입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP 상태 코드입니다.
    pub status: u16,
    /// 제한된 길이로 읽은 응답 본문입니다.
    pub body: Vec<u8>,
}

/// 업데이트 검사 통신 또는 응답 검증 실패를 나타내는 안전한 오류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCheckError {
    /// 네트워크 요청을 완료하지 못했거나 릴리스 응답이 안전한 형식이 아닙니다.
    Network,
}

/// 업데이트 확인에 필요한 최소 HTTP 인터페이스입니다.
pub trait ReleaseHttpClient: Send + Sync {
    /// 제한된 응답 크기와 시간 제한을 사용하여 GET 요청을 보냅니다.
    ///
    /// `url`, `user_agent`, `timeout`, `max_bytes`를 그대로 적용해야 하며, 성공 시 본문 길이는
    /// `max_bytes` 이하여야 합니다. 전송 또는 제한 위반은 `UpdateCheckError`로 반환합니다.
    fn get(
        &self,
        url: &str,
        user_agent: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<HttpResponse, UpdateCheckError>;
}

/// ureq 기반의 HTTPS 전용 릴리스 HTTP 클라이언트입니다.
#[derive(Clone, Copy, Debug, Default)]
pub struct UreqHttpClient;

impl ReleaseHttpClient for UreqHttpClient {
    fn get(
        &self,
        url: &str,
        user_agent: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<HttpResponse, UpdateCheckError> {
        if !url.starts_with("https://") || max_bytes == 0 {
            return Err(UpdateCheckError::Network);
        }
        let max_bytes = u64::try_from(max_bytes).map_err(|_| UpdateCheckError::Network)?;
        let config = release_agent_config(user_agent, timeout);
        let agent = ureq::Agent::new_with_config(config);
        let mut response = agent
            .get(url)
            .call()
            .map_err(|_| UpdateCheckError::Network)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(max_bytes)
            .read_to_vec()
            .map_err(|_| UpdateCheckError::Network)?;
        if u64::try_from(body.len()).map_err(|_| UpdateCheckError::Network)? > max_bytes {
            return Err(UpdateCheckError::Network);
        }
        Ok(HttpResponse { status, body })
    }
}

fn release_agent_config(user_agent: &str, timeout: Duration) -> ureq::config::Config {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(timeout))
        .user_agent(user_agent)
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .build(),
        )
        .build()
}

/// 안전하게 표시할 새 버전 정보입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    /// 비교를 통과한 새 버전입니다.
    pub version: Version,
    /// GitHub의 HTTPS 릴리스 페이지입니다.
    pub release_url: String,
}

/// 수동 업데이트 확인이 끝난 뒤 사용자에게 한 번 표시할 결과입니다.
///
/// `Available`에는 검증이 끝난 정확한 GitHub 태그 URL만 포함됩니다. 자동 확인 결과는 사용자가
/// 진행 중인 검사에 명시적으로 합류한 경우가 아니면 이 알림으로 생성되지 않습니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheckNotice {
    /// 현재 실행 중인 버전이 최신입니다.
    Current,
    /// 검증된 새 버전을 사용할 수 있습니다.
    Available(AvailableUpdate),
    /// 네트워크 요청 또는 릴리스 응답 검증에 실패했습니다.
    Failed,
    /// The verified updater helper is waiting for this process to exit.
    InstallReady,
    /// Downloading the release assets failed before the executable was changed.
    DownloadFailed,
    /// Release metadata, asset identity, or checksum verification failed.
    VerificationFailed,
    /// Preparing or starting the updater helper failed.
    InstallFailed,
    /// The executable was not produced by the official release workflow.
    UnofficialBuild,
}

/// 업데이트 검사가 시작된 사용자 의도를 구분합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCheckIntent {
    /// 시작 또는 일일 주기에 따른 자동 검사입니다.
    Automatic,
    /// 사용자가 메뉴에서 직접 요청한 검사입니다.
    UserInitiated,
}

/// 업데이트 검사를 실제로 시작해야 하는지 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCheckStart {
    /// 호출자가 새 검사 작업자를 시작해야 합니다.
    Started,
    /// 검사가 이미 실행 중이므로 새 작업자를 만들지 않습니다.
    AlreadyRunning,
}

/// 사용자에게 표시할 업데이트 검사 상태입니다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdatePresentationStatus {
    /// 아직 검사 결과가 없습니다.
    #[default]
    Idle,
    /// 업데이트를 확인하고 있습니다.
    Checking,
    /// 새 버전을 사용할 수 있습니다.
    Available,
    /// 현재 버전이 최신입니다.
    Current,
    /// 업데이트 검사에 실패했습니다.
    Failed,
    /// The selected release assets are being downloaded and verified.
    Downloading,
    /// The verified helper is ready to replace the executable and restart the app.
    Installing,
}

/// 사용자의 업데이트 메뉴 동작을 안전한 저장 상태로 해석한 결과입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateUserAction {
    /// 검사기가 검증해 저장한 업데이트를 사용자에게 제시합니다.
    Open(AvailableUpdate),
    /// 호출자가 새 검사 작업자를 시작해야 합니다.
    StartCheck,
    /// 실행 중인 검사에 사용자 의도가 합쳐졌으므로 완료를 기다립니다.
    WaitForRunning,
}

#[derive(Default)]
struct UpdatePresentationInner {
    status: UpdatePresentationStatus,
    available: Option<AvailableUpdate>,
    open_requested: bool,
    pending_user_notice: Option<UpdateCheckNotice>,
    running_intent: Option<UpdateCheckIntent>,
    pending_user_intent: bool,
    pending_install_request: Option<AvailableUpdate>,
}

/// 업데이트 결과와 UI 스레드가 처리할 일회성 사용자 알림을 공유하는 상태입니다.
///
/// 복제본은 같은 내부 상태를 공유합니다. 검사 작업자는 결과만 기록하고, 대화상자와 브라우저 같은
/// 사용자 상호작용은 `take_user_notice`로 결과를 소비한 UI 스레드가 담당해야 합니다.
#[derive(Clone, Default)]
pub struct UpdatePresentation {
    inner: Arc<Mutex<UpdatePresentationInner>>,
}

impl UpdatePresentation {
    /// 검사 시작 권한을 원자적으로 획득하거나 실행 중인 검사에 사용자 의도를 합칩니다.
    ///
    /// 이미 자동 검사가 실행 중일 때 `UserInitiated`가 들어오면 새 작업자를 만들지 않고
    /// 완료 결과를 사용자 요청으로 처리하도록 승격합니다.
    pub fn begin_check(&self, intent: UpdateCheckIntent) -> UpdateCheckStart {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        begin_check_locked(&mut inner, intent)
    }

    /// 실행 중인 검사를 완료하고 상태와 일회성 사용자 알림을 원자적으로 기록합니다.
    ///
    /// 유효한 사용자 의도가 있었으면 새 버전·최신 상태·검사 실패 중 하나를 알림으로 한 번 저장합니다.
    /// 자동 검사에서 새 버전을 찾은 경우에도 알림을 만들며, 거절한 버전과 정확히 같을 때만
    /// 자동 알림을 생략합니다.
    pub fn record_result(&self, result: Result<Option<AvailableUpdate>, UpdateCheckError>) {
        self.record_result_with_dismissed_version(result, None);
    }

    /// 실행 중인 검사를 완료하고 저장된 거절 버전을 적용해 표시 결과를 기록합니다.
    ///
    /// `dismissed_version`은 자동 검사에만 적용됩니다. 수동 검사 또는 자동 검사 도중 합쳐진
    /// 사용자 요청은 같은 버전을 이전에 거절했더라도 항상 결과 알림을 만듭니다.
    pub fn record_result_with_dismissed_version(
        &self,
        result: Result<Option<AvailableUpdate>, UpdateCheckError>,
        dismissed_version: Option<&str>,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.running_intent.is_none() {
            return;
        }
        let user_initiated = inner.running_intent == Some(UpdateCheckIntent::UserInitiated)
            || inner.pending_user_intent;
        inner.running_intent = None;
        inner.pending_user_intent = false;
        inner.open_requested = false;
        let notice = match result {
            Ok(Some(update)) => {
                inner.status = UpdatePresentationStatus::Available;
                inner.available = Some(update.clone());
                inner.open_requested = user_initiated;
                Some(UpdateCheckNotice::Available(update))
            }
            Ok(None) => {
                inner.status = UpdatePresentationStatus::Current;
                inner.available = None;
                Some(UpdateCheckNotice::Current)
            }
            Err(_) => {
                inner.status = UpdatePresentationStatus::Failed;
                inner.available = None;
                Some(UpdateCheckNotice::Failed)
            }
        };
        let automatic_update_was_dismissed = !user_initiated
            && inner.available.as_ref().is_some_and(|update| {
                dismissed_version == Some(update.version.to_string().as_str())
            });
        if user_initiated || (inner.available.is_some() && !automatic_update_was_dismissed) {
            inner.pending_user_notice = notice;
        }
    }

    /// 현재 사용자에게 표시할 업데이트 검사 상태를 반환합니다.
    pub fn status(&self) -> UpdatePresentationStatus {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .status
    }

    /// 현재 표시할 검증된 업데이트를 복사해 반환합니다.
    pub fn available_update(&self) -> Option<AvailableUpdate> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .available
            .clone()
    }

    /// 사용자 메뉴 동작을 저장된 결과 표시 또는 원자적인 검사 시작 결정으로 변환합니다.
    ///
    /// 결과 확인과 검사 시작을 같은 잠금에서 처리하므로 자동 검사 완료와 경합해 사용자 의도가
    /// 사라지지 않습니다. `StartCheck`인 경우에만 호출자가 새 작업자를 만들어야 합니다.
    pub fn begin_user_action(&self) -> UpdateUserAction {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(update) = inner.available.clone() {
            return UpdateUserAction::Open(update);
        }
        match begin_check_locked(&mut inner, UpdateCheckIntent::UserInitiated) {
            UpdateCheckStart::Started => UpdateUserAction::StartCheck,
            UpdateCheckStart::AlreadyRunning => UpdateUserAction::WaitForRunning,
        }
    }

    /// UI 스레드가 처리할 기존 일회성 브라우저 열기 요청을 소비합니다.
    ///
    /// 호환성을 위해 유지되는 API입니다. 새 UI는 `take_user_notice`로 모든 결과를 소비하고, 반환된
    /// 업데이트를 대화상자로 제시한 뒤 사용자가 동의한 경우에만 검증된 URL을 열어야 합니다.
    pub fn take_open_request(&self) -> Option<AvailableUpdate> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if !inner.open_requested {
            return None;
        }
        inner.open_requested = false;
        inner.available.clone()
    }

    /// 수동 업데이트 확인 결과를 정확히 한 번 반환합니다.
    ///
    /// 백그라운드 작업자는 결과만 기록하며, 호출자는 UI 스레드에서 이 메서드를 호출해 대화상자 같은
    /// 사용자 상호작용을 처리해야 합니다. 결과를 반환하면 같은 알림은 즉시 제거됩니다.
    pub fn take_user_notice(&self) -> Option<UpdateCheckNotice> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let notice = inner.pending_user_notice.take();
        if notice.is_some() {
            inner.open_requested = false;
        }
        notice
    }

    /// UI 스레드가 표시할 사용자 업데이트 결과를 한 번 저장합니다.
    ///
    /// 자동 검사로 이미 발견된 업데이트를 사용자가 메뉴에서 요청한 경우처럼 작업자가 새 요청을
    /// 만들지 않고도 결과를 표시해야 할 때 사용합니다. 호출은 네트워크 I/O를 수행하지 않습니다.
    pub fn queue_user_notice(&self, notice: UpdateCheckNotice) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.open_requested = false;
        inner.pending_user_notice = Some(notice);
    }

    /// 현재 검증된 업데이트 버전의 자동 안내를 거절 처리합니다.
    ///
    /// 표시 중인 업데이트와 버전이 정확히 일치할 때만 `true`를 반환합니다. 호출자는 성공한
    /// 버전만 설정에 저장해야 하며, 이 메서드는 파일 I/O를 수행하지 않습니다.
    pub fn dismiss_available_version(&self, version: &str) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner
            .available
            .as_ref()
            .is_none_or(|update| update.version.to_string() != version)
        {
            return false;
        }
        inner.open_requested = false;
        if inner.pending_user_notice.as_ref().is_some_and(|notice| {
            matches!(notice, UpdateCheckNotice::Available(update) if update.version.to_string() == version)
        }) {
            inner.pending_user_notice = None;
        }
        true
    }

    /// 사용자가 승인한 검증된 업데이트를 설치 작업자 경계에 한 번 저장합니다.
    ///
    /// 임의 URL 주입을 막기 위해 검사기가 보관한 전체 업데이트 값과 정확히 일치해야 합니다.
    /// 실제 다운로드와 교체는 이 큐를 소비하는 별도 작업자가 수행해야 합니다.
    pub fn queue_install_request(&self, update: AvailableUpdate) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.available.as_ref() != Some(&update) || inner.pending_install_request.is_some() {
            return false;
        }
        inner.open_requested = false;
        if inner.pending_user_notice.as_ref() == Some(&UpdateCheckNotice::Available(update.clone()))
        {
            inner.pending_user_notice = None;
        }
        inner.pending_install_request = Some(update);
        inner.status = UpdatePresentationStatus::Downloading;
        true
    }

    /// 설치 작업자에게 전달할 검증된 업데이트를 정확히 한 번 반환합니다.
    pub fn take_install_request(&self) -> Option<AvailableUpdate> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending_install_request
            .take()
    }

    /// Records the terminal result of the background self-update preparation once.
    pub fn record_install_notice(&self, notice: UpdateCheckNotice) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.status = match notice {
            UpdateCheckNotice::InstallReady => UpdatePresentationStatus::Installing,
            UpdateCheckNotice::DownloadFailed
            | UpdateCheckNotice::VerificationFailed
            | UpdateCheckNotice::InstallFailed => UpdatePresentationStatus::Failed,
            _ => return,
        };
        inner.pending_user_notice = Some(notice);
    }
}

fn begin_check_locked(
    inner: &mut UpdatePresentationInner,
    intent: UpdateCheckIntent,
) -> UpdateCheckStart {
    if inner.running_intent.is_some() {
        if intent == UpdateCheckIntent::UserInitiated {
            inner.pending_user_intent = true;
        }
        return UpdateCheckStart::AlreadyRunning;
    }
    inner.status = UpdatePresentationStatus::Checking;
    inner.available = None;
    inner.open_requested = false;
    inner.running_intent = Some(intent);
    inner.pending_user_intent = false;
    UpdateCheckStart::Started
}

/// GitHub 릴리스만 조회하는 업데이트 검사기입니다.
#[derive(Clone, Debug)]
pub struct UpdateChecker {
    current_version: Version,
    owner: String,
    repository: String,
    max_bytes: usize,
}

impl UpdateChecker {
    /// 유효한 GitHub 저장소 메타데이터가 있을 때만 검사기를 만듭니다.
    ///
    /// `current_version`은 SemVer여야 하고 `repository_url`은 선택적 `.git`을 가진
    /// `https://github.com/<owner>/<repo>` 형식이어야 합니다. `max_bytes`는 0보다 커야 하며,
    /// 하나라도 만족하지 않으면 네트워크 작업 없이 `None`을 반환합니다.
    pub fn new(
        current_version: &str,
        repository_url: Option<&str>,
        max_bytes: usize,
    ) -> Option<Self> {
        let current_version = Version::parse(current_version).ok()?;
        let (owner, repository) = parse_repository(repository_url?)?;
        (max_bytes > 0).then_some(Self {
            current_version,
            owner,
            repository,
            max_bytes,
        })
    }

    /// 마지막 검사 시각이 지났을 때만 최신 릴리스를 확인합니다.
    ///
    /// `last_check` 뒤 24시간이 지나지 않았으면 요청 없이 `Ok(None)`을 반환합니다. 그 외에는
    /// `client`로 최신 릴리스를 조회해 현재 버전보다 새롭고 정확한 GitHub 태그 페이지를 가진 경우만
    /// `Ok(Some(...))`으로 반환합니다. 네트워크 실패와 비정상·과대·안전하지 않은 응답은 모두
    /// `Err`로 반환하여 최신 상태로 오인하지 않습니다.
    pub fn check_if_due(
        &self,
        client: &dyn ReleaseHttpClient,
        last_check: Option<SystemTime>,
        now: SystemTime,
    ) -> Result<Option<AvailableUpdate>, UpdateCheckError> {
        if last_check.is_some_and(|at| {
            now.duration_since(at)
                .is_ok_and(|elapsed| elapsed < CHECK_INTERVAL)
        }) {
            return Ok(None);
        }
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.repository
        );
        let response = client.get(&url, USER_AGENT, REQUEST_TIMEOUT, self.max_bytes)?;
        if response.status / 100 != 2 || response.body.len() > self.max_bytes {
            return Err(UpdateCheckError::Network);
        }
        let release: ReleaseDto = match serde_json::from_slice(&response.body) {
            Ok(release) => release,
            Err(_) => return Err(UpdateCheckError::Network),
        };
        let version_text = release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name);
        if version_text.starts_with('v') {
            return Err(UpdateCheckError::Network);
        }
        let version = match Version::parse(version_text) {
            Ok(version) => version,
            Err(_) => return Err(UpdateCheckError::Network),
        };
        if !self.is_safe_release_url(&release.html_url, &release.tag_name) {
            return Err(UpdateCheckError::Network);
        }
        if version <= self.current_version {
            return Ok(None);
        }
        Ok(Some(AvailableUpdate {
            version,
            release_url: release.html_url,
        }))
    }

    fn is_safe_release_url(&self, value: &str, tag_name: &str) -> bool {
        valid_segment(tag_name)
            && value
                == format!(
                    "https://github.com/{}/{}/releases/tag/{tag_name}",
                    self.owner, self.repository
                )
    }
}

#[derive(Deserialize)]
struct ReleaseDto {
    tag_name: String,
    html_url: String,
}

fn parse_repository(value: &str) -> Option<(String, String)> {
    if !value.starts_with("https://github.com/") || value.contains(['?', '#', '@']) {
        return None;
    }
    let path = value.strip_prefix("https://github.com/")?;
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let repository_part = parts.next()?;
    let repository = repository_part
        .strip_suffix(".git")
        .unwrap_or(repository_part);
    if owner.is_empty()
        || repository.is_empty()
        || parts.next().is_some()
        || !valid_segment(owner)
        || !valid_segment(repository)
    {
        return None;
    }
    Some((owner.to_owned(), repository.to_owned()))
}

fn valid_segment(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.')
        && value != "."
        && value != ".."
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ureq::tls::TlsProvider;

    use super::release_agent_config;

    #[test]
    fn release_agent_uses_the_compiled_native_tls_provider() {
        let config = release_agent_config("CodexUsageMonitor/test", Duration::from_secs(1));

        assert_eq!(config.tls_config().provider(), TlsProvider::NativeTls);
    }
}
