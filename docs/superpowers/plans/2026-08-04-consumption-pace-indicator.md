# Consumption Pace Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 현재 소비 속도가 다음 초기화 전에 한도를 소진할 위험을 `여유/보통/빠름`으로 분류해 위젯 상태점과 호버 툴팁에 표시한다.

**Architecture:** 기존 예측 엔진이 한 번 정제한 표본으로 소진 예측과 소비 속도 평가를 함께 계산하고, 비동기 서비스가 둘을 같은 캐시에 보관한다. 애플리케이션 계층은 타입 상태를 지역화된 두 줄 설명으로 변환하고, Win32 렌더러는 점 색과 진행 막대 색을 분리해 그린다.

**Tech Stack:** Rust 2021, Rust 1.85+, 표준 라이브러리, 기존 `serde`와 `windows` crate, Win32 GDI, Cargo test/clippy/fmt

## Global Constraints

- 새 dependency를 추가하지 않는다.
- 사용량은 Codex app-server가 제공한 `usedPercent`만 사용하며 토큰 수나 원본 RPC payload를 수집하지 않는다.
- 표본 정제와 Theil-Sen 기울기 계산은 `src/forecast.rs` 한 곳에서만 수행한다.
- 기존 소진 예측의 3개 표본, 30분, 총 1퍼센트 포인트, 시간당 0.05퍼센트 포인트 정책을 변경하지 않는다.
- 소비 속도는 3개 표본과 30분 이후부터 평가하되 낮은 활동을 숨기지 않고 `여유`로 분류한다.
- 점은 소비 속도, 진행 막대는 현재 사용량 위험도를 나타낸다. 로딩 회색 점과 오류 빨간 `!`가 우선한다.
- 설정 및 `usage-history.json` schema를 변경하지 않는다.
- 새 public API, 상태 전이와 UI 모델에는 입력·반환·제약을 설명하는 한국어 rustdoc을 작성한다.
- 모든 사용자 문구는 `src/localization.rs`의 12개 언어에 추가하고 문자열 비교로 제어 흐름을 만들지 않는다.
- 자동 검증 완료와 별개로 Windows/DPI/테마/Explorer 복구 수동 검증 미실행 항목을 최종 결과에 남긴다.

---

### Task 1: 예측 엔진에 소비 속도 분석 추가

**Files:**
- Modify: `src/forecast.rs`
- Modify: `src/lib.rs`
- Test: `tests/forecast_runtime.rs`

**Interfaces:**
- Consumes: 기존 `UsageSample`, `UsageWindow`, `ForecastPolicy`, `ForecastResult`, `current_segment`, `theil_sen_hourly_rate`
- Produces: `ConsumptionPaceLevel`, `ConsumptionPaceMetrics`, `ConsumptionPaceAssessment`, `ForecastAnalysis`, `ForecastEngine::analyze`
- Preserves: `ForecastEngine::calculate(...) -> ForecastResult`

- [ ] **Step 1: 저활동·경계·판단 불가 테스트를 먼저 작성한다**

`tests/forecast_runtime.rs`에 다음 형태의 테스트와 helper를 추가한다.

```rust
fn evenly_spaced_samples_for_reset(
    count: usize,
    observation_span: Duration,
    start_percent: f64,
    rise: f64,
    reset: SystemTime,
    now: SystemTime,
) -> Vec<UsageSample> {
    let intervals = u64::try_from(count - 1).unwrap();
    (0..count)
        .map(|index| {
            let index = u64::try_from(index).unwrap();
            sample(
                UsageProfileId::System,
                WindowKind::Primary,
                start_percent + rise * index as f64 / intervals as f64,
                Some(reset),
                now - Duration::from_secs(
                    observation_span.as_secs() * (intervals - index) / intervals,
                ),
            )
        })
        .collect()
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

fn pace_for_exact_rate(
    current_used_percent: f64,
    hourly_rate: f64,
    reset: SystemTime,
    now: SystemTime,
) -> ConsumptionPaceMetrics {
    let span = Duration::from_secs(30 * 60);
    let rise = hourly_rate * span.as_secs_f64() / 3_600.0;
    let samples = evenly_spaced_samples_for_reset(
        3,
        span,
        current_used_percent - rise,
        rise,
        reset,
        now,
    );
    let ConsumptionPaceAssessment::Ready(metrics) =
        analyze(&samples, &window(current_used_percent, Some(reset)), now).pace
    else {
        panic!("expected ready pace");
    };
    metrics
}

#[test]
fn low_activity_is_comfortable_even_when_forecast_is_suppressed() {
    let now = at(3_300_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);
    let samples = evenly_spaced_samples_for_reset(
        3,
        Duration::from_secs(2 * 60 * 60),
        10.0,
        0.9,
        reset,
        now,
    );

    let analysis = analyze(&samples, &window(10.9, Some(reset)), now);

    assert_eq!(analysis.forecast, ForecastResult::InsufficientActivity);
    let ConsumptionPaceAssessment::Ready(metrics) = analysis.pace else {
        panic!("expected ready pace");
    };
    assert_eq!(metrics.level, ConsumptionPaceLevel::Comfortable);
    assert!((metrics.total_rise - 0.9).abs() < 0.000_001);
    assert!(metrics.hourly_rate >= 0.0);
}

#[test]
fn pace_ratio_boundaries_are_normal_and_fast() {
    let now = at(3_400_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);

    let normal = pace_for_exact_rate(50.0, 2.5, reset, now);
    let fast = pace_for_exact_rate(50.0, 5.0, reset, now);

    assert_eq!(normal.level, ConsumptionPaceLevel::Normal);
    assert_eq!(fast.level, ConsumptionPaceLevel::Fast);
}

#[test]
fn pace_requires_three_samples_thirty_minutes_and_future_reset() {
    let now = at(3_500_000);
    let reset = now + Duration::from_secs(10 * 60 * 60);
    let two = evenly_spaced_samples_for_reset(
        2,
        Duration::from_secs(30 * 60),
        10.0,
        1.0,
        reset,
        now,
    );
    let short = evenly_spaced_samples_for_reset(
        3,
        Duration::from_secs(30 * 60 - 1),
        10.0,
        1.0,
        reset,
        now,
    );

    assert!(matches!(
        analyze(&two, &window(11.0, Some(reset)), now).pace,
        ConsumptionPaceAssessment::Measuring { sample_count: 2, .. }
    ));
    assert!(matches!(
        analyze(&short, &window(11.0, Some(reset)), now).pace,
        ConsumptionPaceAssessment::Measuring { .. }
    ));
    assert_eq!(
        analyze(&[], &window(10.0, None), now).pace,
        ConsumptionPaceAssessment::Unavailable
    );
    assert_eq!(
        analyze(
            &[],
            &window(10.0, Some(now - Duration::from_secs(1))),
            now,
        )
        .pace,
        ConsumptionPaceAssessment::Unavailable
    );
}
```

- [ ] **Step 2: 새 테스트가 컴파일 또는 assertion 단계에서 실패하는지 확인한다**

Run:

```powershell
cargo test --test forecast_runtime pace_ -- --nocapture
cargo test --test forecast_runtime low_activity_is_comfortable_even_when_forecast_is_suppressed -- --nocapture
```

Expected: `ForecastAnalysis`/`ConsumptionPaceAssessment` 미정의 또는 기존 `InsufficientActivity`가 속도 평가를 제공하지 않아 FAIL.

- [ ] **Step 3: 타입 지정 분석 결과를 구현한다**

`src/forecast.rs`에 다음 공개 타입을 한국어 rustdoc과 함께 추가한다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumptionPaceLevel {
    Comfortable,
    Normal,
    Fast,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsumptionPaceMetrics {
    pub level: ConsumptionPaceLevel,
    pub sample_count: usize,
    pub observation_span: Duration,
    pub total_rise: f64,
    pub hourly_rate: f64,
    pub safe_hourly_rate: f64,
    pub expected_remaining_percent_at_reset: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConsumptionPaceAssessment {
    Measuring {
        sample_count: usize,
        observation_span: Duration,
    },
    Ready(ConsumptionPaceMetrics),
    Exhausted,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForecastAnalysis {
    pub forecast: ForecastResult,
    pub pace: ConsumptionPaceAssessment,
}
```

`ForecastEngine::analyze`가 기존 validation/정렬/중복 제거/최근 32개/current segment를 한 번 수행한 뒤 다음 순서로 결과를 만든다.

```rust
pub fn analyze(
    samples: &[UsageSample],
    current_window: &UsageWindow,
    last_success_at: SystemTime,
    now: SystemTime,
    stale: bool,
    policy: &ForecastPolicy,
) -> ForecastAnalysis
```

판정 규칙:

```rust
let pace = if current_window.used_percent >= 100.0 {
    ConsumptionPaceAssessment::Exhausted
} else if current_window.resets_at.is_none_or(|reset| reset <= now) {
    ConsumptionPaceAssessment::Unavailable
} else if sample_count < ForecastPolicy::MINIMUM_SAMPLES
    || observation_span < ForecastPolicy::MINIMUM_OBSERVATION_SPAN
{
    ConsumptionPaceAssessment::Measuring {
        sample_count,
        observation_span,
    }
} else {
    let hourly_rate = theil_sen_hourly_rate(&segment).unwrap_or(0.0).max(0.0);
    let hours_until_reset = reset.duration_since(now).unwrap().as_secs_f64() / 3_600.0;
    let remaining = (100.0 - current_window.used_percent).max(0.0);
    let safe_hourly_rate = remaining / hours_until_reset;
    let ratio = hourly_rate / safe_hourly_rate;
    let level = if ratio >= 1.0 {
        ConsumptionPaceLevel::Fast
    } else if ratio >= 0.5 {
        ConsumptionPaceLevel::Normal
    } else {
        ConsumptionPaceLevel::Comfortable
    };
    let expected_remaining_percent_at_reset =
        (100.0 - (current_window.used_percent + hourly_rate * hours_until_reset))
            .clamp(0.0, 100.0);
    ConsumptionPaceAssessment::Ready(ConsumptionPaceMetrics {
        level,
        sample_count,
        observation_span,
        total_rise: rise,
        hourly_rate,
        safe_hourly_rate,
        expected_remaining_percent_at_reset,
    })
};
```

`expected_remaining_percent_at_reset`은 `(100.0 - (current_used + hourly_rate * hours_until_reset)).clamp(0.0, 100.0)`으로 계산한다. validation/stale/mixed stream 실패는 기존 `Invalid`/`Stale` forecast와 `Unavailable` pace를 함께 반환한다. `ForecastEngine::calculate`는 `Self::analyze(...).forecast` wrapper로 바꿔 공개 호환성을 유지한다.

- [ ] **Step 4: 타입을 crate root에서 노출한다**

`src/lib.rs`의 forecast export에 다음을 추가한다.

```rust
ConsumptionPaceAssessment, ConsumptionPaceLevel, ConsumptionPaceMetrics, ForecastAnalysis,
```

- [ ] **Step 5: 계산 테스트와 기존 예측 테스트를 통과시킨다**

Run:

```powershell
cargo test --test forecast_runtime
cargo test --lib forecast
```

Expected: 새 속도 경계 테스트와 기존 소진 예측 회귀 테스트 모두 PASS.

- [ ] **Step 6: 계산 엔진 변경을 커밋한다**

```powershell
git add src/forecast.rs src/lib.rs tests/forecast_runtime.rs
git commit -m "feat: Add consumption pace analysis"
```

---

### Task 2: 예측 서비스에서 속도 평가를 함께 캐시

**Files:**
- Modify: `src/usage_forecast.rs`
- Test: `tests/usage_forecast_runtime.rs`

**Interfaces:**
- Consumes: Task 1의 `ForecastAnalysis`, `ConsumptionPaceAssessment`, `ForecastEngine::analyze`
- Produces: `UsageForecastService::pace_at(profile_id, window_kind, now) -> Option<ConsumptionPaceAssessment>`
- Preserves: `forecast_at(...) -> Option<ForecastResult>`와 기존 lifecycle/cache invalidation 계약

- [ ] **Step 1: 저활동 캐시와 lifecycle 회귀 테스트를 작성한다**

`tests/usage_forecast_runtime.rs`에 다음 테스트를 추가한다.

```rust
fn wait_for_pace(
    service: &UsageForecastService,
    id: UsageProfileId,
    kind: WindowKind,
    now: SystemTime,
) -> ConsumptionPaceAssessment {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(pace) = service.pace_at(id, kind, now) {
            return pace;
        }
        assert!(Instant::now() < deadline, "pace was not cached in time");
        thread::yield_now();
    }
}

#[test]
fn low_activity_pace_is_cached_even_without_an_exhaustion_forecast() {
    let root = TestRoot::new("low-activity-pace");
    let service = UsageForecastService::start(
        UsageHistoryStore::for_root(&root.0),
        [UsageProfileId::System],
        ForecastPolicy::default(),
    );
    let now = SystemTime::now();
    let reset = now + Duration::from_secs(10 * 60 * 60);
    for (minutes_ago, percent) in [(120, 10.0), (60, 10.5), (0, 10.9)] {
        let observed_at = now - Duration::from_secs(minutes_ago * 60);
        service.record_success(
            UsageProfileId::System,
            &usage(WindowKind::Secondary, percent, reset, observed_at),
            observed_at,
        );
    }

    assert!(matches!(
        wait_for_forecast(&service, UsageProfileId::System, WindowKind::Secondary, now),
        ForecastResult::InsufficientActivity
    ));
    assert!(matches!(
        wait_for_pace(&service, UsageProfileId::System, WindowKind::Secondary, now),
        ConsumptionPaceAssessment::Ready(metrics)
            if metrics.level == ConsumptionPaceLevel::Comfortable
    ));
    service.stop();
}
```

기존 `disabled_cleared_and_removed_profiles_do_not_expose_cached_forecasts`를 확장해 기능 끄기, clear, remove 이후 `pace_at`도 `None`임을 assertion 한다. stale 시각 조회는 `ConsumptionPaceAssessment::Unavailable`을 반환하는 테스트를 추가한다.

- [ ] **Step 2: 새 서비스 테스트의 실패를 확인한다**

Run:

```powershell
cargo test --test usage_forecast_runtime pace_ -- --nocapture
```

Expected: `pace_at` 미정의로 FAIL.

- [ ] **Step 3: 캐시를 분석 결과 단위로 변경한다**

`CachedForecast`를 다음처럼 바꾸고 이름을 `CachedAnalysis`로 변경한다.

```rust
#[derive(Clone)]
struct CachedAnalysis {
    observed_at: SystemTime,
    analysis: ForecastAnalysis,
}
```

`cache_usage`는 `ForecastEngine::calculate` 대신 `ForecastEngine::analyze`를 한 번 호출한다. `forecast_at`은 `analysis.forecast.clone()`을 반환하고, 새 `pace_at`은 `analysis.pace.clone()`을 반환한다. 캐시가 `stale_after()`를 넘으면 `forecast_at`은 기존처럼 `Stale`, `pace_at`은 `Unavailable`을 반환한다. disabled/inactive/missing stream은 둘 다 `None`이다.

- [ ] **Step 4: 서비스 단위와 runtime 테스트를 통과시킨다**

Run:

```powershell
cargo test --test usage_forecast_runtime
cargo test --lib usage_forecast
```

Expected: 프로필/창 격리, 저장 실패, clear/remove/disable, stale 테스트 모두 PASS.

- [ ] **Step 5: 서비스 변경을 커밋한다**

```powershell
git add src/usage_forecast.rs tests/usage_forecast_runtime.rs
git commit -m "feat: Cache consumption pace assessments"
```

---

### Task 3: 속도 상태를 지역화된 호버 표시 모델로 변환

**Files:**
- Modify: `src/localization.rs`
- Modify: `src/windows/mod.rs`
- Modify: `src/app.rs`
- Test: `tests/localization_runtime.rs`
- Test: `src/app.rs` inline tests
- Test fixtures: `tests/windows_app.rs`

**Interfaces:**
- Consumes: Task 2의 `UsageForecastService::pace_at`, Task 1의 pace 타입
- Produces: `ConsumptionPaceState`, `ConsumptionPaceView`, `WidgetViewModel.consumption_pace`
- Preserves: 기존 `ForecastView`와 기본·보조 창별 소진 예측 순서

- [ ] **Step 1: UI 표시 모델과 문구 테스트를 먼저 작성한다**

`src/app.rs` inline tests에 다음 behavior를 추가한다.

```rust
#[test]
fn comfortable_pace_copy_explains_recent_activity_without_statistical_terms() {
    let view = consumption_pace_view(
        Some(ConsumptionPaceAssessment::Ready(ConsumptionPaceMetrics {
            level: ConsumptionPaceLevel::Comfortable,
            sample_count: 8,
            observation_span: Duration::from_secs(2 * 60 * 60),
            total_rise: 3.0,
            hourly_rate: 1.5,
            safe_hourly_rate: 4.0,
            expected_remaining_percent_at_reset: 42.0,
        })),
        true,
        Language::Korean,
    );

    assert_eq!(view.state, ConsumptionPaceState::Comfortable);
    assert_eq!(view.summary, "소비 속도: 여유");
    assert_eq!(
        view.detail.as_deref(),
        Some(
            "최근 2시간 동안 3% 사용 · 시간당 약 1.5%\n\
             현재 속도면 초기화 시 약 42% 남아요"
        )
    );
}

#[test]
fn pace_copy_covers_measuring_unavailable_disabled_and_exhausted() {
    let measuring = consumption_pace_view(
        Some(ConsumptionPaceAssessment::Measuring {
            sample_count: 2,
            observation_span: Duration::from_secs(20 * 60),
        }),
        true,
        Language::Korean,
    );
    assert_eq!(measuring.state, ConsumptionPaceState::Measuring);
    assert_eq!(
        measuring.summary,
        "소비 속도 측정 중 · 데이터 2/3 · 관측 20/30분"
    );

    let unavailable = consumption_pace_view(
        Some(ConsumptionPaceAssessment::Unavailable),
        true,
        Language::Korean,
    );
    assert_eq!(unavailable.state, ConsumptionPaceState::Unavailable);
    assert!(unavailable.summary.contains("초기화 시각"));

    let disabled = consumption_pace_view(None, false, Language::Korean);
    assert_eq!(disabled.state, ConsumptionPaceState::Disabled);
    assert_eq!(disabled.summary, "소비 속도 표시 꺼짐");

    let exhausted = consumption_pace_view(
        Some(ConsumptionPaceAssessment::Exhausted),
        true,
        Language::Korean,
    );
    assert_eq!(exhausted.state, ConsumptionPaceState::Exhausted);
    assert_eq!(exhausted.summary, "한도 소진됨");
}

#[test]
fn pace_tooltip_precedes_per_window_forecasts() {
    let pace = ConsumptionPaceView {
        state: ConsumptionPaceState::Comfortable,
        summary: "Usage pace: Comfortable".to_owned(),
        detail: Some(
            "Used 3% over the last 2 hours · about 1.5% per hour\n\
             At this pace, about 42% will remain at reset"
                .to_owned(),
        ),
    };
    let primary = UsageRowView {
        label: "5h".to_owned(),
        used_percent: 20.0,
        display_percent: 20.0,
        percent_text: "20%".to_owned(),
        reset_text: "tomorrow".to_owned(),
        level: UsageLevel::Stable,
        forecast: ForecastView::ForecastAvailable {
            line: "primary estimate".to_owned(),
        },
    };
    let tooltip = append_consumption_pace_tooltip("Codex usage\nStatus: Polling", &pace);
    let tooltip = append_forecast_tooltip(&tooltip, Some(&primary), None, Language::English);

    assert_eq!(
        tooltip,
        "Codex usage\nStatus: Polling\n\nUsage pace: Comfortable\n\
         Used 3% over the last 2 hours · about 1.5% per hour\n\
         At this pace, about 42% will remain at reset\n\n\
         Primary window: primary estimate"
    );
}
```

`tests/localization_runtime.rs`의 전체 key 목록과 대표 한국어/영어 assertion에 새 key를 추가한다. `tests/windows_app.rs`의 모든 `WidgetViewModel` fixture에는 명시적 `consumption_pace`를 넣어 public contract 변경을 컴파일로 검증한다.

- [ ] **Step 2: UI 모델 테스트가 새 타입/키 부재로 실패하는지 확인한다**

Run:

```powershell
cargo test --lib app::tests::comfortable_pace_copy_explains_recent_activity_without_statistical_terms
cargo test --test localization_runtime
```

Expected: `ConsumptionPaceView`/새 localization key 미정의로 FAIL.

- [ ] **Step 3: 타입 지정 UI 상태를 추가한다**

`src/windows/mod.rs`에 한국어 rustdoc과 함께 다음 타입을 추가한다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumptionPaceState {
    Comfortable,
    Normal,
    Fast,
    Measuring,
    Unavailable,
    Disabled,
    Exhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsumptionPaceView {
    pub state: ConsumptionPaceState,
    pub summary: String,
    pub detail: Option<String>,
}
```

`WidgetViewModel`에 `pub consumption_pace: ConsumptionPaceView`를 추가한다. 모든 production/test 생성자를 컴파일 오류 목록에 따라 갱신하되 문자열로 상태를 판단하지 않는다.

- [ ] **Step 4: 지역화 key와 12개 언어 문구를 추가한다**

`LocalizationKey` 끝에 다음 key를 추가하고 `LOCALIZATION_KEY_COUNT`를 `102`로, `ALL`/`index`를 같은 순서로 갱신한다.

```rust
UsagePaceComfortable,
UsagePaceNormal,
UsagePaceFast,
UsagePaceMeasuring,
UsagePaceUnavailable,
UsagePaceDisabled,
UsagePaceRecentActivity,
UsagePaceExpectedRemaining,
UsagePaceBeforeReset,
```

한국어/영어 기준 문구는 정확히 다음과 같다.

| Key | Korean | English |
| --- | --- | --- |
| `UsagePaceComfortable` | `소비 속도: 여유` | `Usage pace: Comfortable` |
| `UsagePaceNormal` | `소비 속도: 보통` | `Usage pace: Moderate` |
| `UsagePaceFast` | `소비 속도: 빠름` | `Usage pace: Fast` |
| `UsagePaceMeasuring` | `소비 속도 측정 중 · 데이터 {count}/{required} · 관측 {minutes}/{required_minutes}분` | `Measuring usage pace · data {count}/{required} · observed {minutes}/{required_minutes} min` |
| `UsagePaceUnavailable` | `소비 속도를 판단할 수 없음 · 최신 사용량과 초기화 시각이 필요함` | `Usage pace unavailable · recent usage and a reset time are required` |
| `UsagePaceDisabled` | `소비 속도 표시 꺼짐` | `Usage pace display is off` |
| `UsagePaceRecentActivity` | `최근 {duration} 동안 {rise}% 사용 · 시간당 약 {rate}%` | `Used {rise}% over the last {duration} · about {rate}% per hour` |
| `UsagePaceExpectedRemaining` | `현재 속도면 초기화 시 약 {percent}% 남아요` | `At this pace, about {percent}% will remain at reset` |
| `UsagePaceBeforeReset` | `현재 속도면 초기화 전에 소진될 수 있어요` | `At this pace, the limit may be exhausted before reset` |

나머지 10개 언어에는 아래 문구를 같은 key 순서로 넣는다. `Expected/Before reset` 열의 첫 문구는
`{percent}` 치환 토큰을 포함한다.

| Language | Comfortable / Normal / Fast | Measuring | Unavailable | Disabled | Recent activity | Expected / Before reset |
| --- | --- | --- | --- | --- | --- | --- |
| Spanish | `Ritmo de uso: Holgado` / `Ritmo de uso: Moderado` / `Ritmo de uso: Rápido` | `Midiendo el ritmo de uso · datos {count}/{required} · observación {minutes}/{required_minutes} min` | `Ritmo de uso no disponible · se requieren datos recientes y una hora de restablecimiento` | `La visualización del ritmo de uso está desactivada` | `Uso del {rise}% en las últimas {duration} · aprox. {rate}% por hora` | `A este ritmo, quedará cerca del {percent}% al restablecerse` / `A este ritmo, el límite puede agotarse antes del restablecimiento` |
| Portuguese (Brazil) | `Ritmo de uso: Tranquilo` / `Ritmo de uso: Moderado` / `Ritmo de uso: Rápido` | `Medindo o ritmo de uso · dados {count}/{required} · observação {minutes}/{required_minutes} min` | `Ritmo de uso indisponível · são necessários dados recentes e um horário de redefinição` | `A exibição do ritmo de uso está desativada` | `Uso de {rise}% nas últimas {duration} · cerca de {rate}% por hora` | `Neste ritmo, cerca de {percent}% restará na redefinição` / `Neste ritmo, o limite pode se esgotar antes da redefinição` |
| Indonesian | `Laju penggunaan: Longgar` / `Laju penggunaan: Sedang` / `Laju penggunaan: Cepat` | `Mengukur laju penggunaan · data {count}/{required} · diamati {minutes}/{required_minutes} mnt` | `Laju penggunaan tidak tersedia · penggunaan terbaru dan waktu reset diperlukan` | `Tampilan laju penggunaan nonaktif` | `Menggunakan {rise}% selama {duration} terakhir · sekitar {rate}% per jam` | `Dengan laju ini, sekitar {percent}% akan tersisa saat reset` / `Dengan laju ini, batas dapat habis sebelum reset` |
| Japanese | `消費ペース: 余裕` / `消費ペース: 標準` / `消費ペース: 速い` | `消費ペースを測定中 · データ {count}/{required} · 観測 {minutes}/{required_minutes}分` | `消費ペースを判定できません · 最新の使用量とリセット時刻が必要です` | `消費ペース表示はオフ` | `直近{duration}で{rise}%使用 · 1時間あたり約{rate}%` | `このペースではリセット時に約{percent}%残ります` / `このペースではリセット前に上限へ達する可能性があります` |
| Hindi | `उपयोग गति: सहज` / `उपयोग गति: सामान्य` / `उपयोग गति: तेज़` | `उपयोग गति मापी जा रही है · डेटा {count}/{required} · अवलोकन {minutes}/{required_minutes} मिनट` | `उपयोग गति उपलब्ध नहीं · हाल का उपयोग और रीसेट समय आवश्यक है` | `उपयोग गति प्रदर्शन बंद है` | `पिछले {duration} में {rise}% उपयोग · लगभग {rate}% प्रति घंटा` | `इस गति पर रीसेट के समय लगभग {percent}% शेष रहेगा` / `इस गति पर सीमा रीसेट से पहले समाप्त हो सकती है` |
| German | `Nutzungstempo: Entspannt` / `Nutzungstempo: Normal` / `Nutzungstempo: Schnell` | `Nutzungstempo wird gemessen · Daten {count}/{required} · Beobachtung {minutes}/{required_minutes} Min.` | `Nutzungstempo nicht verfügbar · aktuelle Nutzung und Rücksetzzeit erforderlich` | `Anzeige des Nutzungstempos ist aus` | `In den letzten {duration} {rise}% genutzt · etwa {rate}% pro Stunde` | `Bei diesem Tempo bleiben beim Zurücksetzen etwa {percent}% übrig` / `Bei diesem Tempo kann das Limit vor dem Zurücksetzen aufgebraucht sein` |
| French | `Rythme d’utilisation : confortable` / `Rythme d’utilisation : modéré` / `Rythme d’utilisation : rapide` | `Mesure du rythme d’utilisation · données {count}/{required} · observation {minutes}/{required_minutes} min` | `Rythme d’utilisation indisponible · utilisation récente et heure de réinitialisation requises` | `L’affichage du rythme d’utilisation est désactivé` | `{rise}% utilisés au cours des dernières {duration} · environ {rate}% par heure` | `À ce rythme, il restera environ {percent}% à la réinitialisation` / `À ce rythme, la limite peut être épuisée avant la réinitialisation` |
| Vietnamese | `Tốc độ sử dụng: Dư dả` / `Tốc độ sử dụng: Bình thường` / `Tốc độ sử dụng: Nhanh` | `Đang đo tốc độ sử dụng · dữ liệu {count}/{required} · quan sát {minutes}/{required_minutes} phút` | `Không thể xác định tốc độ sử dụng · cần mức dùng gần đây và thời điểm đặt lại` | `Hiển thị tốc độ sử dụng đang tắt` | `Đã dùng {rise}% trong {duration} gần đây · khoảng {rate}% mỗi giờ` | `Với tốc độ này, khoảng {percent}% sẽ còn lại khi đặt lại` / `Với tốc độ này, giới hạn có thể hết trước khi đặt lại` |
| Turkish | `Kullanım hızı: Rahat` / `Kullanım hızı: Normal` / `Kullanım hızı: Hızlı` | `Kullanım hızı ölçülüyor · veri {count}/{required} · gözlem {minutes}/{required_minutes} dk` | `Kullanım hızı kullanılamıyor · güncel kullanım ve sıfırlama zamanı gerekli` | `Kullanım hızı göstergesi kapalı` | `Son {duration} içinde %{rise} kullanıldı · saatte yaklaşık %{rate}` | `Bu hızla sıfırlamada yaklaşık %{percent} kalır` / `Bu hızla sınır sıfırlamadan önce tükenebilir` |
| Arabic | `وتيرة الاستخدام: مريحة` / `وتيرة الاستخدام: معتدلة` / `وتيرة الاستخدام: سريعة` | `جارٍ قياس وتيرة الاستخدام · البيانات {count}/{required} · الرصد {minutes}/{required_minutes} دقيقة` | `وتيرة الاستخدام غير متاحة · يلزم استخدام حديث ووقت لإعادة التعيين` | `عرض وتيرة الاستخدام متوقف` | `تم استخدام {rise}% خلال آخر {duration} · نحو {rate}% في الساعة` | `بهذه الوتيرة، سيتبقى نحو {percent}% عند إعادة التعيين` / `بهذه الوتيرة، قد ينفد الحد قبل إعادة التعيين` |

각 언어 배열의 key 수가 compile-time 길이로 검증되고, runtime test가 `{count}`, `{required}`, `{minutes}`, `{required_minutes}`, `{duration}`, `{rise}`, `{rate}`, `{percent}` 잔존 여부를 검사하게 한다.

- [ ] **Step 5: pace를 사용자 문구로 변환한다**

`src/app.rs`에 다음 helper를 추가한다.

```rust
fn consumption_pace_view(
    assessment: Option<ConsumptionPaceAssessment>,
    enabled: bool,
    language: Language,
) -> ConsumptionPaceView
```

규칙:

- `enabled == false` -> `Disabled`, `UsagePaceDisabled`, detail 없음
- `None` -> `Measuring` 0/3, 0/30분
- `Measuring` -> count를 3, span 분을 30으로 cap해 template 치환
- `Ready` -> level별 summary + `UsagePaceRecentActivity`와 위험 설명을 줄바꿈한 detail
- `Unavailable` -> `UsagePaceUnavailable`, detail 없음
- `Exhausted` -> `UsageForecastExhausted` 재사용, detail 없음

ready detail 숫자는 다음 helper로 최대 소수 한 자리만 표시한다.

```rust
fn compact_decimal(value: f64) -> String {
    let rounded = (value.max(0.0) * 10.0).round() / 10.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}
```

관측 시간은 기존 `forecast_duration_text`를 재사용한다. `AppRuntime::snapshot`은 실제로 표시하는 창과 같은 규칙으로 secondary pace를 우선하고, secondary가 없으면 primary pace를 선택한다.

`Ready` detail은 `UsagePaceRecentActivity` 다음 줄에 Comfortable/Normal이면
`UsagePaceExpectedRemaining`, Fast이면 `UsagePaceBeforeReset`을 붙인다. 따라서 낮은 활동으로 기존
정확한 소진 시각 예측이 `InsufficientActivity`여도 속도 등급의 초기화 기준 결론은 항상 보인다.

- [ ] **Step 6: 호버 툴팁에 속도 block을 한 번 추가한다**

`append_consumption_pace_tooltip`을 추가해 기본 사용량 block 뒤에 `summary`, 선택적 `detail`을 빈 줄로 분리한다. 그 결과를 기존 `append_forecast_tooltip`에 전달해 최종 순서를 보존한다.

```rust
let details = append_consumption_pace_tooltip(&taskbar.tooltip, &consumption_pace);
let details = append_forecast_tooltip(&details, primary.as_ref(), secondary.as_ref(), language);
let taskbar_tooltip = profile_taskbar_tooltip(&usage_profile_label, &details, language);
```

- [ ] **Step 7: UI/지역화 테스트를 통과시킨다**

Run:

```powershell
cargo test --lib app::tests
cargo test --test localization_runtime
cargo test --test windows_app profile_tooltip
```

Expected: 12개 언어, 치환 토큰 처리, hover block 순서와 모든 `WidgetViewModel` fixture PASS.

- [ ] **Step 8: 표시 모델 변경을 커밋한다**

```powershell
git add src/localization.rs src/windows/mod.rs src/app.rs tests/localization_runtime.rs tests/windows_app.rs
git commit -m "feat: Explain consumption pace in widget tooltip"
```

---

### Task 4: 상태점과 진행 막대의 색상 의미 분리

**Files:**
- Modify: `src/windows/taskbar_widget.rs`
- Modify: `src/windows/native/platform.rs`
- Test: `tests/windows_app.rs`
- Test: `src/windows/native/platform.rs` inline tests

**Interfaces:**
- Consumes: Task 3의 `WidgetViewModel.consumption_pace.state`, 기존 `WidgetDataState`, `TaskbarRisk`
- Produces: `TaskbarIndicator`, `TaskbarVisualState`, `taskbar_visual_state`
- Preserves: `TaskbarLayout.dot` 위치, minimal layout의 점 생략, 오류 `!`, 현재 사용량 progress width

- [ ] **Step 1: 순수 시각 상태 테스트를 작성한다**

`tests/windows_app.rs`에 다음 테스트를 추가한다.

```rust
fn taskbar_view(
    data_state: WidgetDataState,
    pace_state: ConsumptionPaceState,
    used_percent: f64,
) -> WidgetViewModel {
    WidgetViewModel {
        usage_profile_label: "Main".to_owned(),
        primary: None,
        secondary: Some(UsageRowView {
            label: "7d".to_owned(),
            used_percent,
            display_percent: used_percent,
            percent_text: format!("{used_percent:.0}%"),
            reset_text: "tomorrow".to_owned(),
            level: UsageLevel::Danger,
            forecast: ForecastView::Hidden,
        }),
        status: "Polling".to_owned(),
        last_success: String::new(),
        is_stale: false,
        taskbar_label: "Weekly usage".to_owned(),
        taskbar_tooltip: String::new(),
        reset_credits_text: None,
        data_state,
        consumption_pace: ConsumptionPaceView {
            state: pace_state,
            summary: String::new(),
            detail: None,
        },
    }
}

#[test]
fn taskbar_dot_uses_pace_while_progress_uses_current_usage_risk() {
    let view = taskbar_view(
        WidgetDataState::Ready,
        ConsumptionPaceState::Comfortable,
        95.0,
    );

    let visual = taskbar_visual_state(&view);

    assert_eq!(visual.indicator, TaskbarIndicator::Comfortable);
    assert_eq!(visual.progress_risk, TaskbarRisk::Critical);
}

#[test]
fn loading_and_error_override_the_pace_dot() {
    let loading = taskbar_view(
        WidgetDataState::Loading,
        ConsumptionPaceState::Fast,
        95.0,
    );
    let error = taskbar_view(
        WidgetDataState::Error,
        ConsumptionPaceState::Comfortable,
        95.0,
    );

    assert_eq!(
        taskbar_visual_state(&loading),
        TaskbarVisualState {
            indicator: TaskbarIndicator::Neutral,
            progress_risk: TaskbarRisk::Critical,
        }
    );
    assert_eq!(
        taskbar_visual_state(&error),
        TaskbarVisualState {
            indicator: TaskbarIndicator::Error,
            progress_risk: TaskbarRisk::Critical,
        }
    );
}
```

native inline test에는 다음 exact `COLORREF` 검증을 추가한다.

```rust
#[test]
fn taskbar_indicator_colors_are_stable() {
    assert_eq!(
        taskbar_indicator_color(TaskbarIndicator::Comfortable),
        COLORREF(0x0074_c748)
    );
    assert_eq!(
        taskbar_indicator_color(TaskbarIndicator::Normal),
        COLORREF(0x0023_a6f5)
    );
    assert_eq!(
        taskbar_indicator_color(TaskbarIndicator::Fast),
        COLORREF(0x005c_5cff)
    );
    assert_eq!(
        taskbar_indicator_color(TaskbarIndicator::Neutral),
        COLORREF(0x0097_9797)
    );
    assert_eq!(
        taskbar_indicator_color(TaskbarIndicator::Error),
        COLORREF(0x005c_5cff)
    );
}
```

- [ ] **Step 2: 새 시각 상태 테스트가 미정의 타입으로 실패하는지 확인한다**

Run:

```powershell
cargo test --test windows_app taskbar_dot_ -- --nocapture
cargo test --lib windows::native::platform::tests::taskbar_indicator_colors_are_stable -- --nocapture
```

Expected: `TaskbarVisualState`/indicator color helper 미정의로 FAIL.

- [ ] **Step 3: 순수 상태 매핑을 구현한다**

`src/windows/taskbar_widget.rs`에 다음 타입을 한국어 rustdoc과 함께 추가한다.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarIndicator {
    Comfortable,
    Normal,
    Fast,
    Neutral,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskbarVisualState {
    pub indicator: TaskbarIndicator,
    pub progress_risk: TaskbarRisk,
}

pub fn taskbar_visual_state(view: &WidgetViewModel) -> TaskbarVisualState
```

`progress_risk`은 secondary 우선 row의 `used_percent`만 보고 계산하고 row가 없을 때 `Loading`이다. indicator 우선순위는 `WidgetDataState::Error -> Error`, `Loading -> Neutral`, `Ready -> pace state`다. `Comfortable/Normal/Fast`는 같은 indicator로, `Exhausted`는 `Fast`로, `Measuring/Unavailable/Disabled`는 `Neutral`로 변환한다.

- [ ] **Step 4: Win32 렌더러의 색을 분리한다**

`paint_compact_taskbar_content`에서 기존 단일 `accent`를 다음 두 값으로 나눈다.

```rust
let visual = taskbar_visual_state(view);
let indicator_accent = taskbar_indicator_color(visual.indicator);
let progress_accent = taskbar_risk_color(visual.progress_risk);
```

점 또는 `!`는 `indicator_accent`, progress fill은 `progress_accent`를 사용한다. `TaskbarIndicator::Error`일 때만 `!`를 그린다. 색상은 기존 semantic palette를 재사용한다.

```rust
const fn taskbar_indicator_color(indicator: TaskbarIndicator) -> COLORREF {
    match indicator {
        TaskbarIndicator::Comfortable => COLORREF(0x0074_c748),
        TaskbarIndicator::Normal => COLORREF(0x0023_a6f5),
        TaskbarIndicator::Fast | TaskbarIndicator::Error => COLORREF(0x005c_5cff),
        TaskbarIndicator::Neutral => COLORREF(0x0097_9797),
    }
}
```

- [ ] **Step 5: 렌더 상태와 레이아웃 회귀 테스트를 통과시킨다**

Run:

```powershell
cargo test --test windows_app taskbar_
cargo test --lib windows::native::platform::tests
```

Expected: pace dot, progress risk, error/loading priority, full/compact/minimal layout와 DPI tests PASS.

- [ ] **Step 6: 렌더러 변경을 커밋한다**

```powershell
git add src/windows/taskbar_widget.rs src/windows/native/platform.rs tests/windows_app.rs
git commit -m "feat: Show consumption pace in widget status dot"
```

---

### Task 5: 사용자 문서와 전체 검증

**Files:**
- Modify: `README.md`
- Modify: `docs/translations/README.ko.md`
- Modify: `docs/ACCOUNT_STORAGE.md`
- Modify: `docs/RELEASE_CHECKLIST.md`
- Verify: all files changed by Tasks 1-4

**Interfaces:**
- Consumes: 확정된 속도 등급, 점/막대 의미와 호버 문구
- Produces: 사용자 설명, 데이터 보관 설명, 수동 릴리스 검증 항목
- Preserves: 인증 파일 비열람, 원본 payload 비보관, CLI·IDE 로그인 비변경 문서 계약

- [ ] **Step 1: README 양 언어에 사용자 동작을 설명한다**

`README.md`와 `docs/translations/README.ko.md`의 forecast 설명에 다음 내용을 각각 영어/한국어로 추가한다.

```text
The widget's upper-left dot summarizes the displayed window's current usage pace: green means comfortable, amber means moderate, and red means the current pace may exhaust the limit before reset. Hover details explain the rating using recent observation time, usage increase, and approximate hourly rate. Loading or unavailable measurements use gray; a refresh error keeps the red exclamation mark.
```

```text
위젯 좌측 상단 점은 현재 표시 중인 사용량 창의 소비 속도를 요약합니다. 초록은 여유, 주황은 보통, 빨강은 현재 속도라면 초기화 전에 한도를 소진할 수 있음을 뜻합니다. 호버 상세에는 최근 관측 시간, 사용량 증가폭과 대략적인 시간당 속도를 쉬운 문장으로 표시합니다. 로딩 또는 판단 불가는 회색, 조회 오류는 기존 빨간 느낌표를 사용합니다.
```

- [ ] **Step 2: 저장·보안 문서와 릴리스 체크리스트를 갱신한다**

`docs/ACCOUNT_STORAGE.md`에 속도 등급과 지표는 기존 `used_percent`, `resets_at`, `observed_at`에서 메모리로 파생하며 새 필드나 토큰 수를 저장하지 않는다고 명시한다.

`docs/RELEASE_CHECKLIST.md`에 다음 수동 항목을 추가한다.

- flat/저활동 3표본·30분 후 초록 점과 `여유` 문구
- 속도 비율 0.5/1.0 경계의 주황/빨강 전환
- 95% 현재 사용량 + 여유 속도에서 빨간 진행 막대와 초록 점의 분리
- 로딩 회색 점, stale/초기화 없음 회색 점, 오류 빨간 `!` 우선순위
- 기능 비활성화 시 회색 점과 `소비 속도 표시 꺼짐`
- 12개 언어, 밝은/어두운 테마, 100/125/150/200% DPI, minimal layout, Explorer 재시작

- [ ] **Step 3: 포맷과 전체 자동 테스트를 실행한다**

Run:

```powershell
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Expected: 모든 명령 exit code 0.

- [ ] **Step 4: 릴리스 빌드를 확인한다**

Run:

```powershell
cargo build --release
```

Expected: `target/release/codex-peek.exe` 생성, exit code 0. 실행 중인 CodexPeek 때문에 파일 잠금이 발생하면 앱을 종료한 뒤 같은 명령을 다시 실행한다.

- [ ] **Step 5: 문서를 커밋한다**

```powershell
git add README.md docs/translations/README.ko.md docs/ACCOUNT_STORAGE.md docs/RELEASE_CHECKLIST.md
git commit -m "docs: Explain consumption pace indicator"
```

- [ ] **Step 6: 완료 증거를 확인한다**

Run:

```powershell
git status --short
git log -5 --oneline
```

Expected: 작업 트리가 clean이고 Tasks 1-5의 논리적 커밋이 최신 이력에 존재한다. 실제 Windows 수동 검증을 실행하지 않았다면 최종 보고에 체크리스트 경로와 미실행 사유를 명시한다.
