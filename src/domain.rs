use std::time::{Duration, SystemTime};

use crate::{Language, UsageError};

/// 사용량 제한 창의 종류를 구분합니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Windows 현지 시간대로 변환된 초기화 날짜와 시각입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResetDateTime {
    year: u16,
    month: u16,
    day: u16,
    weekday: u16,
    hour: u16,
    minute: u16,
}

impl ResetDateTime {
    /// 검증된 달력 구성 요소로 초기화 시각을 만듭니다.
    ///
    /// `weekday`는 Windows `SYSTEMTIME`과 같은 일요일 0부터 토요일 6까지의 값입니다.
    pub(crate) fn new(
        year: u16,
        month: u16,
        day: u16,
        weekday: u16,
        hour: u16,
        minute: u16,
    ) -> Option<Self> {
        if year == 0
            || !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || weekday > 6
            || hour > 23
            || minute > 59
        {
            return None;
        }

        Some(Self {
            year,
            month,
            day,
            weekday,
            hour,
            minute,
        })
    }

    /// 날짜 순서와 현지화된 요일을 적용해 초기화 시각을 반환합니다.
    pub(crate) fn localized_label(self, language: Language) -> String {
        let weekday = localized_weekday(language, self.weekday as usize);
        match language {
            Language::Korean | Language::English => format!(
                "{:04}-{:02}-{:02} ({weekday}) {:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute
            ),
            Language::Spanish
            | Language::PortugueseBrazil
            | Language::Indonesian
            | Language::French
            | Language::Vietnamese
            | Language::Arabic => format!(
                "{:02}/{:02}/{:04} ({weekday}) {:02}:{:02}",
                self.day, self.month, self.year, self.hour, self.minute
            ),
            Language::Japanese => format!(
                "{:04}年{:02}月{:02}日 ({weekday}) {:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute
            ),
            Language::Hindi => format!(
                "{:02}-{:02}-{:04} ({weekday}) {:02}:{:02}",
                self.day, self.month, self.year, self.hour, self.minute
            ),
            Language::German | Language::Turkish => format!(
                "{:02}.{:02}.{:04} ({weekday}) {:02}:{:02}",
                self.day, self.month, self.year, self.hour, self.minute
            ),
        }
    }
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
            return format_duration_unit(duration_mins / (24 * 60), DurationUnit::Day, language);
        }

        if duration_mins % 60 == 0 {
            return format_duration_unit(duration_mins / 60, DurationUnit::Hour, language);
        }

        format_duration_unit(duration_mins, DurationUnit::Minute, language)
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
    /// 사용 가능한 리셋권 정보입니다. 서버가 제공하지 않으면 `None`입니다.
    pub reset_credits: Option<ResetCredits>,
    /// 사용량 정보를 성공적으로 가져온 시각입니다.
    pub fetched_at: SystemTime,
}

fn fallback_period_label(kind: WindowKind, language: Language) -> &'static str {
    let (primary, secondary) = match language {
        Language::Korean => ("단기", "주간"),
        Language::English => ("Short", "Weekly"),
        Language::Spanish => ("Corto", "Semanal"),
        Language::PortugueseBrazil => ("Curto", "Semanal"),
        Language::Indonesian => ("Singkat", "Mingguan"),
        Language::Japanese => ("短期", "週間"),
        Language::Hindi => ("अल्पकालिक", "साप्ताहिक"),
        Language::German => ("Kurz", "Wöchentlich"),
        Language::French => ("Court", "Hebdomadaire"),
        Language::Vietnamese => ("Ngắn hạn", "Hàng tuần"),
        Language::Turkish => ("Kısa", "Haftalık"),
        Language::Arabic => ("قصير", "أسبوعي"),
    };
    match kind {
        WindowKind::Primary => primary,
        WindowKind::Secondary => secondary,
    }
}

pub(crate) fn reset_unavailable_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "초기화 시각 없음",
        Language::English => "Reset unavailable",
        Language::Spanish => "Restablecimiento no disponible",
        Language::PortugueseBrazil => "Redefinição indisponível",
        Language::Indonesian => "Waktu reset tidak tersedia",
        Language::Japanese => "リセット時刻なし",
        Language::Hindi => "रीसेट समय उपलब्ध नहीं",
        Language::German => "Zurücksetzen nicht verfügbar",
        Language::French => "Réinitialisation indisponible",
        Language::Vietnamese => "Không có thời gian đặt lại",
        Language::Turkish => "Sıfırlama zamanı yok",
        Language::Arabic => "وقت إعادة التعيين غير متاح",
    }
}

fn days_in_month(year: u16, month: u16) -> u16 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

fn reset_soon_label(language: Language) -> &'static str {
    match language {
        Language::Korean => "곧 초기화",
        Language::English => "Reset soon",
        Language::Spanish => "Se restablece pronto",
        Language::PortugueseBrazil => "Redefinição em breve",
        Language::Indonesian => "Segera direset",
        Language::Japanese => "まもなくリセット",
        Language::Hindi => "जल्द रीसेट होगा",
        Language::German => "Bald zurückgesetzt",
        Language::French => "Réinitialisation imminente",
        Language::Vietnamese => "Sắp đặt lại",
        Language::Turkish => "Yakında sıfırlanır",
        Language::Arabic => "ستتم إعادة التعيين قريبًا",
    }
}

/// 사용량 제한 창을 완전히 초기화할 수 있는 리셋권 정보입니다.
///
/// 개수는 서버가 알려준 권위 있는 값이며, 만료 시각은 표시 지원용으로만 사용합니다.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResetCredits {
    /// 사용 가능한 리셋권 개수입니다.
    pub available_count: u32,
    /// 보유 리셋권 중 가장 빠른 만료 시각입니다.
    pub nearest_expiry: Option<SystemTime>,
}

/// 리셋권 개수와 현지화된 만료 문자열로 표시 문구를 만듭니다.
///
/// `expiry_text`는 이미 현지 시간대로 변환된 만료 표시 문자열입니다.
/// 개수가 0이면 표시할 의미가 없으므로 `None`을 반환합니다.
pub(crate) fn reset_credits_label(
    available_count: u32,
    expiry_text: Option<&str>,
    language: Language,
) -> Option<String> {
    if available_count == 0 {
        return None;
    }
    Some(match (language, expiry_text) {
        (Language::Korean, Some(expiry)) => {
            format!("Full reset {available_count}개 (만료 {expiry})")
        }
        (Language::Korean, None) => format!("Full reset {available_count}개"),
        (Language::English, Some(expiry)) => {
            format!("Full reset: {available_count} (expires {expiry})")
        }
        (Language::English, None) => format!("Full reset: {available_count}"),
        (Language::Spanish, Some(expiry)) => {
            format!("Restablecimiento completo: {available_count} (vence {expiry})")
        }
        (Language::Spanish, None) => {
            format!("Restablecimiento completo: {available_count}")
        }
        (Language::PortugueseBrazil, Some(expiry)) => {
            format!("Redefinição completa: {available_count} (expira em {expiry})")
        }
        (Language::PortugueseBrazil, None) => {
            format!("Redefinição completa: {available_count}")
        }
        (Language::Indonesian, Some(expiry)) => {
            format!("Reset penuh: {available_count} (kedaluwarsa {expiry})")
        }
        (Language::Indonesian, None) => format!("Reset penuh: {available_count}"),
        (Language::Japanese, Some(expiry)) => {
            format!("フルリセット: {available_count}（有効期限 {expiry}）")
        }
        (Language::Japanese, None) => format!("フルリセット: {available_count}"),
        (Language::Hindi, Some(expiry)) => {
            format!("पूर्ण रीसेट: {available_count} (समाप्ति {expiry})")
        }
        (Language::Hindi, None) => format!("पूर्ण रीसेट: {available_count}"),
        (Language::German, Some(expiry)) => {
            format!("Vollständige Zurücksetzung: {available_count} (läuft am {expiry} ab)")
        }
        (Language::German, None) => {
            format!("Vollständige Zurücksetzung: {available_count}")
        }
        (Language::French, Some(expiry)) => {
            format!("Réinitialisation complète : {available_count} (expire le {expiry})")
        }
        (Language::French, None) => {
            format!("Réinitialisation complète : {available_count}")
        }
        (Language::Vietnamese, Some(expiry)) => {
            format!("Đặt lại toàn bộ: {available_count} (hết hạn {expiry})")
        }
        (Language::Vietnamese, None) => {
            format!("Đặt lại toàn bộ: {available_count}")
        }
        (Language::Turkish, Some(expiry)) => {
            format!("Tam sıfırlama: {available_count} (sona erme {expiry})")
        }
        (Language::Turkish, None) => format!("Tam sıfırlama: {available_count}"),
        (Language::Arabic, Some(expiry)) => {
            format!("إعادة ضبط كاملة: {available_count} (تنتهي في {expiry})")
        }
        (Language::Arabic, None) => format!("إعادة ضبط كاملة: {available_count}"),
    })
}

fn format_remaining_duration(remaining: Duration, language: Language) -> String {
    let minutes = remaining.as_secs() / 60
        + u64::from(remaining.as_secs() % 60 > 0 || remaining.subsec_nanos() > 0);
    let days = minutes / (24 * 60);
    let hours = (minutes % (24 * 60)) / 60;
    let minutes = minutes % 60;

    if days > 0 {
        return format!(
            "{} {}",
            format_duration_unit(days, DurationUnit::Day, language),
            format_duration_unit(hours, DurationUnit::Hour, language)
        );
    }
    if hours > 0 {
        return format!(
            "{} {}",
            format_duration_unit(hours, DurationUnit::Hour, language),
            format_duration_unit(minutes, DurationUnit::Minute, language)
        );
    }
    format_duration_unit(minutes, DurationUnit::Minute, language)
}

#[derive(Clone, Copy)]
enum DurationUnit {
    Day,
    Hour,
    Minute,
}

fn localized_weekday(language: Language, weekday: usize) -> &'static str {
    const KOREAN: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];
    const ENGLISH: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const SPANISH: [&str; 7] = ["dom.", "lun.", "mar.", "mié.", "jue.", "vie.", "sáb."];
    const PORTUGUESE_BRAZIL: [&str; 7] = ["dom.", "seg.", "ter.", "qua.", "qui.", "sex.", "sáb."];
    const INDONESIAN: [&str; 7] = ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"];
    const JAPANESE: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];
    const HINDI: [&str; 7] = ["रवि", "सोम", "मंगल", "बुध", "गुरु", "शुक्र", "शनि"];
    const GERMAN: [&str; 7] = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];
    const FRENCH: [&str; 7] = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."];
    const VIETNAMESE: [&str; 7] = ["CN", "Th 2", "Th 3", "Th 4", "Th 5", "Th 6", "Th 7"];
    const TURKISH: [&str; 7] = ["Paz", "Pzt", "Sal", "Çar", "Per", "Cum", "Cmt"];
    const ARABIC: [&str; 7] = [
        "الأحد",
        "الاثنين",
        "الثلاثاء",
        "الأربعاء",
        "الخميس",
        "الجمعة",
        "السبت",
    ];

    match language {
        Language::Korean => KOREAN[weekday],
        Language::English => ENGLISH[weekday],
        Language::Spanish => SPANISH[weekday],
        Language::PortugueseBrazil => PORTUGUESE_BRAZIL[weekday],
        Language::Indonesian => INDONESIAN[weekday],
        Language::Japanese => JAPANESE[weekday],
        Language::Hindi => HINDI[weekday],
        Language::German => GERMAN[weekday],
        Language::French => FRENCH[weekday],
        Language::Vietnamese => VIETNAMESE[weekday],
        Language::Turkish => TURKISH[weekday],
        Language::Arabic => ARABIC[weekday],
    }
}

fn format_duration_unit(value: u64, unit: DurationUnit, language: Language) -> String {
    match (unit, language) {
        (DurationUnit::Day, Language::Korean) => format!("{value}일"),
        (DurationUnit::Day, Language::English) => format!("{value}d"),
        (DurationUnit::Day, Language::Spanish) => {
            format!("{value} {}", if value == 1 { "día" } else { "días" })
        }
        (DurationUnit::Day, Language::PortugueseBrazil) => {
            format!("{value} {}", if value == 1 { "dia" } else { "dias" })
        }
        (DurationUnit::Day, Language::Indonesian) => format!("{value} hari"),
        (DurationUnit::Day, Language::Japanese) => format!("{value}日"),
        (DurationUnit::Day, Language::Hindi) => format!("{value} दिन"),
        (DurationUnit::Day, Language::German) => {
            format!("{value} {}", if value == 1 { "Tag" } else { "Tage" })
        }
        (DurationUnit::Day, Language::French) => {
            format!("{value} {}", if value == 1 { "jour" } else { "jours" })
        }
        (DurationUnit::Day, Language::Vietnamese) => format!("{value} ngày"),
        (DurationUnit::Day, Language::Turkish) => format!("{value} gün"),
        (DurationUnit::Day, Language::Arabic) => format!("{value} يوم"),
        (DurationUnit::Hour, Language::Korean) => format!("{value}시간"),
        (DurationUnit::Hour, Language::English) => format!("{value}h"),
        (DurationUnit::Hour, Language::Spanish) => {
            format!("{value} {}", if value == 1 { "hora" } else { "horas" })
        }
        (DurationUnit::Hour, Language::PortugueseBrazil) => {
            format!("{value} {}", if value == 1 { "hora" } else { "horas" })
        }
        (DurationUnit::Hour, Language::Indonesian) => format!("{value} jam"),
        (DurationUnit::Hour, Language::Japanese) => format!("{value}時間"),
        (DurationUnit::Hour, Language::Hindi) => {
            format!(
                "{value} {}",
                if value == 1 {
                    "घंटा"
                } else {
                    "घंटे"
                }
            )
        }
        (DurationUnit::Hour, Language::German) => {
            format!("{value} {}", if value == 1 { "Stunde" } else { "Stunden" })
        }
        (DurationUnit::Hour, Language::French) => {
            format!("{value} {}", if value == 1 { "heure" } else { "heures" })
        }
        (DurationUnit::Hour, Language::Vietnamese) => format!("{value} giờ"),
        (DurationUnit::Hour, Language::Turkish) => format!("{value} saat"),
        (DurationUnit::Hour, Language::Arabic) => format!("{value} ساعة"),
        (DurationUnit::Minute, Language::Korean) => format!("{value}분"),
        (DurationUnit::Minute, Language::English) => format!("{value}m"),
        (DurationUnit::Minute, Language::Spanish) => {
            format!("{value} {}", if value == 1 { "minuto" } else { "minutos" })
        }
        (DurationUnit::Minute, Language::PortugueseBrazil) => {
            format!("{value} {}", if value == 1 { "minuto" } else { "minutos" })
        }
        (DurationUnit::Minute, Language::Indonesian) => format!("{value} menit"),
        (DurationUnit::Minute, Language::Japanese) => format!("{value}分"),
        (DurationUnit::Minute, Language::Hindi) => format!("{value} मिनट"),
        (DurationUnit::Minute, Language::German) => {
            format!("{value} {}", if value == 1 { "Minute" } else { "Minuten" })
        }
        (DurationUnit::Minute, Language::French) => {
            format!("{value} {}", if value == 1 { "minute" } else { "minutes" })
        }
        (DurationUnit::Minute, Language::Vietnamese) => format!("{value} phút"),
        (DurationUnit::Minute, Language::Turkish) => format!("{value} dakika"),
        (DurationUnit::Minute, Language::Arabic) => format!("{value} دقيقة"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{ResetDateTime, UsageLevel, UsageWindow, WindowKind};
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

    #[test]
    fn reset_date_time_formats_local_weekdays_in_both_languages() {
        let monday = ResetDateTime::new(2026, 7, 27, 1, 3, 4).unwrap();
        let sunday = ResetDateTime::new(2026, 8, 2, 0, 13, 9).unwrap();

        assert_eq!(
            monday.localized_label(Language::Korean),
            "2026-07-27 (월) 03:04"
        );
        assert_eq!(
            monday.localized_label(Language::English),
            "2026-07-27 (Mon) 03:04"
        );
        assert_eq!(
            sunday.localized_label(Language::Korean),
            "2026-08-02 (일) 13:09"
        );
        assert_eq!(
            sunday.localized_label(Language::English),
            "2026-08-02 (Sun) 13:09"
        );
    }

    #[test]
    fn every_supported_language_has_complete_dynamic_domain_copy() {
        let now = UNIX_EPOCH + Duration::from_secs(10_000);
        let reset = ResetDateTime::new(2026, 7, 27, 1, 3, 4).unwrap();
        let day_period = UsageWindow::new(WindowKind::Primary, 1.0, Some(1_440), None).unwrap();
        let hour_period = UsageWindow::new(WindowKind::Primary, 1.0, Some(120), None).unwrap();
        let minute_period = UsageWindow::new(WindowKind::Primary, 1.0, Some(1), None).unwrap();
        let primary_fallback = UsageWindow::new(WindowKind::Primary, 1.0, None, None).unwrap();
        let secondary_fallback = UsageWindow::new(WindowKind::Secondary, 1.0, None, None).unwrap();
        let days_remaining = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now + Duration::from_secs(26 * 60 * 60)),
        )
        .unwrap();
        let hours_remaining = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now + Duration::from_secs(2 * 60 * 60 + 61)),
        )
        .unwrap();
        let minutes_remaining = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now + Duration::from_secs(1)),
        )
        .unwrap();
        let elapsed = UsageWindow::new(
            WindowKind::Primary,
            1.0,
            None,
            Some(now - Duration::from_secs(1)),
        )
        .unwrap();

        for &language in Language::ALL {
            for text in [
                reset.localized_label(language),
                day_period.period_label(language),
                hour_period.period_label(language),
                minute_period.period_label(language),
                primary_fallback.period_label(language),
                secondary_fallback.period_label(language),
                days_remaining.remaining_label(language, now),
                hours_remaining.remaining_label(language, now),
                minutes_remaining.remaining_label(language, now),
                primary_fallback.remaining_label(language, now),
                elapsed.remaining_label(language, now),
            ] {
                assert!(!text.trim().is_empty(), "{language:?}");
            }
        }
    }

    #[test]
    fn dynamic_domain_copy_uses_representative_local_scripts_and_formats() {
        let monday = ResetDateTime::new(2026, 7, 27, 1, 3, 4).unwrap();
        let day = UsageWindow::new(WindowKind::Primary, 1.0, Some(1_440), None).unwrap();
        let hours = UsageWindow::new(WindowKind::Primary, 1.0, Some(120), None).unwrap();

        assert_eq!(
            monday.localized_label(Language::Japanese),
            "2026年07月27日 (月) 03:04"
        );
        assert_eq!(
            monday.localized_label(Language::Arabic),
            "27/07/2026 (الاثنين) 03:04"
        );
        assert_eq!(day.period_label(Language::Spanish), "1 día");
        assert_eq!(hours.period_label(Language::German), "2 Stunden");
    }

    #[test]
    fn reset_date_time_rejects_invalid_calendar_parts() {
        assert!(ResetDateTime::new(2026, 0, 27, 1, 3, 4).is_none());
        assert!(ResetDateTime::new(2026, 2, 29, 0, 3, 4).is_none());
        assert!(ResetDateTime::new(2024, 2, 29, 4, 3, 4).is_some());
        assert!(ResetDateTime::new(2026, 7, 27, 7, 3, 4).is_none());
        assert!(ResetDateTime::new(2026, 7, 27, 1, 24, 4).is_none());
        assert!(ResetDateTime::new(2026, 7, 27, 1, 3, 60).is_none());
    }

    #[test]
    fn reset_credits_label_formats_count_and_optional_expiry() {
        assert_eq!(
            super::reset_credits_label(1, Some("2026-07-31 (목) 23:59"), Language::Korean),
            Some("Full reset 1개 (만료 2026-07-31 (목) 23:59)".to_owned())
        );
        assert_eq!(
            super::reset_credits_label(1, None, Language::Korean),
            Some("Full reset 1개".to_owned())
        );
        assert_eq!(
            super::reset_credits_label(2, Some("2026-07-31 (Thu) 23:59"), Language::English),
            Some("Full reset: 2 (expires 2026-07-31 (Thu) 23:59)".to_owned())
        );
        assert_eq!(
            super::reset_credits_label(2, None, Language::English),
            Some("Full reset: 2".to_owned())
        );
        for &language in Language::ALL {
            let with_expiry =
                super::reset_credits_label(2, Some("2026-07-31 23:59"), language).unwrap();
            let without_expiry = super::reset_credits_label(2, None, language).unwrap();
            assert!(with_expiry.contains('2'), "{language:?}: {with_expiry}");
            assert!(
                with_expiry.contains("2026-07-31 23:59"),
                "{language:?}: {with_expiry}"
            );
            assert!(
                without_expiry.contains('2'),
                "{language:?}: {without_expiry}"
            );
        }
    }

    #[test]
    fn reset_credits_label_omits_zero_and_missing_counts() {
        assert_eq!(
            super::reset_credits_label(0, Some("2026-07-31"), Language::Korean),
            None
        );
        assert_eq!(super::reset_credits_label(0, None, Language::English), None);
    }
}
