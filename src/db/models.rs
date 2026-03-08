use sqlx::FromRow;

/// ユーザーごとのテーマ設定
#[derive(Debug, Clone, FromRow)]
pub struct UserTheme {
    pub user_id: i64,
    pub color_scheme: String,
    pub background_id: String,
    pub blur_radius: f64,
    pub opacity: f64,
    pub font_family: String,
    pub font_size: f64,
    pub title_bar_style: String,
    pub show_line_numbers: i32,
    pub updated_at: String,
}

impl UserTheme {
    /// デフォルト設定で新しい UserTheme を作成する
    pub fn with_defaults(user_id: i64) -> Self {
        Self {
            user_id,
            color_scheme: "base16-ocean.dark".to_string(),
            background_id: "default".to_string(),
            blur_radius: 8.0,
            opacity: 0.75,
            font_family: "Fira Code".to_string(),
            font_size: 14.0,
            title_bar_style: "macos".to_string(),
            show_line_numbers: 0,
            updated_at: String::new(),
        }
    }
}
