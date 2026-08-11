use std::{
    io::{self, Write},
    net::TcpListener,
    sync::Mutex,
    thread,
    time::{Duration, SystemTime},
};

use codex_usage_monitor::{
    AvailableUpdate, HttpResponse, ReleaseHttpClient, UpdateCheckError, UpdateCheckIntent,
    UpdateCheckNotice, UpdateCheckStart, UpdateChecker, UpdatePresentation,
    UpdatePresentationStatus, UpdateUserAction, UreqHttpClient,
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
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Current)
    );
    assert!(presentation.take_user_notice().is_none());
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
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Failed)
    );
    assert!(presentation.take_user_notice().is_none());
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
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Available(update.clone()))
    );
    assert!(presentation.take_user_notice().is_none());
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
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Current)
    );
    assert!(presentation.take_user_notice().is_none());
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
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Failed)
    );
    assert!(presentation.take_user_notice().is_none());
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn automatic_update_results_prompt_once_without_requesting_browser_open() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.0.0");

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );
    presentation.record_result(Ok(Some(update.clone())));

    assert_eq!(presentation.available_update(), Some(update.clone()));
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Available(update))
    );
    assert!(presentation.take_user_notice().is_none());
    assert!(presentation.take_open_request().is_none());
}

#[test]
fn automatic_check_suppresses_only_the_exact_dismissed_version() {
    let presentation = UpdatePresentation::default();
    let dismissed = available_update("2.0.0");

    presentation.begin_check(UpdateCheckIntent::Automatic);
    presentation.record_result_with_dismissed_version(Ok(Some(dismissed.clone())), Some("2.0.0"));

    assert_eq!(presentation.available_update(), Some(dismissed));
    assert!(presentation.take_user_notice().is_none());

    let newer = available_update("2.1.0");
    presentation.begin_check(UpdateCheckIntent::Automatic);
    presentation.record_result_with_dismissed_version(Ok(Some(newer.clone())), Some("2.0.0"));

    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Available(newer))
    );
}

#[test]
fn manual_check_ignores_the_dismissed_version() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.0.0");

    presentation.begin_check(UpdateCheckIntent::UserInitiated);
    presentation.record_result_with_dismissed_version(Ok(Some(update.clone())), Some("2.0.0"));

    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Available(update))
    );
}

#[test]
fn dismissal_and_install_queue_accept_only_the_stored_update() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.4.0");
    presentation.begin_check(UpdateCheckIntent::Automatic);
    presentation.record_result(Ok(Some(update.clone())));

    assert!(!presentation.dismiss_available_version("2.3.0"));
    assert!(presentation.dismiss_available_version("2.4.0"));
    assert!(!presentation.queue_install_request(available_update("2.3.0")));
    assert!(presentation.queue_install_request(update.clone()));
    assert!(!presentation.queue_install_request(update.clone()));
    assert_eq!(presentation.take_install_request(), Some(update.clone()));
    assert!(presentation.take_install_request().is_none());
    assert!(!presentation.queue_install_request(update));
}

#[test]
fn starting_another_check_does_not_discard_an_unconsumed_user_notice() {
    let presentation = UpdatePresentation::default();
    presentation.begin_check(UpdateCheckIntent::UserInitiated);
    presentation.record_result(Ok(None));

    assert_eq!(
        presentation.begin_check(UpdateCheckIntent::Automatic),
        UpdateCheckStart::Started
    );
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Current)
    );
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

    assert_eq!(presentation.take_open_request(), Some(update.clone()));
    assert!(presentation.take_open_request().is_none());
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Available(update))
    );
    assert!(presentation.take_user_notice().is_none());
}

#[test]
fn queued_user_notice_is_consumed_once_by_the_ui_boundary() {
    let presentation = UpdatePresentation::default();

    presentation.queue_user_notice(UpdateCheckNotice::Current);

    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::Current)
    );
    assert!(presentation.take_user_notice().is_none());
}

#[test]
fn install_preparation_exposes_downloading_ready_and_failure_states() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.0.0");
    presentation.begin_check(UpdateCheckIntent::Automatic);
    presentation.record_result(Ok(Some(update.clone())));
    let _ = presentation.take_user_notice();

    assert!(presentation.queue_install_request(update.clone()));
    assert_eq!(presentation.status(), UpdatePresentationStatus::Downloading);
    assert_eq!(presentation.take_install_request(), Some(update.clone()));
    assert_eq!(
        presentation.begin_user_action(),
        UpdateUserAction::WaitForRunning
    );
    presentation.record_install_notice(UpdateCheckNotice::InstallReady);
    assert_eq!(presentation.status(), UpdatePresentationStatus::Installing);
    assert!(!presentation.queue_install_request(update.clone()));
    assert_eq!(
        presentation.begin_user_action(),
        UpdateUserAction::WaitForRunning
    );
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::InstallReady)
    );
    assert!(!presentation.queue_install_request(update));
}

#[test]
fn failed_install_preparation_releases_the_install_latch_for_retry() {
    let presentation = UpdatePresentation::default();
    let update = available_update("2.0.0");
    presentation.begin_check(UpdateCheckIntent::Automatic);
    presentation.record_result(Ok(Some(update.clone())));
    let _ = presentation.take_user_notice();

    assert!(presentation.queue_install_request(update.clone()));
    assert_eq!(presentation.take_install_request(), Some(update.clone()));
    presentation.record_install_notice(UpdateCheckNotice::VerificationFailed);
    assert_eq!(presentation.status(), UpdatePresentationStatus::Failed);
    assert_eq!(
        presentation.take_user_notice(),
        Some(UpdateCheckNotice::VerificationFailed)
    );
    assert!(presentation.queue_install_request(update));
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
fn invalid_version_prefix_fails_the_check() {
    let checker = UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo"), 1024).unwrap();
    let client = FakeClient::new(HttpResponse {
        status: 200,
        body: br#"{"tag_name":"vv2.0.0","html_url":"https://github.com/owner/repo/releases/tag/vv2.0.0"}"#.to_vec(),
    });

    assert_eq!(
        checker.check_if_due(
            &client,
            None,
            SystemTime::UNIX_EPOCH + Duration::from_secs(100_000)
        ),
        Err(UpdateCheckError::Network)
    );
}

#[test]
fn tag_must_match_the_final_browser_url_segment_policy() {
    let checker = UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo"), 1024).unwrap();
    let client = FakeClient::new(HttpResponse {
        status: 200,
        body: br#"{"tag_name":"v2.0.0+build","html_url":"https://github.com/owner/repo/releases/tag/v2.0.0+build"}"#
            .to_vec(),
    });

    assert_eq!(
        checker.check_if_due(
            &client,
            None,
            SystemTime::UNIX_EPOCH + Duration::from_secs(100_000)
        ),
        Err(UpdateCheckError::Network)
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
fn malformed_oversized_non_success_and_unsafe_responses_fail_the_check() {
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
        assert_eq!(
            checker.check_if_due(
                &client,
                None,
                SystemTime::UNIX_EPOCH + Duration::from_secs(100_000)
            ),
            Err(UpdateCheckError::Network)
        );
    }
}

#[test]
fn release_url_must_be_the_exact_tag_page() {
    let checker = UpdateChecker::new("1.0.0", Some("https://github.com/owner/repo"), 1024).unwrap();
    let client = FakeClient::new(HttpResponse {
        status: 200,
        body: br#"{"tag_name":"2.0.0","html_url":"https://github.com/owner/repo/releases/tag/v2.0.0/assets"}"#.to_vec(),
    });

    assert_eq!(
        checker.check_if_due(
            &client,
            None,
            SystemTime::UNIX_EPOCH + Duration::from_secs(100_000)
        ),
        Err(UpdateCheckError::Network)
    );
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

#[test]
fn production_http_client_maps_tls_handshake_failure_to_network_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.write_all(b"not a TLS handshake");
                    return;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::yield_now();
                }
                Err(_) => return,
            }
        }
    });

    let result = UreqHttpClient.get(
        &format!("https://{address}/"),
        "CodexUsageMonitor/test",
        Duration::from_secs(1),
        1024,
    );
    server.join().unwrap();

    assert_eq!(result, Err(UpdateCheckError::Network));
}
