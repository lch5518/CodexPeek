use std::time::{Duration, SystemTime};

use crate::{Language, UsageError};

/// 사용량 제한 창의 종류를 구분합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    /// 짧은 주기의 기본 사용량 창입니다.
    Primary,
    /// 긴 주기의 보조 사용량 창입니다.
    Secondary,
}

/// 사용량 비율에 따라 표시할 상태 수준입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageLevel {
    /// 사용량이 0% 이상 50% 미만인 안정 상태입니다.
    Stable,
    /// 사용량이 50% 이상 75% 미만인 일반 상태입니다.
    Normal,
    /// 사용량이 75% 이상 90% 미만인 주의 상태입니다.
    Caution,
    /// 사용량이 90% 이상 100% 미만인 위험 상태입니다.
    Danger,
    /// 사용량이 100% 이상인 제한 상태입니다.
    Limited,
}

/// 하나의 사용량 제한 창과 다음 초기화 시각을 표현합니다.
#[derive(Clone, Debug, PartialEq)]
pub struct UsageWindow {
    /// 기본 또는 보조 사용량 창의 종류입니다.
    pub kind: WindowKind,
    /// 원본 사용량 비율이며 100을 초과하는 값도 보존합니다.
    pub used_percent: f64,
    /// 서버가 제공한 사용량 창 길이(분)입니다.
    pub window_duration_mins: Option<u64>,
    /// 서버가 제공한 다음 초기화 시각입니다.
    pub resets_at: Option<SystemTime>,
}

impl UsageWindow {
    /// 유효한 사용량 비율로 사용량 창을 생성합니다.
    ///
    /// 음수 또는 유한하지 않은 비율은 유효하지 않은 서버 응답으로 처리합니다.
    pub fn new(
        kind: WindowKind,
        used_percent: f64,
        window_duration_mins: Option<u64>,
        resets_at: Option<SystemTime>,
    ) -> Result<Self, UsageError> {
        if !used_percent.is_finite() || used_percent < 0.0 {
            return Err(UsageError::InvalidResponse);
        }

        Ok(Self {
            kind,
            used_percent,
            window_duration_mins,
            resets_at,
        })
    }

    /// 막대 렌더링에 사용할 0부터 100까지의 비율을 반환합니다.
    pub fn bar_percent(&self) -> f64 {
        self.used_percent.clamp(0.0, 100.0)
    }

    /// 원본 사용량 비율에 대응하는 전역 상태 수준을 반환합니다.
    pub fn level(&self) -> UsageLevel {
        match self.used_percent {
            value if value < 50.0 => UsageLevel::Stable,
            value if value < 75.0 => UsageLevel::Normal,
            value if value < 90.0 => UsageLevel::Caution,
            value if value < 100.0 => UsageLevel::Danger,
            _ => UsageLevel::Limited,
        }
    }

    /// 사용량 창의 실제 길이 또는 종류별 대체 문구를 반환합니다.
    pub fn period_label(&self, language: Language) -> String {
        let Some(duration_mins) = self.window_duration_mins.filter(|duration| *duration > 0) else {
            return fallback_period_label(self.kind, language).to_owned();
        };

        if duration_mins % (24 * 60) == 0 {
            return match language {
                Language::Korean => format!("{}일", duration_mins / (24 * 60)),
                Language::English => format!("{}d", duration_mins / (24 * 60)),
            };
        }

        if duration_mins % 60 == 0 {
            return match language {
                Language::Korean => format!("{}시간", duration_mins / 60),
                Language::English => format!("{}h", duration_mins / 60),
            };
        }

        match language {
            Language::Korean => format!("{duration_mins}분"),
            Language::English => format!("{duration_mins}m"),
        }
    }

    /// 현재 시각을 기준으로 다음 초기화까지 남은 시간을 반환합니다.
    pub fn remaining_label(&self, language: Language, now: SystemTime) -> String {
        let Some(resets_at) = self.resets_at else {
            return reset_unavailable_label(language).to_owned();
        };
        let Ok(remaining) = resets_at.duration_since(now) else {
            return reset_soon_label(language).to_owned();
        };

        format_remaining_duration(remaining, language)
    }
}

/// 기본 및 보조 사용량 창을 한 번에 전달하는 조회 결과입니다.
#[derive(Clone, Debug, PartialEq)]
pub struct CodexUsage {
    /// 짧은 주기의 기본 사용량 창입니다.
    pub primary: Option<UsageWindow>,
    /// 긴 주기의 보조 사용량 창입니다.
    pub secondary: Option<UsageWindow>,
    /// 사용량 정보를 성공적으로 가져온 시각입니다.
    pub fetched_at: SystemTime,
}

fn fallback_period_label(kind: WindowKind, language: Language) -> &'static str {
    match (kind, language) {
        (WindowKind::Primary, Language::Korean) => "단기",
        (WindowKind::Primary, Language::English) => "Short",
        (WindowKind::Secondary, Language::Korean) => "주간",
        (WindowKind::Secondary, Language::English) => "Weekly",
    }
}

fn reset_unavailable_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "초기화 시각 없음",
        Language::English => "Reset unavailable",
    }
}

fn reset_soon_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "곧 초기화",
        Language::English => "Reset soon",
    }
}

fn format_remaining_duration(remaining: Duration, language: Language) -> String {
    let minutes = remaining.as_secs() / 60
        + u64::from(remaining.as_secs() % 60 > 0 || remaining.subsec_nanos() > 0);
    let days = minutes / (24 * 60);
    let hours = (minutes % (24 * 60)) / 60;
    let minutes = minutes % 60;

    match (days, hours, language) {
        (days, hours, Language::Korean) if days > 0 => format!("{days}일 {hours}시간"),
        (days, hours, Language::English) if days > 0 => format!("{days}d {hours}h"),
        (_, hours, Language::Korean) if hours > 0 => format!("{hours}시간 {minutes}분"),
        (_, hours, Language::English) if hours > 0 => format!("{hours}h {minutes}m"),
        (_, _, Language::Korean) => format!("{minutes}분"),
        (_, _, Language::English) => format!("{minutes}m"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{UsageLevel, UsageWindow, WindowKind};
    use crate::{Language, UsageError};

    fn window(used_percent: f64) -> UsageWindow {
        UsageWindow::new(WindowKind::Primary, used_percent, None, None).unwrap()
    }

    #[test]
    fn usage_levels_follow_the_global_thresholds() {
        let cases = [
            (0.0, UsageLevel::Stable),
            (49.0, UsageLevel::Stable),
            (50.0, UsageLevel::Normal),
            (74.0, UsageLevel::Normal),
            (75.0, UsageLevel::Caution),
            (89.0, UsageLevel::Caution),
            (90.0, UsageLevel::Danger),
            (99.0, UsageLevel::Danger),
            (100.0, UsageLevel::Limited),
            (125.0, UsageLevel::Limited),
        ];

        for (used_percent, expected) in cases {
            assert_eq!(window(used_percent).level(), expected);
        }
    }

    #[test]
    fn usage_window_rejects_negative_and_non_finite_percentages() {
        for used_percent in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                UsageWindow::new(WindowKind::Primary, used_percent, None, None),
                Err(UsageError::InvalidResponse)
            );
        }
    }

    #[test]
    fn usage_window_accepts_negative_zero_as_stable() {
        let usage = UsageWindow::new(WindowKind::Primary, -0.0, None, None).unwrap();

        assert_eq!(usage.level(), UsageLevel::Stable);
    }

    #[test]
    fn bar_percent_clamps_only_the_rendered_value() {
        let usage = window(125.0);

        assert_eq!(usage.used_percent, 125.0);
        assert_eq!(usage.bar_percent(), 100.0);
    }

    #[test]
    fn period_label_uses_positive_actual_durations() {
        let day = UsageWindow::new(WindowKind::Primary, 1.0, Some(1_440), None).unwrap();
        let hour = UsageWindow::new(WindowKind::Primary, 1.0, Some(120), None).unwrap();
        let minute = UsageWindow::new(WindowKind::Primary, 1.0, Some(59), None).unwrap();

        assert_eq!(day.period_label(Language::English), "1d");
        assert_eq!(hour.period_label(Language::English), "2h");
        assert_eq!(minute.period_label(Language::English), "59m");
        assert_eq!(day.period_label(Language::Korean), "1일");
        assert_eq!(hour.period_label(Language::Korean), "2시간");
        assert_eq!(minute.period_label(Language::Korean), "59분");
    }

    #[test]
    fn period_label_uses_kind_specific_fallback_for_missing_or_zero_duration() {
        let primary = UsageWindow::new(WindowKind::Primary, 1.0, None, None).unwrap();
        let secondary = UsageWindow::new(WindowKind::Secondary, 1.0, Some(0), None).unwrap();

        assert_eq!(primary.period_label(Language::English), "Short");
        assert_eq!(secondary.period_label(Language::English), "Weekly");
        assert_eq!(primary.period_label(Language::Korean), "단기");
        assert_eq!(secondary.period_label(Language::Korean), "주간");
    }

    #[test]
    fn remaining_label_rounds_up_and_uses_the_largest_relevant_units() {
        let now = UNIX_EPOCH + Duration::from_secs(10_000);
        let window = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now + Duration::from_secs(26 * 60 * 60)),
        )
        .unwrap();
        let hours = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now + Duration::from_secs(2 * 60 * 60 + 61)),
        )
        .unwrap();
        let minutes = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now + Duration::from_secs(1)),
        )
        .unwrap();

        assert_eq!(window.remaining_label(Language::English, now), "1d 2h");
        assert_eq!(hours.remaining_label(Language::English, now), "2h 2m");
        assert_eq!(minutes.remaining_label(Language::English, now), "1m");
        assert_eq!(window.remaining_label(Language::Korean, now), "1일 2시간");
        assert_eq!(hours.remaining_label(Language::Korean, now), "2시간 2분");
        assert_eq!(minutes.remaining_label(Language::Korean, now), "1분");
    }

    #[test]
    fn remaining_label_handles_missing_and_elapsed_reset_timestamps() {
        let now = UNIX_EPOCH + Duration::from_secs(10_000);
        let missing = UsageWindow::new(WindowKind::Primary, 1.0, None, None).unwrap();
        let elapsed = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now - Duration::from_secs(1)),
        )
        .unwrap();

        assert_eq!(
            missing.remaining_label(Language::English, now),
            "Reset unavailable"
        );
        assert_eq!(
            elapsed.remaining_label(Language::English, now),
            "Reset soon"
        );
        assert_eq!(
            missing.remaining_label(Language::Korean, now),
            "초기화 시각 없음"
        );
        assert_eq!(elapsed.remaining_label(Language::Korean, now), "곧 초기화");
    }
}
