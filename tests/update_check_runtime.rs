use std::{
    sync::Mutex,
    time::{Duration, SystemTime},
};

use codex_usage_monitor::{
    AvailableUpdate, HttpResponse, ReleaseHttpClient, UpdateCheckError, UpdateCheckIntent,
    UpdateCheckStart, UpdateChecker, UpdatePresentation, UpdatePresentationStatus,
    UpdateUserAction, UreqHttpClient,
};
use semver::Version;

struct FakeClient {
    response: HttpResponse,
    requests: Mutex<Vec<(String, String, Duration, usize)>>,
}

fn available_update(version: &str) -> AvailableUpdate {
    AvailableUpdate {
        version: Version::parse(version).unwrap(),
        release_url: format!("https://github.com/owner/repo/releases/tag/v{version}"),
    }
}

#[test]
fn user_check_without_an_update_reports_current() {
    let presentation = UpdatePresentation::default();

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::Started
    );
    assert_eq!(presentation.status(), UpdatePresentationStatus::Checking);
    presentation.record_result(Ok(None));

    assert_eq!(presentation.status(), UpdatePresentationStatus::Current);
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn user_check_network_error_reports_failed() {
    let presentation = UpdatePresentation::default();

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::Started
    );
    presentation.record_result(Err(UpdateCheckError::Network));

    assert_eq!(presentation.status(), UpdatePresentationStatus::Failed);
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn user_intent_during_automatic_check_opens_available_result_once_without_duplicate_start() {
    let presentation = UpdatePresentation::default();
    let update = available_update("3.0.0");

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::AlreadyRunning
    );
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::AlreadyRunning
    );
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::AlreadyRunning
    );
    presentation.record_result(Ok(Some(update.clone())));

    assert_eq!(presentation.status(), UpdatePresentationStatus::Available);
    assert_eq!(presentation.take_open_request(), Some(update.clone()));
    assert!(presentation.take_open_request().is_none());
    presentation.record_result(Ok(Some(available_update("4.0.0"))));
    assert_eq!(presentation.available_update(), Some(update));
}

#[test]
fn user_intent_during_automatic_check_reports_current_without_opening() {
    let presentation = UpdatePresentation::default();
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::AlreadyRunning
    );

    presentation.record_result(Ok(None));

    assert_eq!(presentation.status(), UpdatePresentationStatus::Current);
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn user_intent_during_automatic_check_reports_failure_without_opening() {
    let presentation = UpdatePresentation::default();
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::AlreadyRunning
    );

    presentation.record_result(Err(UpdateCheckError::Network));

    assert_eq!(presentation.status(), UpdatePresentationStatus::Failed);
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn automatic_update_results_are_visible_without_requesting_browser_open() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.0.0");

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );
    presentation.record_result(Ok(Some(update.clone())));

    assert_eq!(presentation.available_update(), Some(update));
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn user_initiated_results_create_exactly_one_open_request() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.1.0");

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::UserInitiated),
        UpdateCheckStart::Started
    );
    presentation.record_result(Ok(Some(update.clone())));

    assert_eq!(presentation.take_open_request(), Some(update));
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn explicit_open_actions_use_only_the_stored_validated_result() {
    let presentation = UpdatePresentation::default();
    assert_eq!(
        presentation.begin_user_action(),
        UpdateUserAction::StartCheck
    );
    presentation.record_result(Ok(None));

    let update = available_update("2.2.0");
    presentation.begin_check(UpdateCheckIntent::Automatic);
    presentation.record_result(Ok(Some(update.clone())));

    assert_eq!(
        presentation.begin_user_action(),
        UpdateUserAction::Open(update)
    );
}

#[test]
fn explicit_action_atomically_joins_a_running_automatic_check() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.3.0");
    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );

    assert_eq!(
        presentation.begin_user_action(),
        UpdateUserAction::WaitForRunning
    );
    presentation.record_result(Ok(Some(update.clone())));

    assert_eq!(presentation.take_open_request(), Some(update));
    assert!(presentation.take_open_request().is_none());
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
    for repository_url in [
        "https://github.com/./repo",
        "https://github.com/owner/.",
        "https://github.com/../repo",
        "https://github.com/owner/..",
    ] {
        assert!(UpdateChecker::new("0.1.0", Some(repository_url), 1024).is_none());
    }
}

#[test]
fn tag_name_may_have_one_v_prefix_but_not_multiple_prefixes() {
    let checker = UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo"), 1024).unwrap();
    let client = FakeClient::new(HttpResponse {
        status: 200,
        body: br#"{"tag_name":"vv2.0.0","html_url":"https://github.com/owner/repo/releases/tag/vv2.0.0"}"#.to_vec(),
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
