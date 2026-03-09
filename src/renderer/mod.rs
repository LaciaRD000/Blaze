use std::sync::Arc;

use resvg::usvg;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// ビルド時にダンプした SyntaxSet バイナリ
static SYNTAX_SET_DUMP: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/syntax_set.packdump"));
/// ビルド時にダンプした ThemeSet バイナリ
static THEME_SET_DUMP: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/theme_set.packdump"));

use crate::error::BlazeError;

pub mod background;
pub mod canvas;
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
    /// 背景画像ID。None で背景なし
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

/// SVG サイズの定数（svg_builder と同じ値）
const WINDOW_WIDTH: u32 = 800;
const SHADOW_MARGIN: u32 = 32;
const TITLE_BAR_HEIGHT: u32 = 36;
const PADDING_Y: u32 = 16;
const LINE_HEIGHT: u32 = 20;

/// レンダリングパイプラインを統括する構造体
/// Arc で共有し、複数リクエストで使い回す（読み取り専用、ロック不要）
pub struct Renderer {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub font_db: Arc<usvg::fontdb::Database>,
    pub font_set: canvas::FontSet,
    pub shadow_cache: rasterize::ShadowCache,
    pub background_cache: background::BackgroundCache,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        // ビルド時にダンプした uncompressed バイナリからデシリアライズ
        // デフォルトの load_defaults_newlines() より高速（圧縮展開をスキップ）
        let syntax_set: SyntaxSet =
            syntect::dumps::from_uncompressed_data(SYNTAX_SET_DUMP)
                .expect("SyntaxSet のデシリアライズに失敗");
        let theme_set: ThemeSet =
            syntect::dumps::from_uncompressed_data(THEME_SET_DUMP)
                .expect("ThemeSet のデシリアライズに失敗");
        let mut font_db = usvg::fontdb::Database::new();
        load_fonts(&mut font_db);
        let font_set = canvas::FontSet::new();
        let shadow_cache = rasterize::ShadowCache::new();
        let background_cache = background::BackgroundCache::new();

        Self {
            syntax_set,
            theme_set,
            font_db: Arc::new(font_db),
            font_set,
            shadow_cache,
            background_cache,
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

    /// コードを画像化する: highlight → 直接描画 → PNG（オプション指定）
    /// SVG パイプライン (usvg/resvg) を完全に排除し、fontdue + tiny_skia で直接描画
    pub fn render_with_options(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
        options: &RenderOptions,
    ) -> Result<Vec<u8>, BlazeError> {
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

        let lines =
            highlight::highlight(code, language, &self.syntax_set, theme);

        let canvas_options = canvas::CanvasOptions {
            bg_color: [bg.r, bg.g, bg.b],
            opacity: options.opacity as f32,
            title_bar_style: &options.title_bar_style,
            language,
            max_line_length: options.max_line_length,
            show_line_numbers: options.show_line_numbers,
        };

        if let Some(bg_id) = &options.background_image {
            let (total_w, total_h) =
                Self::estimate_svg_size(code, options);
            let blur_margin =
                (options.blur_radius * 3.0).ceil() as u32;
            let img_w = total_w + blur_margin * 2;
            let img_h = total_h + blur_margin * 2;

            let bg_pixmap = background::load_background_pixmap(
                &self.background_cache,
                bg_id,
                img_w,
                img_h,
            )?;

            rasterize::rasterize_direct_with_background(
                &lines,
                &self.font_set,
                &canvas_options,
                &self.shadow_cache,
                bg_pixmap,
                options.blur_radius,
                blur_margin,
            )
        } else {
            rasterize::rasterize_direct(
                &lines,
                &self.font_set,
                &canvas_options,
                &self.shadow_cache,
            )
        }
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
    /// 注意: 背景画像は SVG に含まれない（rasterize 側で合成される）
    pub fn render_svg_with_options(
        &self,
        code: &str,
        language: Option<&str>,
        theme_name: &str,
        options: &RenderOptions,
    ) -> Result<String, BlazeError> {
        self.build_svg_internal(code, language, theme_name, options)
    }

    /// SVG のピクセルサイズを推定する（svg_builder と同じ計算）
    fn estimate_svg_size(code: &str, options: &RenderOptions) -> (u32, u32) {
        let line_count = code.lines().count().max(1) as u32;
        let title_bar_h = match options.title_bar_style.as_str() {
            "macos" | "linux" | "plain" => TITLE_BAR_HEIGHT,
            _ => 0,
        };
        let window_h = title_bar_h + PADDING_Y * 2 + LINE_HEIGHT * line_count;
        let total_w = WINDOW_WIDTH + SHADOW_MARGIN * 2;
        let total_h = window_h + SHADOW_MARGIN * 2;
        (total_w, total_h)
    }

    /// ハイライト → SVG生成の共通処理（背景画像は含めない）
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

        let svg_options = svg_builder::SvgOptions {
            bg_color: &bg_color,
            language,
            title_bar_style: &render_options.title_bar_style,
            opacity: render_options.opacity,
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
    font_db.load_font_data(
        include_bytes!("../../assets/fonts/HackGenConsoleNF-Regular.ttf").to_vec(),
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
    fn font_db_contains_hackgen_nf() {
        let renderer = Renderer::new();
        let has_hackgen = renderer.font_db.faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.contains("HackGen Console NF"))
        });
        assert!(has_hackgen, "HackGen Console NF フォントが登録されているべき");
    }

    #[test]
    fn render_invalid_theme_uses_fallback() {
        let renderer = Renderer::new();
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
    fn render_with_background_gradient_produces_png() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            background_image: Some("gradient".to_string()),
            ..Default::default()
        };
        let png = renderer
            .render_with_options("test", None, "base16-ocean.dark", &opts)
            .expect("背景付きレンダリングに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn render_with_background_denim_produces_png() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            background_image: Some("denim".to_string()),
            ..Default::default()
        };
        let png = renderer
            .render_with_options("test", None, "base16-ocean.dark", &opts)
            .expect("denim背景付きレンダリングに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn render_with_background_repeated_square_dark_produces_png() {
        let renderer = Renderer::new();
        let opts = RenderOptions {
            background_image: Some("repeated-square-dark".to_string()),
            ..Default::default()
        };
        let png = renderer
            .render_with_options("test", None, "base16-ocean.dark", &opts)
            .expect("repeated-square-dark背景付きレンダリングに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }
}
