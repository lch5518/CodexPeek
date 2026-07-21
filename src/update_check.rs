use std::time::{Duration, SystemTime};

use semver::Version;
use serde::Deserialize;

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

/// 업데이트 검사 통신 실패를 나타내는 안전한 오류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCheckError {
    /// 네트워크 요청을 완료하지 못했습니다.
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
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(timeout))
            .user_agent(user_agent)
            .build();
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

/// 안전하게 표시할 새 버전 정보입니다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    /// 비교를 통과한 새 버전입니다.
    pub version: Version,
    /// GitHub의 HTTPS 릴리스 페이지입니다.
    pub release_url: String,
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
    /// `Ok(Some(...))`으로 반환합니다. 네트워크 실패만 `Err`로 전달하며, 비정상 응답은 안전하게 무시합니다.
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
            return Ok(None);
        }
        let release: ReleaseDto = match serde_json::from_slice(&response.body) {
            Ok(release) => release,
            Err(_) => return Ok(None),
        };
        let version_text = release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name);
        if version_text.starts_with('v') {
            return Ok(None);
        }
        let version = match Version::parse(version_text) {
            Ok(version) => version,
            Err(_) => return Ok(None),
        };
        if version <= self.current_version
            || !self.is_safe_release_url(&release.html_url, &release.tag_name)
        {
            return Ok(None);
        }
        Ok(Some(AvailableUpdate {
            version,
            release_url: release.html_url,
        }))
    }

    fn is_safe_release_url(&self, value: &str, tag_name: &str) -> bool {
        !tag_name.is_empty()
            && !tag_name.contains(['/', '\\', '?', '#', '@'])
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
