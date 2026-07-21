/// 사용자에게 표시할 문구의 언어를 나타냅니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// 한국어 문구를 사용합니다.
    Korean,
    /// 영어 문구를 사용합니다.
    English,
}

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn language_variants_are_available_for_all_supported_locales() {
        assert_ne!(Language::Korean, Language::English);
    }
}
