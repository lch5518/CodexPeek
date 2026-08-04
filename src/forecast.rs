use std::{
    cmp::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{UsageSample, UsageWindow};

/// 사용량 소진 예측에 적용하는 고정 v1 기준을 나타냅니다.
///
/// 폴링 간격만 호출자가 지정할 수 있으며, 표본 수·관측 기간·상승량·최소 속도 기준은 v1에서
/// 고정됩니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForecastPolicy {
    refresh_interval: Duration,
}

impl ForecastPolicy {
    /// 예측에 필요한 최소 표본 수입니다.
    pub const MINIMUM_SAMPLES: usize = 3;
    /// 예측에 필요한 최소 관측 기간입니다.
    pub const MINIMUM_OBSERVATION_SPAN: Duration = Duration::from_secs(30 * 60);
    /// 활동으로 간주하는 최소 총 사용량 상승폭(퍼센트 포인트)입니다.
    pub const MINIMUM_TOTAL_RISE: f64 = 1.0;
    /// 반올림 또는 서버 지연으로 평탄화하는 최대 하락폭(퍼센트 포인트)입니다.
    pub const NOISE_DECREASE: f64 = 1.0;
    /// 예측을 표시하는 최소 시간당 사용량 상승폭(퍼센트 포인트)입니다.
    pub const MINIMUM_HOURLY_RATE: f64 = 0.05;
    /// Theil-Sen 기울기 계산에 사용하는 최대 최근 표본 수입니다.
    pub const MAX_SAMPLES: usize = 32;

    /// 지정한 폴링 간격을 이용해 v1 정책을 생성합니다.
    ///
    /// `refresh_interval`은 마지막 성공 조회의 신선도 한계 계산에만 사용하며, 0초이면
    /// 최소 10분 신선도 한계가 적용됩니다.
    pub const fn new(refresh_interval: Duration) -> Self {
        Self { refresh_interval }
    }

    /// 지정한 폴링 간격을 이용해 v1 정책을 생성합니다.
    pub const fn with_refresh_interval(refresh_interval: Duration) -> Self {
        Self::new(refresh_interval)
    }

    /// 마지막 성공 조회가 오래되었다고 판정하는 기간을 반환합니다.
    pub fn stale_after(self) -> Duration {
        self.refresh_interval
            .checked_mul(2)
            .unwrap_or(Duration::MAX)
            .max(Duration::from_secs(10 * 60))
    }
}

impl Default for ForecastPolicy {
    /// 기본 5분 폴링 간격으로 v1 정책을 생성합니다.
    fn default() -> Self {
        Self::new(Duration::from_secs(5 * 60))
    }
}

/// 아직 예측을 만들기에 관측이 부족한 이유를 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForecastCollectionReason {
    /// 같은 초기화 구간의 유효 표본이 세 개보다 적습니다.
    TooFewSamples,
    /// 표본 수는 충분하지만 첫 표본부터 마지막 표본까지 30분보다 짧습니다.
    ObservationSpanTooShort,
}

/// 표본량·관측 기간·상승량으로만 분류한 예측 품질입니다.
///
/// 이 값은 통계적 확률 또는 보장 수준이 아닙니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForecastQuality {
    /// 최소 기준만 충족한 예측입니다.
    Low,
    /// 표본 다섯 개, 한 시간, 2퍼센트 포인트 상승을 충족한 예측입니다.
    Medium,
    /// 표본 여덟 개, 두 시간, 5퍼센트 포인트 상승을 충족한 예측입니다.
    High,
}

/// 다음 초기화까지 한도를 유지할 수 있는 속도와 비교한 소비 속도 단계입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumptionPaceLevel {
    /// 현재 속도가 초기화 전 소진 속도의 절반보다 느립니다.
    Comfortable,
    /// 현재 속도로 초기화 전에는 버티지만 여유가 크지 않습니다.
    Normal,
    /// 현재 속도라면 초기화 전에 한도를 소진할 수 있습니다.
    Fast,
}

/// 사용자가 소비 속도를 이해하는 데 필요한 검증된 관측 지표입니다.
///
/// 모든 비율은 app-server가 제공한 사용률에서 파생한 퍼센트 포인트이며 토큰 수나 계정 식별
/// 정보는 포함하지 않습니다.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsumptionPaceMetrics {
    /// 초기화 전 소진 위험을 기준으로 한 소비 속도 단계입니다.
    pub level: ConsumptionPaceLevel,
    /// 현재 연속 관측 구간에서 계산에 사용한 표본 수입니다.
    pub sample_count: usize,
    /// 첫 표본부터 마지막 표본까지의 관측 시간입니다.
    pub observation_span: Duration,
    /// 관측 구간의 총 사용량 증가폭입니다.
    pub total_rise: f64,
    /// Theil-Sen 중앙 기울기로 계산한 시간당 소비율입니다.
    pub hourly_rate: f64,
    /// 현재 잔량을 다음 초기화까지 유지할 수 있는 최대 시간당 소비율입니다.
    pub safe_hourly_rate: f64,
    /// 현재 속도가 유지될 때 다음 초기화 시점에 남을 것으로 계산한 사용량입니다.
    pub expected_remaining_percent_at_reset: f64,
}

/// 소비 속도 분석이 사용자에게 제공할 수 있는 상태입니다.
#[derive(Clone, Debug, PartialEq)]
pub enum ConsumptionPaceAssessment {
    /// 표본 수 또는 관측 시간이 부족해 속도를 더 측정해야 합니다.
    Measuring {
        /// 현재 연속 관측 구간의 표본 수입니다.
        sample_count: usize,
        /// 현재 연속 관측 구간의 관측 시간입니다.
        observation_span: Duration,
    },
    /// 초기화 기준 소비 속도와 관측 지표를 계산했습니다.
    Ready(ConsumptionPaceMetrics),
    /// 현재 사용량이 이미 100% 이상입니다.
    Exhausted,
    /// 초기화 시각, 신선도 또는 입력 검증 문제로 속도를 판단할 수 없습니다.
    Unavailable,
}

/// 계산 가능한 사용량 소진 예측입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct Forecast {
    /// Theil-Sen 중앙값으로 계산한 시간당 사용량 상승폭(퍼센트 포인트)입니다.
    pub hourly_rate: f64,
    /// 현재 사용량이 100%에 도달할 것으로 계산된 시각입니다.
    pub exhaustion_at: SystemTime,
    /// 다음 초기화 전에 소진하는지 여부이며, 초기화 시각이 없거나 지났으면 `None`입니다.
    pub exhausts_before_reset: Option<bool>,
    /// 다음 초기화 시점의 예상 사용량이며, 비교할 초기화 시각이 없으면 `None`입니다.
    pub expected_used_percent_at_reset: Option<f64>,
    /// 다음 초기화 시점의 예상 잔여 사용량이며, 비교할 초기화 시각이 없으면 `None`입니다.
    pub expected_remaining_percent_at_reset: Option<f64>,
    /// 실제 기울기 계산에 사용한 현재 연속 구간의 표본 수입니다.
    pub sample_count: usize,
    /// 실제 기울기 계산에 사용한 현재 연속 구간의 관측 기간입니다.
    pub observation_span: Duration,
    /// 표본 수·관측 기간·상승량만으로 정한 설명용 품질입니다.
    pub quality: ForecastQuality,
}

/// 순수 예측 계산의 결과를 나타냅니다.
#[derive(Clone, Debug, PartialEq)]
pub enum ForecastResult {
    /// 유효한 현재 구간 표본을 더 수집해야 합니다.
    Collecting {
        /// 현재 초기화 구간에서 정제 후 남은 표본 수입니다.
        sample_count: usize,
        /// 첫 표본과 마지막 표본 사이의 관측 기간입니다.
        observation_span: Duration,
        /// 표본을 더 수집해야 하는 구체적인 이유입니다.
        reason: ForecastCollectionReason,
    },
    /// 표본은 충분하지만 관측된 사용량 상승이 너무 작습니다.
    InsufficientActivity,
    /// 표시 가능한 소진 예측입니다.
    ForecastAvailable(Forecast),
    /// 현재 사용량이 이미 100% 이상입니다.
    AlreadyExhausted,
    /// 호출자가 표시한 상태 또는 마지막 성공 조회가 오래되었습니다.
    Stale,
    /// 현재 창이나 표본 스트림의 안전성 검증에 실패했습니다.
    Invalid,
}

/// 동일한 정제 표본에서 계산한 소진 예측과 소비 속도 평가입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct ForecastAnalysis {
    /// 기존 정책을 그대로 적용한 소진 예측 결과입니다.
    pub forecast: ForecastResult,
    /// 다음 초기화 전 소진 위험을 기준으로 한 소비 속도 평가입니다.
    pub pace: ConsumptionPaceAssessment,
}

/// 저장된 표본만으로 사용량 소진 시각을 계산하는 순수 엔진입니다.
#[derive(Clone, Copy, Debug, Default)]
pub struct ForecastEngine;

impl ForecastEngine {
    /// 현재 창과 같은 프로필·창·초기화 구간의 표본에서 소진 예측을 계산합니다.
    ///
    /// `now`와 `last_success_at`은 호출자가 주입하며 이 메서드는 시스템 시각이나 I/O를 사용하지
    /// 않습니다. 혼합된 프로필 또는 창, 유효하지 않은 현재 창은 `Invalid`로 거부하고, 개별
    /// 손상 표본은 안전하게 제외합니다.
    pub fn calculate(
        samples: &[UsageSample],
        current_window: &UsageWindow,
        last_success_at: SystemTime,
        now: SystemTime,
        stale: bool,
        policy: &ForecastPolicy,
    ) -> ForecastResult {
        Self::analyze(samples, current_window, last_success_at, now, stale, policy).forecast
    }

    /// 현재 창과 같은 프로필·창·초기화 구간의 표본에서 예측과 소비 속도를 함께 계산합니다.
    ///
    /// 표본 정제와 기울기 계산은 한 번만 수행합니다. 기존 소진 예측은 v1 최소 상승량과 속도
    /// 기준을 유지하지만, 소비 속도 평가는 표본 3개와 30분 관측 후 낮은 활동도 `Comfortable`로
    /// 분류합니다. 유효하지 않거나 오래된 입력은 두 결과 모두 안전한 비가용 상태로 반환합니다.
    pub fn analyze(
        samples: &[UsageSample],
        current_window: &UsageWindow,
        last_success_at: SystemTime,
        now: SystemTime,
        stale: bool,
        policy: &ForecastPolicy,
    ) -> ForecastAnalysis {
        if !valid_current_window(current_window) || after_epoch(now).is_none() {
            return ForecastAnalysis {
                forecast: ForecastResult::Invalid,
                pace: ConsumptionPaceAssessment::Unavailable,
            };
        }
        if stale || is_stale(last_success_at, now, *policy) {
            return ForecastAnalysis {
                forecast: ForecastResult::Stale,
                pace: ConsumptionPaceAssessment::Unavailable,
            };
        }
        if current_window.used_percent >= 100.0 {
            return ForecastAnalysis {
                forecast: ForecastResult::AlreadyExhausted,
                pace: ConsumptionPaceAssessment::Exhausted,
            };
        }
        if !single_stream(samples, current_window) {
            return ForecastAnalysis {
                forecast: ForecastResult::Invalid,
                pace: ConsumptionPaceAssessment::Unavailable,
            };
        }

        let mut selected: Vec<_> = samples
            .iter()
            .filter(|sample| valid_sample(sample, now))
            .filter(|sample| sample.resets_at() == current_window.resets_at)
            .map(|sample| Point {
                observed_at: sample.observed_at(),
                used_percent: sample.used_percent(),
            })
            .collect();
        // 동일 시각의 손상·중복 표본은 사용량이 큰 값을 남겨 입력 순서에 의존하지 않게 합니다.
        selected.sort_by(|left, right| {
            left.observed_at.cmp(&right.observed_at).then_with(|| {
                right
                    .used_percent
                    .partial_cmp(&left.used_percent)
                    .unwrap_or(Ordering::Equal)
            })
        });
        selected.dedup_by_key(|point| point.observed_at);
        if selected.len() > ForecastPolicy::MAX_SAMPLES {
            selected.drain(..selected.len() - ForecastPolicy::MAX_SAMPLES);
        }

        let segment = current_segment(selected);
        let sample_count = segment.len();
        let observation_span = span(&segment);
        let rise = segment
            .last()
            .map(|last| last.used_percent - segment[0].used_percent)
            .unwrap_or_default();
        let hourly_rate = if sample_count >= ForecastPolicy::MINIMUM_SAMPLES
            && observation_span >= ForecastPolicy::MINIMUM_OBSERVATION_SPAN
        {
            theil_sen_hourly_rate(&segment)
        } else {
            None
        };
        let pace = consumption_pace(
            current_window.resets_at,
            now,
            current_window.used_percent,
            sample_count,
            observation_span,
            rise,
            hourly_rate,
        );
        let forecast = forecast_result(
            current_window,
            now,
            sample_count,
            observation_span,
            rise,
            hourly_rate,
        );

        ForecastAnalysis { forecast, pace }
    }
}

fn consumption_pace(
    reset: Option<SystemTime>,
    now: SystemTime,
    current_used_percent: f64,
    sample_count: usize,
    observation_span: Duration,
    rise: f64,
    hourly_rate: Option<f64>,
) -> ConsumptionPaceAssessment {
    let Some(reset) = reset.filter(|reset| *reset > now) else {
        return ConsumptionPaceAssessment::Unavailable;
    };
    if sample_count < ForecastPolicy::MINIMUM_SAMPLES
        || observation_span < ForecastPolicy::MINIMUM_OBSERVATION_SPAN
    {
        return ConsumptionPaceAssessment::Measuring {
            sample_count,
            observation_span,
        };
    }
    let Some(hourly_rate) = hourly_rate.filter(|rate| rate.is_finite()) else {
        return ConsumptionPaceAssessment::Unavailable;
    };
    if !rise.is_finite() {
        return ConsumptionPaceAssessment::Unavailable;
    }

    let hourly_rate = hourly_rate.max(0.0);
    let hours_until_reset = reset.duration_since(now).unwrap_or_default().as_secs_f64() / 3_600.0;
    let remaining = (100.0 - current_used_percent).max(0.0);
    let safe_hourly_rate = remaining / hours_until_reset;
    if !safe_hourly_rate.is_finite() || safe_hourly_rate <= 0.0 {
        return ConsumptionPaceAssessment::Exhausted;
    }
    let ratio = hourly_rate / safe_hourly_rate;
    let level = if ratio >= 1.0 {
        ConsumptionPaceLevel::Fast
    } else if ratio >= 0.5 {
        ConsumptionPaceLevel::Normal
    } else {
        ConsumptionPaceLevel::Comfortable
    };
    let expected_remaining_percent_at_reset =
        (100.0 - (current_used_percent + hourly_rate * hours_until_reset)).clamp(0.0, 100.0);

    ConsumptionPaceAssessment::Ready(ConsumptionPaceMetrics {
        level,
        sample_count,
        observation_span,
        total_rise: rise.max(0.0),
        hourly_rate,
        safe_hourly_rate,
        expected_remaining_percent_at_reset,
    })
}

fn forecast_result(
    current_window: &UsageWindow,
    now: SystemTime,
    sample_count: usize,
    observation_span: Duration,
    rise: f64,
    hourly_rate: Option<f64>,
) -> ForecastResult {
    if sample_count < ForecastPolicy::MINIMUM_SAMPLES {
        return ForecastResult::Collecting {
            sample_count,
            observation_span,
            reason: ForecastCollectionReason::TooFewSamples,
        };
    }
    if observation_span < ForecastPolicy::MINIMUM_OBSERVATION_SPAN {
        return ForecastResult::Collecting {
            sample_count,
            observation_span,
            reason: ForecastCollectionReason::ObservationSpanTooShort,
        };
    }
    if !rise.is_finite() || rise < ForecastPolicy::MINIMUM_TOTAL_RISE {
        return ForecastResult::InsufficientActivity;
    }
    let Some(hourly_rate) = hourly_rate else {
        return ForecastResult::InsufficientActivity;
    };
    if !hourly_rate.is_finite() || hourly_rate < ForecastPolicy::MINIMUM_HOURLY_RATE {
        return ForecastResult::InsufficientActivity;
    }

    let seconds_to_exhaustion = (100.0 - current_window.used_percent) * 3_600.0 / hourly_rate;
    let Some(until_exhaustion) = duration_from_seconds(seconds_to_exhaustion) else {
        return ForecastResult::Invalid;
    };
    let Some(exhaustion_at) = now.checked_add(until_exhaustion) else {
        return ForecastResult::Invalid;
    };
    let (
        exhausts_before_reset,
        expected_used_percent_at_reset,
        expected_remaining_percent_at_reset,
    ) = reset_estimate(
        current_window.resets_at,
        now,
        current_window.used_percent,
        hourly_rate,
        exhaustion_at,
    );

    ForecastResult::ForecastAvailable(Forecast {
        hourly_rate,
        exhaustion_at,
        exhausts_before_reset,
        expected_used_percent_at_reset,
        expected_remaining_percent_at_reset,
        sample_count,
        observation_span,
        quality: quality(sample_count, observation_span, rise),
    })
}

#[derive(Clone, Copy)]
struct Point {
    observed_at: SystemTime,
    used_percent: f64,
}

fn valid_current_window(current_window: &UsageWindow) -> bool {
    current_window.used_percent.is_finite()
        && current_window.used_percent >= 0.0
        && !current_window.used_percent.is_sign_negative()
        && current_window
            .resets_at
            .is_none_or(|reset| after_epoch(reset).is_some())
}

fn single_stream(samples: &[UsageSample], current_window: &UsageWindow) -> bool {
    let Some(first) = samples.first() else {
        return true;
    };
    samples.iter().all(|sample| {
        sample.profile_id() == first.profile_id() && sample.window_kind() == current_window.kind
    })
}

fn valid_sample(sample: &UsageSample, now: SystemTime) -> bool {
    sample.used_percent().is_finite()
        && sample.used_percent() >= 0.0
        && !sample.used_percent().is_sign_negative()
        && after_epoch(sample.observed_at()).is_some()
        && sample.observed_at() <= now
        && sample
            .resets_at()
            .is_none_or(|reset| after_epoch(reset).is_some() && reset >= sample.observed_at())
}

fn after_epoch(time: SystemTime) -> Option<Duration> {
    time.duration_since(UNIX_EPOCH).ok()
}

fn is_stale(last_success_at: SystemTime, now: SystemTime, policy: ForecastPolicy) -> bool {
    now.duration_since(last_success_at)
        .map_or(true, |age| age > policy.stale_after())
}

fn current_segment(points: Vec<Point>) -> Vec<Point> {
    let mut segment = Vec::with_capacity(points.len());
    let mut running_max = None::<f64>;
    for mut point in points {
        match running_max {
            None => {
                running_max = Some(point.used_percent);
                segment.push(point);
            }
            Some(maximum) if point.used_percent >= maximum => {
                running_max = Some(point.used_percent);
                segment.push(point);
            }
            Some(maximum) if maximum - point.used_percent <= ForecastPolicy::NOISE_DECREASE => {
                point.used_percent = maximum;
                segment.push(point);
            }
            Some(_) => {
                segment.clear();
                running_max = Some(point.used_percent);
                segment.push(point);
            }
        }
    }
    segment
}

fn span(points: &[Point]) -> Duration {
    match (points.first(), points.last()) {
        (Some(first), Some(last)) => last
            .observed_at
            .duration_since(first.observed_at)
            .unwrap_or_default(),
        _ => Duration::ZERO,
    }
}

fn theil_sen_hourly_rate(points: &[Point]) -> Option<f64> {
    let mut slopes =
        Vec::with_capacity(points.len().saturating_mul(points.len().saturating_sub(1)) / 2);
    for (index, earlier) in points.iter().enumerate() {
        for later in &points[index + 1..] {
            let elapsed = later.observed_at.duration_since(earlier.observed_at).ok()?;
            if elapsed.is_zero() {
                continue;
            }
            let slope =
                (later.used_percent - earlier.used_percent) / elapsed.as_secs_f64() * 3_600.0;
            if slope.is_finite() {
                slopes.push(slope);
            }
        }
    }
    if slopes.is_empty() {
        return None;
    }
    slopes.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let middle = slopes.len() / 2;
    Some(if slopes.len() % 2 == 0 {
        (slopes[middle - 1] + slopes[middle]) / 2.0
    } else {
        slopes[middle]
    })
}

fn duration_from_seconds(seconds: f64) -> Option<Duration> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Duration::try_from_secs_f64(seconds).ok()
}

fn reset_estimate(
    reset: Option<SystemTime>,
    now: SystemTime,
    current_used_percent: f64,
    hourly_rate: f64,
    exhaustion_at: SystemTime,
) -> (Option<bool>, Option<f64>, Option<f64>) {
    let Some(reset) = reset.filter(|reset| *reset > now) else {
        return (None, None, None);
    };
    let elapsed_hours = reset.duration_since(now).unwrap_or_default().as_secs_f64() / 3_600.0;
    let projected = (100.0_f64).min((current_used_percent + hourly_rate * elapsed_hours).max(0.0));
    let expected_used = if projected.is_finite() {
        projected
    } else {
        100.0
    };
    (
        Some(exhaustion_at <= reset),
        Some(expected_used),
        Some((100.0 - expected_used).max(0.0)),
    )
}

fn quality(sample_count: usize, observation_span: Duration, rise: f64) -> ForecastQuality {
    if sample_count >= 8 && observation_span >= Duration::from_secs(2 * 60 * 60) && rise >= 5.0 {
        ForecastQuality::High
    } else if sample_count >= 5 && observation_span >= Duration::from_secs(60 * 60) && rise >= 2.0 {
        ForecastQuality::Medium
    } else {
        ForecastQuality::Low
    }
}
