use crate::localization::Language;

/// 사용량을 조회하거나 해석하는 과정에서 발생할 수 있는 오류입니다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// Codex CLI 실행 파일을 찾지 못했습니다.
    CliNotFound,
    /// 설치된 Codex CLI가 지원 범위를 벗어났습니다.
    UnsupportedCli,
    /// Codex 앱 서버를 시작하지 못했습니다.
    AppServerStartFailed,
    /// RPC 응답 대기 시간이 만료되었습니다.
    RpcTimeout,
    /// RPC 서버가 요청을 처리할 수 없을 정도로 혼잡합니다.
    RpcOverloaded,
    /// Codex 로그인이 필요합니다.
    NotLoggedIn,
    /// Codex 인증 정보가 만료되었습니다.
    AuthenticationExpired,
    /// Codex 응답 형식이 유효하지 않습니다.
    InvalidResponse,
    /// 사용량 한도 정보를 확인할 수 없습니다.
    RateLimitUnavailable,
    /// 사용량 요청이 완료되지 못했습니다.
    RequestFailed,
}

struct LocalizedMessages {
    korean: &'static str,
    english: &'static str,
    spanish: &'static str,
    portuguese_brazil: &'static str,
    indonesian: &'static str,
    japanese: &'static str,
    hindi: &'static str,
    german: &'static str,
    french: &'static str,
    vietnamese: &'static str,
    turkish: &'static str,
    arabic: &'static str,
}

impl LocalizedMessages {
    const fn for_language(self, language: Language) -> &'static str {
        match language {
            Language::Korean => self.korean,
            Language::English => self.english,
            Language::Spanish => self.spanish,
            Language::PortugueseBrazil => self.portuguese_brazil,
            Language::Indonesian => self.indonesian,
            Language::Japanese => self.japanese,
            Language::Hindi => self.hindi,
            Language::German => self.german,
            Language::French => self.french,
            Language::Vietnamese => self.vietnamese,
            Language::Turkish => self.turkish,
            Language::Arabic => self.arabic,
        }
    }
}

impl UsageError {
    /// 오류를 식별하기 위한 안정적인 진단 코드를 반환합니다.
    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::CliNotFound => "cli_not_found",
            Self::UnsupportedCli => "unsupported_cli",
            Self::AppServerStartFailed => "app_server_start_failed",
            Self::RpcTimeout => "rpc_timeout",
            Self::RpcOverloaded => "rpc_overloaded",
            Self::NotLoggedIn => "not_logged_in",
            Self::AuthenticationExpired => "authentication_expired",
            Self::InvalidResponse => "invalid_response",
            Self::RateLimitUnavailable => "rate_limit_unavailable",
            Self::RequestFailed => "request_failed",
        }
    }

    /// 지정한 언어로 민감한 정보를 포함하지 않는 사용자 안내 문구를 반환합니다.
    pub const fn user_message(self, language: Language) -> &'static str {
        let messages = match self {
            Self::CliNotFound => LocalizedMessages {
                korean: "Codex CLI를 찾을 수 없습니다.",
                english: "Codex CLI was not found.",
                spanish: "No se encontró Codex CLI.",
                portuguese_brazil: "Codex CLI não foi encontrado.",
                indonesian: "Codex CLI tidak ditemukan.",
                japanese: "Codex CLI が見つかりません。",
                hindi: "Codex CLI नहीं मिला।",
                german: "Codex CLI wurde nicht gefunden.",
                french: "Codex CLI est introuvable.",
                vietnamese: "Không tìm thấy Codex CLI.",
                turkish: "Codex CLI bulunamadı.",
                arabic: "لم يتم العثور على Codex CLI.",
            },
            Self::UnsupportedCli => LocalizedMessages {
                korean: "지원하지 않는 Codex CLI 버전입니다.",
                english: "The installed Codex CLI version is unsupported.",
                spanish: "La versión instalada de Codex CLI no es compatible.",
                portuguese_brazil: "A versão instalada do Codex CLI não é compatível.",
                indonesian: "Versi Codex CLI yang terpasang tidak didukung.",
                japanese: "インストールされている Codex CLI のバージョンはサポートされていません。",
                hindi: "स्थापित Codex CLI संस्करण समर्थित नहीं है।",
                german: "Die installierte Codex-CLI-Version wird nicht unterstützt.",
                french: "La version installée de Codex CLI n’est pas prise en charge.",
                vietnamese: "Phiên bản Codex CLI đã cài đặt không được hỗ trợ.",
                turkish: "Yüklü Codex CLI sürümü desteklenmiyor.",
                arabic: "إصدار Codex CLI المثبت غير مدعوم.",
            },
            Self::AppServerStartFailed => LocalizedMessages {
                korean: "Codex 앱 서버를 시작할 수 없습니다.",
                english: "Codex app server could not start.",
                spanish: "No se pudo iniciar el servidor de aplicaciones de Codex.",
                portuguese_brazil: "Não foi possível iniciar o servidor de aplicativos do Codex.",
                indonesian: "Server aplikasi Codex tidak dapat dimulai.",
                japanese: "Codex アプリサーバーを開始できません。",
                hindi: "Codex ऐप सर्वर शुरू नहीं हो सका।",
                german: "Der Codex-App-Server konnte nicht gestartet werden.",
                french: "Le serveur d’application Codex n’a pas pu démarrer.",
                vietnamese: "Không thể khởi động máy chủ ứng dụng Codex.",
                turkish: "Codex uygulama sunucusu başlatılamadı.",
                arabic: "تعذر بدء خادم تطبيق Codex.",
            },
            Self::RpcTimeout => LocalizedMessages {
                korean: "Codex 응답 시간이 초과되었습니다.",
                english: "Codex did not respond in time.",
                spanish: "Codex no respondió a tiempo.",
                portuguese_brazil: "Codex não respondeu a tempo.",
                indonesian: "Codex tidak merespons tepat waktu.",
                japanese: "Codex から時間内に応答がありませんでした。",
                hindi: "Codex ने समय पर जवाब नहीं दिया।",
                german: "Codex hat nicht rechtzeitig geantwortet.",
                french: "Codex n’a pas répondu à temps.",
                vietnamese: "Codex không phản hồi kịp thời.",
                turkish: "Codex zamanında yanıt vermedi.",
                arabic: "لم يستجب Codex في الوقت المناسب.",
            },
            Self::RpcOverloaded => LocalizedMessages {
                korean: "Codex 요청이 혼잡합니다. 잠시 후 다시 시도하세요.",
                english: "Codex is busy. Please try again shortly.",
                spanish: "Codex está ocupado. Inténtalo de nuevo en breve.",
                portuguese_brazil: "O Codex está ocupado. Tente novamente em breve.",
                indonesian: "Codex sedang sibuk. Coba lagi sebentar lagi.",
                japanese: "Codex は混み合っています。しばらくしてからもう一度お試しください。",
                hindi: "Codex व्यस्त है। कृपया थोड़ी देर बाद फिर कोशिश करें।",
                german: "Codex ist ausgelastet. Bitte versuchen Sie es in Kürze erneut.",
                french: "Codex est occupé. Veuillez réessayer dans un instant.",
                vietnamese: "Codex đang bận. Vui lòng thử lại sau ít phút.",
                turkish: "Codex meşgul. Lütfen kısa süre sonra tekrar deneyin.",
                arabic: "Codex مشغول. يرجى المحاولة مرة أخرى قريبًا.",
            },
            Self::NotLoggedIn => LocalizedMessages {
                korean: "Codex에 로그인되어 있지 않습니다.",
                english: "You are not signed in to Codex.",
                spanish: "No has iniciado sesión en Codex.",
                portuguese_brazil: "Você não iniciou sessão no Codex.",
                indonesian: "Anda belum masuk ke Codex.",
                japanese: "Codex にサインインしていません。",
                hindi: "आपने Codex में साइन इन नहीं किया है।",
                german: "Sie sind nicht bei Codex angemeldet.",
                french: "Vous n’êtes pas connecté à Codex.",
                vietnamese: "Bạn chưa đăng nhập vào Codex.",
                turkish: "Codex'te oturum açmadınız.",
                arabic: "لم تسجل الدخول إلى Codex.",
            },
            Self::AuthenticationExpired => LocalizedMessages {
                korean: "Codex 인증이 만료되었습니다.",
                english: "Codex authentication has expired.",
                spanish: "La autenticación de Codex ha caducado.",
                portuguese_brazil: "A autenticação do Codex expirou.",
                indonesian: "Autentikasi Codex telah kedaluwarsa.",
                japanese: "Codex の認証の有効期限が切れています。",
                hindi: "Codex प्रमाणीकरण की समय-सीमा समाप्त हो गई है।",
                german: "Die Codex-Authentifizierung ist abgelaufen.",
                french: "L’authentification Codex a expiré.",
                vietnamese: "Xác thực Codex đã hết hạn.",
                turkish: "Codex kimlik doğrulamasının süresi doldu.",
                arabic: "انتهت صلاحية مصادقة Codex.",
            },
            Self::InvalidResponse => LocalizedMessages {
                korean: "Codex 응답이 올바르지 않습니다.",
                english: "Codex returned an invalid response.",
                spanish: "Codex devolvió una respuesta no válida.",
                portuguese_brazil: "O Codex retornou uma resposta inválida.",
                indonesian: "Codex mengembalikan respons yang tidak valid.",
                japanese: "Codex から無効な応答が返されました。",
                hindi: "Codex ने अमान्य प्रतिक्रिया भेजी।",
                german: "Codex hat eine ungültige Antwort zurückgegeben.",
                french: "Codex a renvoyé une réponse non valide.",
                vietnamese: "Codex trả về phản hồi không hợp lệ.",
                turkish: "Codex geçersiz bir yanıt döndürdü.",
                arabic: "أعاد Codex استجابة غير صالحة.",
            },
            Self::RateLimitUnavailable => LocalizedMessages {
                korean: "사용량 한도 정보를 사용할 수 없습니다.",
                english: "Usage limit information is unavailable.",
                spanish: "La información del límite de uso no está disponible.",
                portuguese_brazil: "As informações de limite de uso não estão disponíveis.",
                indonesian: "Informasi batas penggunaan tidak tersedia.",
                japanese: "使用量制限情報を利用できません。",
                hindi: "उपयोग सीमा की जानकारी उपलब्ध नहीं है।",
                german: "Informationen zum Nutzungslimit sind nicht verfügbar.",
                french: "Les informations sur la limite d’utilisation ne sont pas disponibles.",
                vietnamese: "Thông tin giới hạn sử dụng không có sẵn.",
                turkish: "Kullanım sınırı bilgisi kullanılamıyor.",
                arabic: "معلومات حد الاستخدام غير متاحة.",
            },
            Self::RequestFailed => LocalizedMessages {
                korean: "사용량 요청에 실패했습니다.",
                english: "The usage request failed.",
                spanish: "La solicitud de uso ha fallado.",
                portuguese_brazil: "A solicitação de uso falhou.",
                indonesian: "Permintaan penggunaan gagal.",
                japanese: "使用量リクエストに失敗しました。",
                hindi: "उपयोग अनुरोध विफल रहा।",
                german: "Die Nutzungsanfrage ist fehlgeschlagen.",
                french: "La demande d’utilisation a échoué.",
                vietnamese: "Yêu cầu sử dụng không thành công.",
                turkish: "Kullanım isteği başarısız oldu.",
                arabic: "فشل طلب الاستخدام.",
            },
        };
        messages.for_language(language)
    }
}

#[cfg(test)]
mod tests {
    use super::UsageError;
    use crate::Language;

    #[test]
    fn every_error_has_a_stable_code_and_complete_localized_messages() {
        let cases = [
            (UsageError::CliNotFound, "cli_not_found"),
            (UsageError::UnsupportedCli, "unsupported_cli"),
            (UsageError::AppServerStartFailed, "app_server_start_failed"),
            (UsageError::RpcTimeout, "rpc_timeout"),
            (UsageError::RpcOverloaded, "rpc_overloaded"),
            (UsageError::NotLoggedIn, "not_logged_in"),
            (UsageError::AuthenticationExpired, "authentication_expired"),
            (UsageError::InvalidResponse, "invalid_response"),
            (UsageError::RateLimitUnavailable, "rate_limit_unavailable"),
            (UsageError::RequestFailed, "request_failed"),
        ];

        for (error, expected_code) in cases {
            assert_eq!(error.diagnostic_code(), expected_code);
            for &language in Language::ALL {
                let message = error.user_message(language);
                assert!(!message.trim().is_empty(), "{error:?} {language:?}");
                assert!(
                    !message.contains('\r') && !message.contains('\n'),
                    "{error:?} {language:?}"
                );
            }
        }
    }

    #[test]
    fn errors_use_representative_local_scripts_without_dynamic_details() {
        assert_eq!(
            UsageError::CliNotFound.user_message(Language::Japanese),
            "Codex CLI が見つかりません。"
        );
        assert_eq!(
            UsageError::RpcOverloaded.user_message(Language::Arabic),
            "Codex مشغول. يرجى المحاولة مرة أخرى قريبًا."
        );
        assert_eq!(
            UsageError::AuthenticationExpired.user_message(Language::PortugueseBrazil),
            "A autenticação do Codex expirou."
        );
    }
}
