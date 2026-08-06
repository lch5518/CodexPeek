use std::time::{Duration, SystemTime, UNIX_EPOCH};

use codex_usage_monitor::{
    ConsumptionPaceAssessment, ConsumptionPaceLevel, ConsumptionPaceMetrics, ForecastAnalysis,
    ForecastEngine, ForecastPolicy, ForecastQuality, ForecastResult, UsageProfileId, UsageSample,
    UsageWindow, WindowKind,
};

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn sample(
    profile_id: UsageProfileId,
    window_kind: WindowKind,
    used_percent: f64,
    resets_at: Option<SystemTime>,
    observed_at: SystemTime,
) -> UsageSample {
    sample_at(
        profile_id,
        window_kind,
        used_percent,
        resets_at,
        observed_at,
        at(10_000_000),
    )
}

fn sample_at(
    profile_id: UsageProfileId,
    window_kind: WindowKind,
    used_percent: f64,
    resets_at: Option<SystemTime>,
    observed_at: SystemTime,
    validation_now: SystemTime,
) -> UsageSample {
    UsageSample::new(
        profile_id,
        window_kind,
        used_percent,
        resets_at,
        observed_at,
        validation_now,
    )
    .unwrap()
}

fn window(used_percent: f64, resets_at: Option<SystemTime>) -> UsageWindow {
    UsageWindow::new(WindowKind::Primary, used_percent, Some(60), resets_at).unwrap()
}

fn calculate(
    samples: &[UsageSample],
    current_window: &UsageWindow,
    now: SystemTime,
) -> ForecastResult {
    ForecastEngine::calculate(
        samples,
        current_window,
        now,
        now,
        false,
        &ForecastPolicy::default(),
    )
}

fn analyze(
    samples: &[UsageSample],
    current_window: &UsageWindow,
    now: SystemTime,
) -> ForecastAnalysis {
    ForecastEngine::analyze(
        samples,
        current_window,
        now,
        now,
        false,
        &ForecastPolicy::default(),
    )
}

fn pace_for_rate(
    current_used_percent: f64,
    hourly_rate: f64,
    reset: SystemTime,
    now: SystemTime,
) -> ConsumptionPaceMetrics {
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            current_used_percent - hourly_rate * 2.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            current_used_percent - hourly_rate,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            current_used_percent,
            Some(reset),
            now,
        ),
    ];
    let ConsumptionPaceAssessment::Ready(metrics) =
        analyze(&samples, &window(current_used_percent, Some(reset)), now).pace
    else {
        panic!("expected ready pace");
    };
    metrics
}

fn evenly_spaced_samples(
    count: usize,
    observation_span: Duration,
    rise: f64,
    now: SystemTime,
) -> Vec<UsageSample> {
    assert!(count >= 2);
    let intervals = u64::try_from(count - 1).unwrap();
    (0..count)
        .map(|index| {
            let index = u64::try_from(index).unwrap();
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                10.0 + rise * index as f64 / intervals as f64,
                None,
                now - Duration::from_secs(
                    observation_span.as_secs() * (intervals - index) / intervals,
                ),
            )
        })
        .collect()
}

fn forecast_quality(
    samples: &[UsageSample],
    current_used_percent: f64,
    now: SystemTime,
) -> ForecastQuality {
    match calculate(samples, &window(current_used_percent, None), now) {
        ForecastResult::ForecastAvailable(forecast) => forecast.quality,
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn steady_usage_produces_a_low_quality_exhaustion_forecast() {
    let now = at(1_000_000);
    let reset = now + Duration::from_secs(12 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(3 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            40.0,
            Some(reset),
            now,
        ),
    ];

    match calculate(&samples, &window(40.0, Some(reset)), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert!((forecast.hourly_rate - 10.0).abs() < 0.000_001);
            assert_eq!(
                forecast.exhaustion_at,
                now + Duration::from_secs(6 * 60 * 60)
            );
            assert_eq!(forecast.exhausts_before_reset, Some(true));
            assert_eq!(forecast.expected_used_percent_at_reset, Some(100.0));
            assert_eq!(forecast.expected_remaining_percent_at_reset, Some(0.0));
            assert_eq!(forecast.sample_count, 4);
            assert_eq!(forecast.observation_span, Duration::from_secs(3 * 60 * 60));
            assert_eq!(forecast.quality, ForecastQuality::Low);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn low_activity_is_not_presented_as_a_forecast() {
    let now = at(1_100_000);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.5,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 10.9, None, now),
    ];

    assert_eq!(
        calculate(&samples, &window(10.9, None), now),
        ForecastResult::InsufficientActivity
    );
}

#[test]
fn low_activity_is_a_comfortable_pace_even_without_an_exhaustion_forecast() {
    let now = at(1_150_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.5,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.9,
            Some(reset),
            now,
        ),
    ];

    let analysis = analyze(&samples, &window(10.9, Some(reset)), now);

    assert_eq!(analysis.forecast, ForecastResult::InsufficientActivity);
    let ConsumptionPaceAssessment::Ready(metrics) = analysis.pace else {
        panic!("expected ready pace");
    };
    assert_eq!(metrics.level, ConsumptionPaceLevel::Comfortable);
    assert_eq!(metrics.sample_count, 3);
    assert_eq!(metrics.observation_span, Duration::from_secs(2 * 60 * 60));
    assert!((metrics.total_rise - 0.9).abs() < 0.000_001);
    assert!((metrics.hourly_rate - 0.45).abs() < 0.000_001);
    assert!((metrics.expected_remaining_percent_at_reset - 84.6).abs() < 0.000_001);
}

#[test]
fn plateau_samples_do_not_make_active_rate_zero() {
    let now = at(1_155_000);
    let reset = now + Duration::from_secs(12 * 60 * 60);
    let values = [
        18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0,
        18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 18.0, 24.0, 24.0, 25.0, 26.0, 27.0,
        28.0, 28.0,
    ];
    let observed_at = (0..25)
        .map(|index| {
            now - Duration::from_secs(9 * 60 * 60 - u64::try_from(index).unwrap() * 5 * 60)
        })
        .chain([
            now - Duration::from_secs(6 * 60 * 60),
            now - Duration::from_secs(5 * 60 * 60),
            now - Duration::from_secs(4 * 60 * 60),
            now - Duration::from_secs(3 * 60 * 60),
            now - Duration::from_secs(2 * 60 * 60),
            now - Duration::from_secs(60 * 60),
            now,
        ])
        .collect::<Vec<_>>();
    let samples: Vec<_> = values
        .into_iter()
        .zip(observed_at)
        .map(|(used_percent, observed_at)| {
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                used_percent,
                Some(reset),
                observed_at,
            )
        })
        .collect();

    let analysis = analyze(&samples, &window(28.0, Some(reset)), now);
    let ForecastResult::ForecastAvailable(forecast) = analysis.forecast else {
        panic!("expected an active forecast, got {:?}", analysis.forecast);
    };
    assert!(forecast.hourly_rate >= ForecastPolicy::MINIMUM_HOURLY_RATE);
    assert!(matches!(analysis.pace, ConsumptionPaceAssessment::Ready(_)));
}

#[test]
fn flat_usage_pace_is_explicitly_insufficient_activity() {
    let now = at(1_156_000);
    let reset = now + Duration::from_secs(12 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now,
        ),
    ];

    assert_eq!(
        analyze(&samples, &window(10.0, Some(reset)), now).pace,
        ConsumptionPaceAssessment::InsufficientActivity
    );
}

#[test]
fn pace_ratio_boundaries_are_normal_and_fast() {
    let now = at(1_160_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);

    let normal = pace_for_rate(50.0, 2.5, reset, now);
    let fast = pace_for_rate(50.0, 5.0, reset, now);

    assert!((normal.safe_hourly_rate - 5.0).abs() < 0.000_001);
    assert_eq!(normal.level, ConsumptionPaceLevel::Normal);
    assert_eq!(fast.level, ConsumptionPaceLevel::Fast);
    assert_eq!(fast.expected_remaining_percent_at_reset, 0.0);
}

#[test]
fn pace_measures_until_three_samples_and_thirty_minutes() {
    let now = at(1_170_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(20 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.5,
            Some(reset),
            now - Duration::from_secs(10 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            11.0,
            Some(reset),
            now,
        ),
    ];

    assert_eq!(
        analyze(&samples[..2], &window(10.5, Some(reset)), now).pace,
        ConsumptionPaceAssessment::Measuring {
            sample_count: 2,
            observation_span: Duration::from_secs(10 * 60),
        }
    );
    assert_eq!(
        analyze(&samples, &window(11.0, Some(reset)), now).pace,
        ConsumptionPaceAssessment::Measuring {
            sample_count: 3,
            observation_span: Duration::from_secs(20 * 60),
        }
    );
}

#[test]
fn pace_is_unavailable_without_a_future_reset() {
    let now = at(1_180_000);
    let past_reset = now - Duration::from_secs(1);

    assert_eq!(
        analyze(&[], &window(10.0, None), now).pace,
        ConsumptionPaceAssessment::Unavailable
    );
    assert_eq!(
        analyze(&[], &window(10.0, Some(past_reset)), now).pace,
        ConsumptionPaceAssessment::Unavailable
    );
}

#[test]
fn pace_marks_an_exhausted_window_before_sample_requirements() {
    let now = at(1_190_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);
    let analysis = analyze(&[], &window(100.0, Some(reset)), now);

    assert_eq!(analysis.forecast, ForecastResult::AlreadyExhausted);
    assert_eq!(analysis.pace, ConsumptionPaceAssessment::Exhausted);
}

#[test]
fn collection_requires_three_samples_and_a_thirty_minute_span() {
    let now = at(1_200_000);
    let short = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(20 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            11.0,
            None,
            now - Duration::from_secs(10 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 12.0, None, now),
    ];
    let few = &short[..2];

    assert!(matches!(
        calculate(few, &window(11.0, None), now),
        ForecastResult::Collecting { .. }
    ));
    assert!(matches!(
        calculate(&short, &window(12.0, None), now),
        ForecastResult::Collecting { .. }
    ));
}

#[test]
fn a_spike_does_not_distort_the_median_rate() {
    let now = at(1_300_000);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(5 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            None,
            now - Duration::from_secs(4 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            None,
            now - Duration::from_secs(3 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            40.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            50.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 90.0, None, now),
    ];

    match calculate(&samples, &window(90.0, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert!((forecast.hourly_rate - 10.0).abs() < 0.000_001);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn small_decreases_are_flattened_but_large_decreases_start_a_new_segment() {
    let now = at(1_400_000);
    let small_drop = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(3 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            29.5,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 31.0, None, now),
    ];
    let reset_segment = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            70.0,
            None,
            now - Duration::from_secs(5 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            80.0,
            None,
            now - Duration::from_secs(4 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 30.0, None, now),
    ];

    match calculate(&small_drop, &window(31.0, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert!((forecast.hourly_rate - 4.0).abs() < 0.000_001)
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
    match calculate(&reset_segment, &window(30.0, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.sample_count, 3);
            assert!((forecast.hourly_rate - 10.0).abs() < 0.000_001);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn reset_matching_and_missing_reset_are_handled_without_mixing_periods() {
    let now = at(1_500_000);
    let old_reset = now + Duration::from_secs(2 * 60 * 60);
    let reset = now + Duration::from_secs(10 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            80.0,
            Some(old_reset),
            now - Duration::from_secs(3 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            Some(reset),
            now,
        ),
    ];

    assert!(matches!(
        calculate(&samples, &window(30.0, Some(reset)), now),
        ForecastResult::ForecastAvailable(_)
    ));
    assert!(matches!(
        calculate(&samples, &window(30.0, None), now),
        ForecastResult::Collecting { .. }
    ));
}

#[test]
fn exhaustion_and_reset_comparison_cover_both_sides_of_the_reset() {
    let now = at(1_600_000);
    let reset = now + Duration::from_secs(3 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            40.0,
            Some(reset),
            now,
        ),
    ];
    let exhausted = window(100.0, None);
    let after_reset = window(40.0, Some(reset));

    assert_eq!(
        calculate(&samples, &exhausted, now),
        ForecastResult::AlreadyExhausted
    );
    match calculate(&samples, &after_reset, now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.exhausts_before_reset, Some(false));
            assert_eq!(forecast.expected_used_percent_at_reset, Some(70.0));
            assert_eq!(forecast.expected_remaining_percent_at_reset, Some(30.0));
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn stale_and_invalid_inputs_do_not_produce_a_forecast() {
    let now = at(1_700_000);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 30.0, None, now),
    ];
    let future = sample(
        UsageProfileId::System,
        WindowKind::Primary,
        40.0,
        None,
        now + Duration::from_secs(60 * 60),
    );
    let mixed_profile = sample(
        UsageProfileId::Managed(7),
        WindowKind::Primary,
        40.0,
        None,
        now,
    );
    let invalid_window = UsageWindow {
        kind: WindowKind::Primary,
        used_percent: f64::NAN,
        window_duration_mins: None,
        resets_at: None,
    };

    assert_eq!(
        ForecastEngine::calculate(
            &samples,
            &window(30.0, None),
            now - Duration::from_secs(11 * 60),
            now,
            false,
            &ForecastPolicy::default()
        ),
        ForecastResult::Stale
    );
    assert!(matches!(
        ForecastEngine::calculate(
            &[
                samples[0].clone(),
                future,
                samples[1].clone(),
                samples[2].clone()
            ],
            &window(30.0, None),
            now,
            now,
            false,
            &ForecastPolicy::default()
        ),
        ForecastResult::ForecastAvailable(_)
    ));
    assert_eq!(
        calculate(
            &[samples[0].clone(), mixed_profile, samples[1].clone()],
            &window(30.0, None),
            now
        ),
        ForecastResult::Invalid
    );
    assert_eq!(
        calculate(&samples, &invalid_window, now),
        ForecastResult::Invalid
    );
}

#[test]
fn recent_thirty_two_samples_bound_pair_calculation() {
    let now = at(1_800_000);
    let samples: Vec<_> = (0..40_u64)
        .map(|index| {
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                index as f64,
                None,
                now - Duration::from_secs((39 - index) * 5 * 60),
            )
        })
        .collect();

    match calculate(&samples, &window(39.0, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.sample_count, 32);
            assert_eq!(forecast.sample_count * (forecast.sample_count - 1) / 2, 496);
            assert!((forecast.hourly_rate - 12.0).abs() < 0.000_001);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn conflicting_same_time_samples_choose_the_highest_value_independent_of_input_order() {
    let now = at(1_900_000);
    let first_order = [
        sample(UsageProfileId::System, WindowKind::Primary, 30.0, None, now),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 99.0, None, now),
    ];
    let reverse_order = [
        sample(UsageProfileId::System, WindowKind::Primary, 99.0, None, now),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 30.0, None, now),
    ];

    let forecast_for = |samples: &[UsageSample]| match calculate(samples, &window(99.0, None), now)
    {
        ForecastResult::ForecastAvailable(forecast) => forecast,
        result => panic!("expected available forecast, got {result:?}"),
    };
    let first = forecast_for(&first_order);
    let reverse = forecast_for(&reverse_order);

    for forecast in [first, reverse] {
        assert_eq!(forecast.sample_count, 3);
        assert!((forecast.hourly_rate - 44.5).abs() < 0.000_001);
    }
}

#[test]
fn very_slow_and_very_fast_rates_have_deterministic_outcomes() {
    let now = at(2_000_000);
    let slow = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(25 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.55,
            None,
            now - Duration::from_secs(12 * 60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 11.1, None, now),
    ];
    let fast = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            1.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            50.0,
            None,
            now - Duration::from_secs(30 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 99.9, None, now),
    ];

    assert_eq!(
        calculate(&slow, &window(11.1, None), now),
        ForecastResult::InsufficientActivity
    );
    assert!(matches!(
        calculate(&fast, &window(99.9, None), now),
        ForecastResult::ForecastAvailable(_)
    ));
}

#[test]
fn mixed_windows_and_explicit_stale_state_are_rejected_safely() {
    let now = at(2_100_000);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Secondary,
            20.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
        sample(UsageProfileId::System, WindowKind::Primary, 30.0, None, now),
    ];

    assert_eq!(
        calculate(&samples, &window(30.0, None), now),
        ForecastResult::Invalid
    );
    assert_eq!(
        ForecastEngine::calculate(
            &samples[..1],
            &window(10.0, None),
            now,
            now,
            true,
            &ForecastPolicy::default()
        ),
        ForecastResult::Stale
    );
}

#[test]
fn eight_samples_over_two_hours_are_high_quality() {
    let now = at(2_200_000);
    let samples: Vec<_> = (0..8_u64)
        .map(|index| {
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                10.0 + index as f64,
                None,
                now - Duration::from_secs((7 - index) * 60 * 60),
            )
        })
        .collect();

    match calculate(&samples, &window(17.0, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.quality, ForecastQuality::High)
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn expired_reset_omits_reset_comparison_without_discarding_the_matching_period() {
    let now = at(2_300_000);
    let expired_reset = now - Duration::from_secs(60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(expired_reset),
            now - Duration::from_secs(3 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            Some(expired_reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            Some(expired_reset),
            now - Duration::from_secs(60 * 60),
        ),
    ];

    match calculate(&samples, &window(30.0, Some(expired_reset)), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.exhausts_before_reset, None);
            assert_eq!(forecast.expected_used_percent_at_reset, None);
            assert_eq!(forecast.expected_remaining_percent_at_reset, None);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn signed_zero_and_pre_epoch_current_inputs_are_invalid() {
    let now = at(2_400_000);
    let signed_zero = UsageWindow {
        kind: WindowKind::Primary,
        used_percent: -0.0,
        window_duration_mins: None,
        resets_at: None,
    };
    let pre_epoch_reset = UsageWindow {
        kind: WindowKind::Primary,
        used_percent: 10.0,
        window_duration_mins: None,
        resets_at: Some(UNIX_EPOCH - Duration::from_secs(1)),
    };

    assert_eq!(calculate(&[], &signed_zero, now), ForecastResult::Invalid);
    assert_eq!(
        calculate(&[], &pre_epoch_reset, now),
        ForecastResult::Invalid
    );
    assert_eq!(
        ForecastEngine::calculate(
            &[],
            &window(10.0, None),
            UNIX_EPOCH - Duration::from_secs(1),
            UNIX_EPOCH - Duration::from_secs(1),
            false,
            &ForecastPolicy::default(),
        ),
        ForecastResult::Invalid
    );
}

#[test]
fn reversed_sample_order_is_sorted_before_rate_calculation() {
    let now = at(2_450_000);
    let samples = [
        sample(UsageProfileId::System, WindowKind::Primary, 30.0, None, now),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            None,
            now - Duration::from_secs(60 * 60),
        ),
    ];

    match calculate(&samples, &window(30.0, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert!((forecast.hourly_rate - 10.0).abs() < 0.000_001);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn old_missing_reset_samples_are_not_mixed_after_reset_initialization() {
    let now = at(2_500_000);
    let reset = now + Duration::from_secs(8 * 60 * 60);
    let samples = [
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            80.0,
            None,
            now - Duration::from_secs(4 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(reset),
            now - Duration::from_secs(2 * 60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            20.0,
            Some(reset),
            now - Duration::from_secs(60 * 60),
        ),
        sample(
            UsageProfileId::System,
            WindowKind::Primary,
            30.0,
            Some(reset),
            now,
        ),
    ];

    match calculate(&samples, &window(30.0, Some(reset)), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.sample_count, 3);
            assert!((forecast.hourly_rate - 10.0).abs() < 0.000_001);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}

#[test]
fn exact_stale_boundary_is_fresh_and_quality_thresholds_are_explainable() {
    let now = at(2_600_000);
    let policy = ForecastPolicy::with_refresh_interval(Duration::from_secs(15 * 60));
    let stale_boundary = now - policy.stale_after();
    let medium: Vec<_> = (0..5_u64)
        .map(|index| {
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                10.0 + index as f64,
                None,
                now - Duration::from_secs((4 - index) * 15 * 60),
            )
        })
        .collect();

    assert!(matches!(
        ForecastEngine::calculate(
            &medium,
            &window(14.0, None),
            stale_boundary,
            now,
            false,
            &policy,
        ),
        ForecastResult::ForecastAvailable(forecast) if forecast.quality == ForecastQuality::Medium
    ));
}

#[test]
fn exhaustion_timestamp_overflow_returns_invalid() {
    let mut seconds = 0_u64;
    for bit in (0..64).rev() {
        let candidate = seconds | (1_u64 << bit);
        if UNIX_EPOCH
            .checked_add(Duration::from_secs(candidate))
            .is_some()
        {
            seconds = candidate;
        }
    }
    let latest = UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .expect("binary search retained a representable timestamp");
    let now = latest - Duration::from_secs(3 * 60 * 60);
    let samples = [
        sample_at(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            None,
            now - Duration::from_secs(2 * 60 * 60),
            now,
        ),
        sample_at(
            UsageProfileId::System,
            WindowKind::Primary,
            11.0,
            None,
            now - Duration::from_secs(60 * 60),
            now,
        ),
        sample_at(
            UsageProfileId::System,
            WindowKind::Primary,
            12.0,
            None,
            now,
            now,
        ),
    ];

    assert_eq!(
        calculate(&samples, &window(12.0, None), now),
        ForecastResult::Invalid
    );
}

#[test]
fn samples_with_resets_before_the_observation_are_rejected_at_the_public_boundary() {
    let now = at(3_000_000);

    assert_eq!(
        UsageSample::new(
            UsageProfileId::System,
            WindowKind::Primary,
            10.0,
            Some(now - Duration::from_secs(1)),
            now,
            now,
        ),
        Err(codex_usage_monitor::UsageHistoryError::ReversedTimestamps)
    );
}

#[test]
fn quality_boundaries_distinguish_just_below_and_exact_v1_thresholds() {
    let now = at(3_100_000);
    let thirty_minutes = Duration::from_secs(30 * 60);
    let one_hour = Duration::from_secs(60 * 60);
    let two_hours = Duration::from_secs(2 * 60 * 60);

    let low = evenly_spaced_samples(3, thirty_minutes, 1.0, now);
    assert!(matches!(
        calculate(&low[..2], &window(10.5, None), now),
        ForecastResult::Collecting { .. }
    ));
    assert!(matches!(
        calculate(
            &evenly_spaced_samples(3, thirty_minutes - Duration::from_secs(1), 1.0, now),
            &window(11.0, None),
            now
        ),
        ForecastResult::Collecting { .. }
    ));
    assert_eq!(
        calculate(
            &evenly_spaced_samples(3, thirty_minutes, 0.999, now),
            &window(10.999, None),
            now
        ),
        ForecastResult::InsufficientActivity
    );
    assert_eq!(forecast_quality(&low, 11.0, now), ForecastQuality::Low);

    assert_eq!(
        forecast_quality(&evenly_spaced_samples(4, one_hour, 2.0, now), 12.0, now),
        ForecastQuality::Low
    );
    assert_eq!(
        forecast_quality(
            &evenly_spaced_samples(5, one_hour - Duration::from_secs(1), 2.0, now),
            12.0,
            now
        ),
        ForecastQuality::Low
    );
    assert_eq!(
        forecast_quality(&evenly_spaced_samples(5, one_hour, 1.999, now), 11.999, now),
        ForecastQuality::Low
    );
    assert_eq!(
        forecast_quality(&evenly_spaced_samples(5, one_hour, 2.0, now), 12.0, now),
        ForecastQuality::Medium
    );

    assert_eq!(
        forecast_quality(&evenly_spaced_samples(7, two_hours, 5.0, now), 15.0, now),
        ForecastQuality::Medium
    );
    assert_eq!(
        forecast_quality(
            &evenly_spaced_samples(8, two_hours - Duration::from_secs(1), 5.0, now),
            15.0,
            now
        ),
        ForecastQuality::Medium
    );
    assert_eq!(
        forecast_quality(
            &evenly_spaced_samples(8, two_hours, 4.999, now),
            14.999,
            now
        ),
        ForecastQuality::Medium
    );
    assert_eq!(
        forecast_quality(&evenly_spaced_samples(8, two_hours, 5.0, now), 15.0, now),
        ForecastQuality::High
    );
}

#[test]
fn long_history_uses_only_the_recent_thirty_two_samples_for_theil_sen_pairs() {
    let now = at(3_200_000);
    let samples: Vec<_> = (0..100_u64)
        .map(|index| {
            let used_percent = if index < 68 {
                index as f64
            } else {
                67.0 + (index - 67) as f64 * 0.1
            };
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                used_percent,
                None,
                now - Duration::from_secs((99 - index) * 60 * 60),
            )
        })
        .collect();

    match calculate(&samples, &window(70.2, None), now) {
        ForecastResult::ForecastAvailable(forecast) => {
            assert_eq!(forecast.sample_count, 32);
            assert_eq!(forecast.sample_count * (forecast.sample_count - 1) / 2, 496);
            assert!((forecast.hourly_rate - 0.1).abs() < 0.000_001);
        }
        result => panic!("expected available forecast, got {result:?}"),
    }
}
