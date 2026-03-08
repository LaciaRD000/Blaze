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
        let mut font_db = usvg::fontdb::Database::new();
        load_fonts(&mut font_db);

        Self {
            syntax_set,
            theme_set,
            font_db: Arc::new(font_db),
        }
    }

    /// コードを画像化する: highlight → SVG → PNG
    pub fn render(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
    ) -> Result<Vec<u8>, BlazeError> {
        let svg = self.build_svg_internal(code, language, theme_name)?;
        rasterize::rasterize(&svg, Arc::clone(&self.font_db))
    }

    /// SVG文字列のみを返す（スナップショットテスト用）
    pub fn render_svg(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
    ) -> Result<String, BlazeError> {
        self.build_svg_internal(code, language, theme_name)
    }

    /// ハイライト → SVG生成の共通処理
    fn build_svg_internal(
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

        let options = svg_builder::SvgOptions {
            bg_color: &bg_color,
            language,
            title_bar_style: "macos",
        };

        Ok(svg_builder::build_svg(&lines, &options))
    }
}

/// フォント読み込み（include_bytes! による静的埋め込み）
fn load_fonts(font_db: &mut usvg::fontdb::Database) {
    font_db.load_font_data(
        include_bytes!("../../assets/fonts/FiraCode-Regular.ttf").to_vec(),
    );
    font_db.load_font_data(
        include_bytes!("../../assets/fonts/PlemolJP-Regular.ttf").to_vec(),
    );
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
    fn font_db_contains_fira_code() {
        let renderer = Renderer::new();
        let has_fira = renderer.font_db.faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.contains("Fira Code"))
        });
        assert!(has_fira, "Fira Code フォントが登録されているべき");
    }

    #[test]
    fn font_db_contains_plemoljp() {
        let renderer = Renderer::new();
        let has_plemol = renderer.font_db.faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.contains("PlemolJP"))
        });
        assert!(has_plemol, "PlemolJP フォントが登録されているべき");
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
