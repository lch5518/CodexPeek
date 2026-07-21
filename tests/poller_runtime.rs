use std::time::{Duration, SystemTime};

use codex_usage_monitor::{PollState, PollTrigger};

#[test]
fn manual_refresh_has_a_ten_second_cooldown() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let mut state = PollState::new(5, now).unwrap();
    assert_eq!(state.begin(PollTrigger::Manual, now), Some(false));
    state.finish(Ok(None), now);
    assert_eq!(
        state.begin(PollTrigger::Manual, now + Duration::from_secs(9)),
        None
    );
    assert_eq!(
        state.begin(PollTrigger::Manual, now + Duration::from_secs(10)),
        Some(false)
    );
}

#[test]
fn failures_back_off_without_losing_last_success_and_staleness_is_interval_based() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let mut state = PollState::new(5, now).unwrap();
    assert_eq!(state.begin(PollTrigger::Automatic, now), Some(false));
    state.finish(Ok(None), now);
    assert!(!state.snapshot(now + Duration::from_secs(599)).is_stale);
    assert!(state.snapshot(now + Duration::from_secs(600)).is_stale);

    assert_eq!(
        state.begin(PollTrigger::Automatic, now + Duration::from_secs(300)),
        Some(false)
    );
    state.finish(Err(()), now + Duration::from_secs(300));
    assert_eq!(state.next_poll_at(), now + Duration::from_secs(360));
    assert_eq!(
        state.begin(PollTrigger::Automatic, now + Duration::from_secs(359)),
        None
    );
    assert_eq!(
        state.begin(PollTrigger::Automatic, now + Duration::from_secs(360)),
        Some(false)
    );
}

#[test]
fn reset_timestamp_can_schedule_one_immediate_refresh_only_once() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let reset = now - Duration::from_secs(1);
    let mut state = PollState::new(5, now).unwrap();
    assert_eq!(state.begin(PollTrigger::Reset(reset), now), Some(false));
    state.finish(Err(()), now);
    assert_eq!(
        state.begin(PollTrigger::Reset(reset), now + Duration::from_secs(1)),
        None
    );
    assert_eq!(
        state.begin(
            PollTrigger::Reset(reset + Duration::from_secs(1)),
            now + Duration::from_secs(1)
        ),
        Some(false)
    );
}

#[test]
fn forced_auth_requests_refresh_and_future_reset_advances_schedule() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let mut state = PollState::new(15, now).unwrap();
    assert_eq!(state.begin(PollTrigger::ForcedAuth, now), Some(true));
    state.finish(Ok(None), now);
    let reset = now + Duration::from_secs(60);
    assert_eq!(
        state.begin(PollTrigger::Reset(reset), now + Duration::from_secs(1)),
        None
    );
    assert_eq!(state.next_poll_at(), reset);
}
