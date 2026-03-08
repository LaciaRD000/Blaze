use serde::Deserialize;

use crate::error::BlazeError;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub max_code_lines: usize,
    pub max_code_chars: usize,
    pub max_line_length: usize,
    pub rate_limit_per_minute: u32,
    pub max_concurrent_renders: usize,
    pub log_level: String,
}

impl Settings {
    /// 設定値の範囲を検証する。Bot起動時に呼び出し、不正値なら即座にパニックさせる
    pub fn validate(&self) -> Result<(), BlazeError> {
        if self.max_code_lines == 0 || self.max_code_lines > 500 {
            return Err(BlazeError::Config(format!(
                "max_code_lines は 1〜500 の範囲: {}",
                self.max_code_lines
            )));
        }
        if self.max_code_chars == 0 || self.max_code_chars > 20_000 {
            return Err(BlazeError::Config(format!(
                "max_code_chars は 1〜20000 の範囲: {}",
                self.max_code_chars
            )));
        }
        if self.rate_limit_per_minute == 0 || self.rate_limit_per_minute > 120 {
            return Err(BlazeError::Config(format!(
                "rate_limit_per_minute は 1〜120 の範囲: {}",
                self.rate_limit_per_minute
            )));
        }
        if self.max_line_length == 0 || self.max_line_length > 500 {
            return Err(BlazeError::Config(format!(
                "max_line_length は 1〜500 の範囲: {}",
                self.max_line_length
            )));
        }
        if self.max_concurrent_renders == 0 || self.max_concurrent_renders > 32
        {
            return Err(BlazeError::Config(format!(
                "max_concurrent_renders は 1〜32 の範囲: {}",
                self.max_concurrent_renders
            )));
        }
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.log_level.as_str()) {
            return Err(BlazeError::Config(format!(
                "log_level は {valid_levels:?} のいずれか: {}",
                self.log_level
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_settings() -> Settings {
        toml::from_str(include_str!("../config/default.toml"))
            .expect("default.toml のパースに失敗")
    }

    #[test]
    fn default_toml_parses_and_validates() {
        let settings = default_settings();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn max_code_lines_zero_is_rejected() {
        let mut settings = default_settings();
        settings.max_code_lines = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_code_lines_over_500_is_rejected() {
        let mut settings = default_settings();
        settings.max_code_lines = 501;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_code_chars_zero_is_rejected() {
        let mut settings = default_settings();
        settings.max_code_chars = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_code_chars_over_20000_is_rejected() {
        let mut settings = default_settings();
        settings.max_code_chars = 20_001;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn rate_limit_zero_is_rejected() {
        let mut settings = default_settings();
        settings.rate_limit_per_minute = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn rate_limit_over_120_is_rejected() {
        let mut settings = default_settings();
        settings.rate_limit_per_minute = 121;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_line_length_zero_is_rejected() {
        let mut settings = default_settings();
        settings.max_line_length = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_line_length_over_500_is_rejected() {
        let mut settings = default_settings();
        settings.max_line_length = 501;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_concurrent_renders_zero_is_rejected() {
        let mut settings = default_settings();
        settings.max_concurrent_renders = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn max_concurrent_renders_over_32_is_rejected() {
        let mut settings = default_settings();
        settings.max_concurrent_renders = 33;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn invalid_log_level_is_rejected() {
        let mut settings = default_settings();
        settings.log_level = "verbose".to_string();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn valid_log_levels_are_accepted() {
        for level in ["trace", "debug", "info", "warn", "error"] {
            let mut settings = default_settings();
            settings.log_level = level.to_string();
            assert!(settings.validate().is_ok(), "{level} は有効なログレベル");
        }
    }
}
