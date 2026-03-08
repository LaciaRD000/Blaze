use std::sync::Arc;

use resvg::usvg;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::error::BlazeError;

pub mod highlight;
pub mod rasterize;
pub mod svg_builder;

/// レンダリングパイプラインを統括する構造体
/// Arc で共有し、複数リクエストで使い回す（読み取り専用、ロック不要）
pub struct Renderer {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub font_db: Arc<usvg::fontdb::Database>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let font_db = Arc::new(usvg::fontdb::Database::new());

        Self {
            syntax_set,
            theme_set,
            font_db,
        }
    }

    /// コードを画像化する: highlight → SVG → PNG
    pub fn render(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
    ) -> Result<Vec<u8>, BlazeError> {
        // テーマ取得（見つからなければデフォルトにフォールバック）
        let theme = self
            .theme_set
            .themes
            .get(theme_name)
            .or_else(|| self.theme_set.themes.get("base16-ocean.dark"))
            .ok_or_else(|| {
                BlazeError::rendering("デフォルトテーマが見つかりません")
            })?;

        // テーマの背景色を取得
        let bg =
            theme
                .settings
                .background
                .unwrap_or(syntect::highlighting::Color {
                    r: 30,
                    g: 30,
                    b: 46,
                    a: 255,
                });
        let bg_color = format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b);

        // 1. ハイライト
        let lines =
            highlight::highlight(code, language, &self.syntax_set, theme);

        // 2. SVG生成
        let svg = svg_builder::build_svg(&lines, &bg_color);

        // 3. PNG変換
        rasterize::rasterize(&svg, Arc::clone(&self.font_db))
    }

    /// SVG文字列のみを返す（スナップショットテスト用）
    pub fn render_svg(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
    ) -> Result<String, BlazeError> {
        let theme = self
            .theme_set
            .themes
            .get(theme_name)
            .or_else(|| self.theme_set.themes.get("base16-ocean.dark"))
            .ok_or_else(|| {
                BlazeError::rendering("デフォルトテーマが見つかりません")
            })?;

        let bg =
            theme
                .settings
                .background
                .unwrap_or(syntect::highlighting::Color {
                    r: 30,
                    g: 30,
                    b: 46,
                    a: 255,
                });
        let bg_color = format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b);

        let lines =
            highlight::highlight(code, language, &self.syntax_set, theme);

        Ok(svg_builder::build_svg(&lines, &bg_color))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_new_succeeds() {
        let renderer = Renderer::new();
        assert!(renderer.syntax_set.find_syntax_by_token("rust").is_some());
        assert!(renderer.theme_set.themes.contains_key("base16-ocean.dark"));
    }

    #[test]
    fn render_rust_code_produces_png() {
        let renderer = Renderer::new();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let png = renderer
            .render(code, Some("rust"), "base16-ocean.dark")
            .expect("レンダリングに成功するべき");
        assert!(!png.is_empty());
        // PNG マジックバイト
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn render_unknown_language_produces_png() {
        let renderer = Renderer::new();
        let png = renderer
            .render("just text", None, "base16-ocean.dark")
            .expect("プレーンテキストでもレンダリングに成功するべき");
        assert!(!png.is_empty());
    }

    #[test]
    fn snapshot_rust_hello_world_svg() {
        let renderer = Renderer::new();
        let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let svg = renderer
            .render_svg(code, Some("rust"), "base16-ocean.dark")
            .expect("SVG生成に成功するべき");
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn render_invalid_theme_uses_fallback() {
        let renderer = Renderer::new();
        // 存在しないテーマでもエラーにならずフォールバックする
        let png = renderer
            .render("test", Some("rust"), "nonexistent-theme")
            .expect("フォールバックテーマでレンダリングに成功するべき");
        assert!(!png.is_empty());
    }
}
