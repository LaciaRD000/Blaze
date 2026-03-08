use std::sync::Arc;

use resvg::usvg;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::error::BlazeError;

pub mod background;
pub mod highlight;
pub mod rasterize;
pub mod svg_builder;

/// レンダリング時のユーザー設定オプション
pub struct RenderOptions {
    pub title_bar_style: String,
    pub opacity: f64,
    pub blur_radius: f64,
    pub show_line_numbers: bool,
    /// 1行あたりの最大文字数。超過分は `…` でトリミング。None で無制限
    pub max_line_length: Option<usize>,
    /// 背景画像ID。"default" でデフォルトグラデーション背景を使用
    pub background_image: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            title_bar_style: "macos".to_string(),
            opacity: 0.75,
            blur_radius: 8.0,
            show_line_numbers: false,
            max_line_length: None,
            background_image: None,
        }
    }
}

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

    /// コードを画像化する: highlight → SVG → PNG（デフォルトオプション）
    pub fn render(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
    ) -> Result<Vec<u8>, BlazeError> {
        self.render_with_options(
            code,
            language,
            theme_name,
            &RenderOptions::default(),
        )
    }

    /// コードを画像化する: highlight → SVG → PNG（オプション指定）
    pub fn render_with_options(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
        options: &RenderOptions,
    ) -> Result<Vec<u8>, BlazeError> {
        let svg =
            self.build_svg_internal(code, language, theme_name, options)?;
        rasterize::rasterize(&svg, Arc::clone(&self.font_db))
    }

    /// SVG文字列のみを返す（スナップショットテスト用、デフォルトオプション）
    pub fn render_svg(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
    ) -> Result<String, BlazeError> {
        self.build_svg_internal(
            code,
            language,
            theme_name,
            &RenderOptions::default(),
        )
    }

    /// SVG文字列のみを返す（オプション指定）
    pub fn render_svg_with_options(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
        options: &RenderOptions,
    ) -> Result<String, BlazeError> {
        self.build_svg_internal(code, language, theme_name, options)
    }

    /// ハイライト → SVG生成の共通処理
    fn build_svg_internal(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
        render_options: &RenderOptions,
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

        // 背景画像の生成（background_id が指定されている場合）
        let bg_image_base64 = if render_options.background_image.is_some() {
            // SVG の総サイズを推定して背景画像を生成
            let window_width = 800u32;
            let shadow_margin = 32u32;
            let total_width = window_width + shadow_margin * 2;
            let total_height =
                shadow_margin * 2 + 36 + 32 + 20 * lines.len() as u32;
            let png = background::generate_default_background(
                total_width,
                total_height,
            );
            Some(background::resize_to_base64(
                &png,
                total_width,
                total_height,
            )?)
        } else {
            None
        };

        let svg_options = svg_builder::SvgOptions {
            bg_color: &bg_color,
            language,
            title_bar_style: &render_options.title_bar_style,
            opacity: render_options.opacity as f32,
            background_image: bg_image_base64.as_deref(),
            blur_radius: render_options.blur_radius as f32,
            max_line_length: render_options.max_line_length,
            show_line_numbers: render_options.show_line_numbers,
        };

        Ok(svg_builder::build_svg(&lines, &svg_options))
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

    #[test]
    fn render_with_options_applies_title_bar_style() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            title_bar_style: "linux".to_string(),
            ..Default::default()
        };
        let svg = renderer
            .render_svg_with_options(
                "fn main() {}",
                Some("rust"),
                "base16-ocean.dark",
                &opts,
            )
            .expect("SVG生成に成功するべき");
        assert!(
            svg.contains("class=\"title-button-close\""),
            "Linux タイトルバーが適用されるべき"
        );
        assert!(
            !svg.contains(r##"fill="#ff5f57""##),
            "macOS の赤丸が含まれないべき"
        );
    }

    #[test]
    fn render_with_options_applies_opacity() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            opacity: 0.5,
            ..Default::default()
        };
        let svg = renderer
            .render_svg_with_options("test", None, "base16-ocean.dark", &opts)
            .expect("SVG生成に成功するべき");
        assert!(
            svg.contains("fill-opacity=\"0.5\""),
            "指定した opacity が適用されるべき"
        );
    }

    #[test]
    fn render_with_options_applies_line_numbers() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            show_line_numbers: true,
            ..Default::default()
        };
        let svg = renderer
            .render_svg_with_options(
                "line1\nline2",
                None,
                "base16-ocean.dark",
                &opts,
            )
            .expect("SVG生成に成功するべき");
        assert!(svg.contains(">1</text>"), "行番号が表示されるべき");
    }

    #[test]
    fn render_with_options_applies_max_line_length() {
        let renderer = Renderer::new();
        let long_line = "a".repeat(150);
        let opts = RenderOptions {
            max_line_length: Some(120),
            ..Default::default()
        };
        let svg = renderer
            .render_svg_with_options(
                &long_line,
                None,
                "base16-ocean.dark",
                &opts,
            )
            .expect("SVG生成に成功するべき");
        assert!(
            svg.contains("…"),
            "max_line_length で長い行がトリミングされるべき"
        );
    }

    #[test]
    fn render_with_options_applies_background_image() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            background_image: Some("default".to_string()),
            ..Default::default()
        };
        let svg = renderer
            .render_svg_with_options("test", None, "base16-ocean.dark", &opts)
            .expect("SVG生成に成功するべき");
        assert!(
            svg.contains("feGaussianBlur"),
            "背景画像ありの場合ガウスぼかしが含まれるべき"
        );
        assert!(
            svg.contains("<image"),
            "背景画像の image 要素が含まれるべき"
        );
    }
}
