use std::{
    sync::Mutex,
    time::{Duration, SystemTime},
};

use codex_usage_monitor::{
    HttpResponse, ReleaseHttpClient, UpdateCheckError, UpdateChecker, UreqHttpClient,
};

struct FakeClient {
    response: HttpResponse,
    requests: Mutex<Vec<(String, String, Duration, usize)>>,
}

impl FakeClient {
    fn new(response: HttpResponse) -> Self {
        Self {
            response,
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl ReleaseHttpClient for FakeClient {
    fn get(
        &self,
        url: &str,
        user_agent: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<HttpResponse, UpdateCheckError> {
        self.requests.lock().unwrap().push((
            url.to_owned(),
            user_agent.to_owned(),
            timeout,
            max_bytes,
        ));
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
fn due_check_uses_the_expected_github_request_and_reports_newer_release() {
    let checker =
        UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo.git"), 1024).unwrap();
    let client = FakeClient::new(HttpResponse {
        status: 200,
        body: br#"{"tag_name":"v1.2.0","html_url":"https://github.com/owner/repo/releases/tag/v1.2.0"}"#
            .to_vec(),
    });
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);

    let update = checker.check_if_due(&client, None, now).unwrap().unwrap();

    assert_eq!(update.version.to_string(), "1.2.0");
    assert_eq!(
        update.release_url,
        "https://github.com/owner/repo/releases/tag/v1.2.0"
    );
    assert_eq!(
        client.requests.lock().unwrap().as_slice(),
        [(
            "https://api.github.com/repos/owner/repo/releases/latest".to_owned(),
            "CodexUsageMonitor/0.1 update-check".to_owned(),
            Duration::from_secs(10),
            1024,
        )]
    );
    assert!(checker
        .check_if_due(&client, Some(now), now)
        .unwrap()
        .is_none());
}

#[test]
fn equal_or_older_release_is_not_reported() {
    let checker = UpdateChecker::new("1.2.0", Some("https://github.com/owner/repo"), 1024).unwrap();
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);

    for tag_name in ["v1.2.0", "1.1.9"] {
        let client = FakeClient::new(HttpResponse {
            status: 200,
            body: format!(
                r#"{{"tag_name":"{tag_name}","html_url":"https://github.com/owner/repo/releases/tag/{tag_name}"}}"#
            )
            .into_bytes(),
        });
        assert!(checker.check_if_due(&client, None, now).unwrap().is_none());
    }
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
        HttpResponse {
            status: 200,
            body: br#"{"tag_name":"2.0.0","html_url":"https://github.com/owner/repo/releases/download/v2.0.0/app.exe"}"#.to_vec(),
        },
    ] {
        let client = FakeClient::new(response);
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

#[test]
fn release_url_must_be_the_exact_tag_page() {
    let checker = UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo"), 1024).unwrap();
    let client = FakeClient::new(HttpResponse {
        status: 200,
        body: br#"{"tag_name":"2.0.0","html_url":"https://github.com/owner/repo/releases/tag/v2.0.0/assets"}"#.to_vec(),
    });

    assert!(checker
        .check_if_due(
            &client,
            None,
            SystemTime::UNIX_EPOCH + Duration::from_secs(100_000)
        )
        .unwrap()
        .is_none());
}

#[test]
fn production_http_client_refuses_non_https_before_network_io() {
    let client = UreqHttpClient;
    assert_eq!(
        client.get(
            "http://github.com/owner/repo",
            "CodexUsageMonitor/test",
            Duration::from_secs(1),
            1024,
        ),
        Err(UpdateCheckError::Network)
    );
}
