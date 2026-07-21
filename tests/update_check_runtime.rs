use std::time::{Duration, SystemTime};

use codex_usage_monitor::{HttpResponse, ReleaseHttpClient, UpdateCheckError, UpdateChecker};

struct FakeClient {
    response: HttpResponse,
}
impl ReleaseHttpClient for FakeClient {
    fn get(
        &self,
        _url: &str,
        _user_agent: &str,
        _timeout: Duration,
        _max_bytes: usize,
    ) -> Result<HttpResponse, UpdateCheckError> {
        Ok(self.response.clone())
    }
}

#[test]
fn invalid_repository_disables_network_checks() {
    assert!(UpdateChecker::new("0.1.0", Some("http://github.com/owner/repo"), 1024).is_none());
    assert!(
        UpdateChecker::new("0.1.0", Some("https://user@github.com/owner/repo"), 1024).is_none()
    );
}

#[test]
fn due_check_reports_only_newer_safe_github_release() {
    let checker =
        UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo.git"), 1024).unwrap();
    let client = FakeClient { response: HttpResponse { status: 200, body: br#"{"tag_name":"v1.2.0","html_url":"https://github.com/owner/repo/releases/tag/v1.2.0"}"#.to_vec() } };
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);
    let update = checker.check_if_due(&client, None, now).unwrap().unwrap();
    assert_eq!(update.version.to_string(), "1.2.0");
    assert_eq!(
        update.release_url,
        "https://github.com/owner/repo/releases/tag/v1.2.0"
    );
    assert!(checker
        .check_if_due(&client, Some(now), now)
        .unwrap()
        .is_none());
}

#[test]
fn malformed_oversized_non_success_and_unsafe_urls_do_not_report_updates() {
    let checker = UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo"), 16).unwrap();
    for response in [
        HttpResponse {
            status: 500,
            body: vec![],
        },
        HttpResponse {
            status: 200,
            body: b"not json".to_vec(),
        },
        HttpResponse {
            status: 200,
            body: vec![b'x'; 17],
        },
        HttpResponse {
            status: 200,
            body: br#"{"tag_name":"2.0.0","html_url":"https://evil.example/release"}"#.to_vec(),
        },
    ] {
        let client = FakeClient { response };
        assert!(checker
            .check_if_due(
                &client,
                None,
                SystemTime::UNIX_EPOCH + Duration::from_secs(100_000)
            )
            .unwrap()
            .is_none());
    }
}
