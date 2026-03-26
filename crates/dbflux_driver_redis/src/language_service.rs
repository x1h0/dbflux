use dbflux_core::{DangerousQueryKind, LanguageService, ValidationResult, detect_dangerous_redis};

/// Redis language service with lightweight syntax/language checks.
pub struct RedisLanguageService;

impl LanguageService for RedisLanguageService {
    fn validate(&self, _query: &str) -> ValidationResult {
        ValidationResult::Valid
    }

    fn detect_dangerous(&self, query: &str) -> Option<DangerousQueryKind> {
        detect_dangerous_redis(query)
    }
}
