use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlazeError {
    #[error(
        "メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした"
    )]
    CodeBlockNotFound,

    #[error("コードが長すぎます（上限: {max_lines}行 / {max_chars}文字）")]
    CodeTooLong { max_lines: usize, max_chars: usize },

    #[error("データベースエラー: {0}")]
    Database(#[from] sqlx::Error),

    #[error("レンダリングエラー: {message}")]
    Rendering {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("レート制限に達しました。しばらくお待ちください。")]
    RateLimitExceeded,

    #[error("無効なテーマ設定: {0}")]
    InvalidTheme(String),

    #[error("設定エラー: {0}")]
    Config(String),
}

impl From<syntect::Error> for BlazeError {
    fn from(e: syntect::Error) -> Self {
        BlazeError::Rendering {
            message: e.to_string(),
            source: Some(Box::new(e)),
        }
    }
}

impl From<poise::serenity_prelude::Error> for BlazeError {
    fn from(e: poise::serenity_prelude::Error) -> Self {
        BlazeError::Rendering {
            message: format!("Discord エラー: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl BlazeError {
    /// ソースエラーなしのレンダリングエラーを作成する
    pub fn rendering(message: impl Into<String>) -> Self {
        BlazeError::Rendering {
            message: message.into(),
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_block_not_found_display() {
        let err = BlazeError::CodeBlockNotFound;
        assert_eq!(
            err.to_string(),
            "メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした"
        );
    }

    #[test]
    fn code_too_long_display() {
        let err = BlazeError::CodeTooLong {
            max_lines: 100,
            max_chars: 4000,
        };
        assert_eq!(
            err.to_string(),
            "コードが長すぎます（上限: 100行 / 4000文字）"
        );
    }

    #[test]
    fn rate_limit_exceeded_display() {
        let err = BlazeError::RateLimitExceeded;
        assert_eq!(
            err.to_string(),
            "レート制限に達しました。しばらくお待ちください。"
        );
    }

    #[test]
    fn invalid_theme_display() {
        let err = BlazeError::InvalidTheme("unknown_theme".to_string());
        assert_eq!(err.to_string(), "無効なテーマ設定: unknown_theme");
    }

    #[test]
    fn config_error_display() {
        let err = BlazeError::Config("bad value".to_string());
        assert_eq!(err.to_string(), "設定エラー: bad value");
    }

    #[test]
    fn database_error_from_sqlx() {
        let sqlx_err = sqlx::Error::RowNotFound;
        let err = BlazeError::from(sqlx_err);
        assert!(matches!(err, BlazeError::Database(_)));
    }

    #[test]
    fn rendering_error_display() {
        let err = BlazeError::Rendering {
            message: "フォントが見つかりません".to_string(),
            source: None,
        };
        assert_eq!(
            err.to_string(),
            "レンダリングエラー: フォントが見つかりません"
        );
    }

    #[test]
    fn rendering_convenience_constructor() {
        let err = BlazeError::rendering("テスト");
        assert!(matches!(err, BlazeError::Rendering { source: None, .. }));
        assert_eq!(err.to_string(), "レンダリングエラー: テスト");
    }

    #[test]
    fn from_syntect_error() {
        let syntect_err = syntect::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "test",
        ));
        let err = BlazeError::from(syntect_err);
        match err {
            BlazeError::Rendering { source, .. } => {
                assert!(source.is_some());
            }
            _ => panic!("syntect::Error から Rendering への変換が期待される"),
        }
    }
}
