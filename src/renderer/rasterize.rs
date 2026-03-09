use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use resvg::usvg;

use crate::error::BlazeError;
use crate::renderer::canvas::{self, CanvasOptions, FontSet};
use crate::renderer::highlight::HighlightedLine;

/// レンダリングスケール（高DPI対応）
const SCALE: f32 = 2.0;

/// Pixmap を高速圧縮で PNG エンコードする
/// Discord は画像アップロード時に再圧縮するため、Bot 側で高圧縮する CPU コストは無駄になる
fn encode_png_fast(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, BlazeError> {
    let mut buf = Vec::new();
    let encoder = PngEncoder::new_with_quality(
        &mut buf,
        CompressionType::Fast,
        FilterType::Sub,
    );
    encoder
        .write_image(
            pixmap.data(),
            pixmap.width(),
            pixmap.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| {
            BlazeError::rendering(format!("PNGエンコード失敗: {e}"))
        })?;
    Ok(buf)
}

/// ドロップシャドウ定数
const SHADOW_MARGIN: f32 = 32.0;
const SHADOW_DY: f32 = 8.0;
const SHADOW_SIGMA: f32 = 16.0;
const SHADOW_OPACITY: u8 = 102; // floor(0.4 * 255)

/// ドロップシャドウの Pixmap を1xサイズで生成する
/// SVG の feDropShadow を tiny_skia で直接実現し、resvg 内部のフィルタ処理を回避
/// 1xで描画+ぼかしを行い、合成時に2xスケールすることでぼかし計算量を1/4に削減
fn create_shadow_pixmap(
    svg_width: f32,
    svg_height: f32,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    let w = svg_width as u32;
    let h = svg_height as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| BlazeError::rendering("シャドウPixmap作成に失敗"))?;

    // ウィンドウ形状の矩形を半透明黒で描画
    // blur sigma=16 がコーナーを十分に丸めるため、角丸パスは不要
    let rect = tiny_skia::Rect::from_xywh(
        SHADOW_MARGIN,
        SHADOW_MARGIN + SHADOW_DY,
        svg_width - SHADOW_MARGIN * 2.0,
        svg_height - SHADOW_MARGIN * 2.0,
    )
    .ok_or_else(|| BlazeError::rendering("シャドウRect作成に失敗"))?;

    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(0, 0, 0, SHADOW_OPACITY);
    pixmap.fill_rect(
        rect,
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );

    // ダウンスケール → ぼかし のみ（upscale は draw_pixmap のスケール変換に委ねる）
    // sigma=16 は十分広いため、1/4 スケールでも品質劣化が見えない
    let rgba = pixmap_to_rgba(&pixmap);
    let quarter_w = (w / 4).max(1);
    let quarter_h = (h / 4).max(1);
    let downscaled = image::imageops::resize(
        &rgba,
        quarter_w,
        quarter_h,
        image::imageops::FilterType::Triangle,
    );
    let blurred =
        image::imageops::blur(&downscaled, SHADOW_SIGMA / 4.0);
    super::background::rgba_to_pixmap(&blurred)
}

/// シャドウの描画スケール（1/4 ダウンスケール × 2x レンダリングスケール = 8x）
const SHADOW_DRAW_SCALE: f32 = SCALE * 4.0;

/// シャドウ Pixmap のサイズ別キャッシュ
/// シャドウは (svg_width, svg_height) にのみ依存し、幅は常に 864px、
/// 高さは行数+タイトルバースタイルで決まるため、パターン数は高々 ~50
pub struct ShadowCache {
    cache: Mutex<HashMap<(u32, u32), tiny_skia::Pixmap>>,
}

impl Default for ShadowCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ShadowCache {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// キャッシュからシャドウを取得、なければ生成してキャッシュする
    pub fn get_or_create(
        &self,
        svg_width: f32,
        svg_height: f32,
    ) -> Result<tiny_skia::Pixmap, BlazeError> {
        let key = (svg_width as u32, svg_height as u32);

        // 読み取りチェック
        {
            let cache = self.cache.lock().expect("ShadowCache lock");
            if let Some(cached) = cache.get(&key) {
                return Ok(cached.clone());
            }
        }

        // キャッシュミス: 生成してから格納
        let shadow = create_shadow_pixmap(svg_width, svg_height)?;
        {
            let mut cache = self.cache.lock().expect("ShadowCache lock");
            cache.insert(key, shadow.clone());
        }
        Ok(shadow)
    }
}

/// SVG文字列をPNGバイト列に変換する（背景なし）
pub fn rasterize(
    svg: &str,
    font_db: Arc<resvg::usvg::fontdb::Database>,
) -> Result<Vec<u8>, BlazeError> {
    let options = usvg::Options {
        fontdb: font_db,
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|e| BlazeError::rendering(format!("SVGパース失敗: {e}")))?;

    let size = tree.size();
    let width = (size.width() * SCALE) as u32;
    let height = (size.height() * SCALE) as u32;

    // シャドウを先に描画し、その上に resvg で直接レンダリング（Pixmap 確保 + draw を1回削減）
    let shadow = create_shadow_pixmap(size.width(), size.height())?;
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("Pixmap の作成に失敗"))?;

    final_pixmap.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(SHADOW_DRAW_SCALE, SHADOW_DRAW_SCALE),
        None,
    );

    // resvg は source-over 合成でシャドウの上にコードを描画
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut final_pixmap.as_mut(),
    );

    encode_png_fast(&final_pixmap)
}

/// tiny_skia::Pixmap (premultiplied alpha) → image::RgbaImage (straight alpha) に変換
fn pixmap_to_rgba(pixmap: &tiny_skia::Pixmap) -> image::RgbaImage {
    let w = pixmap.width();
    let h = pixmap.height();
    let src = pixmap.data();
    let mut buf = vec![0u8; src.len()];
    for i in (0..src.len()).step_by(4) {
        let a = src[i + 3];
        if a == 0 {
            continue;
        }
        let inv_alpha = 255.0 / a as f32;
        buf[i] = (src[i] as f32 * inv_alpha).min(255.0) as u8;
        buf[i + 1] = (src[i + 1] as f32 * inv_alpha).min(255.0) as u8;
        buf[i + 2] = (src[i + 2] as f32 * inv_alpha).min(255.0) as u8;
        buf[i + 3] = a;
    }
    image::RgbaImage::from_raw(w, h, buf).expect("RgbaImage 変換に失敗")
}

/// 背景 Pixmap にガウスぼかしを適用する
/// ダウンスケール → ぼかし のみ行い、元サイズへの復元は draw_pixmap のスケール変換に委ねる
/// 戻り値の (Pixmap, f32) は (ぼかし済みPixmap, 元サイズに戻すスケール倍率)
fn blur_pixmap(
    bg: tiny_skia::Pixmap,
    blur_radius: f64,
) -> Result<(tiny_skia::Pixmap, f32), BlazeError> {
    if blur_radius <= 0.0 {
        return Ok((bg, 1.0));
    }

    // Premultiplied → Straight alpha
    let rgba = pixmap_to_rgba(&bg);

    // 1/2 にダウンスケールしてぼかし計算を軽量化
    let half_w = (bg.width() / 2).max(1);
    let half_h = (bg.height() / 2).max(1);
    let downscaled = image::imageops::resize(
        &rgba,
        half_w,
        half_h,
        image::imageops::FilterType::Triangle,
    );

    // ダウンスケール分だけ blur_radius も半分に
    let blurred =
        image::imageops::blur(&downscaled, (blur_radius / 2.0) as f32);

    // upscale せず、スケール倍率 2.0 を返す（合成時に draw_pixmap で拡大）
    let pixmap = super::background::rgba_to_pixmap(&blurred)?;
    Ok((pixmap, 2.0))
}

/// 背景 Pixmap 付きでコード SVG をラスタライズする
/// 背景にぼかしを適用 → 2xスケール → コード SVG を上に合成
pub fn rasterize_with_background(
    svg: &str,
    font_db: Arc<resvg::usvg::fontdb::Database>,
    bg_pixmap: tiny_skia::Pixmap,
    blur_radius: f64,
    blur_margin: u32,
) -> Result<Vec<u8>, BlazeError> {
    // 1. SVG パース
    let options = usvg::Options {
        fontdb: font_db,
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|e| BlazeError::rendering(format!("SVGパース失敗: {e}")))?;

    let size = tree.size();
    let width = (size.width() * SCALE) as u32;
    let height = (size.height() * SCALE) as u32;

    // 2. 背景ぼかしとシャドウ生成を並列実行（互いに独立した処理）
    let (blur_result, shadow_result) = std::thread::scope(|s| {
        let shadow_handle =
            s.spawn(|| create_shadow_pixmap(size.width(), size.height()));
        let blur_result = blur_pixmap(bg_pixmap, blur_radius);
        let shadow_result = shadow_handle.join().expect("シャドウスレッドがパニック");
        (blur_result, shadow_result)
    });
    let (blurred_bg, blur_scale) = blur_result?;
    let shadow = shadow_result?;

    // 4. 最終キャンバスに合成: 背景 → シャドウ → resvg 直接描画
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("最終Pixmap作成に失敗"))?;

    // 背景を描画（blur_scale × SCALE で拡大、blur_margin 分オフセット）
    let bg_draw_scale = SCALE * blur_scale;
    let offset = -(blur_margin as f32) * SCALE;
    final_pixmap.draw_pixmap(
        0,
        0,
        blurred_bg.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(bg_draw_scale, bg_draw_scale)
            .pre_translate(offset / bg_draw_scale, offset / bg_draw_scale),
        None,
    );

    // シャドウを描画
    final_pixmap.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(SHADOW_DRAW_SCALE, SHADOW_DRAW_SCALE),
        None,
    );

    // resvg で直接 final_pixmap に描画（code_pixmap の確保 + draw を省略）
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut final_pixmap.as_mut(),
    );

    encode_png_fast(&final_pixmap)
}

/// SVG パイプラインを完全に排除した直接描画パス（背景なし）
/// usvg パース + resvg レンダリングの代わりに fontdue + tiny_skia で直接描画
pub fn rasterize_direct(
    lines: &[HighlightedLine],
    font_set: &FontSet,
    canvas_options: &CanvasOptions,
    shadow_cache: &ShadowCache,
) -> Result<Vec<u8>, BlazeError> {
    let (svg_w, svg_h) = canvas::calculate_dimensions(
        lines.len(),
        canvas_options.title_bar_style,
    );

    // シャドウ（キャッシュ済み）→ コード描画の順
    let shadow = shadow_cache.get_or_create(svg_w, svg_h)?;
    let code_pixmap =
        canvas::render_code_pixmap(lines, font_set, canvas_options, SCALE)?;

    // final_pixmap にシャドウ → コードの順で描画
    let width = code_pixmap.width();
    let height = code_pixmap.height();
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("Pixmap作成に失敗"))?;

    final_pixmap.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(
            SHADOW_DRAW_SCALE,
            SHADOW_DRAW_SCALE,
        ),
        None,
    );

    final_pixmap.draw_pixmap(
        0,
        0,
        code_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    encode_png_fast(&final_pixmap)
}

/// SVG パイプラインを完全に排除した直接描画パス（背景あり）
pub fn rasterize_direct_with_background(
    lines: &[HighlightedLine],
    font_set: &FontSet,
    canvas_options: &CanvasOptions,
    shadow_cache: &ShadowCache,
    bg_pixmap: tiny_skia::Pixmap,
    blur_radius: f64,
    blur_margin: u32,
) -> Result<Vec<u8>, BlazeError> {
    let (svg_w, svg_h) = canvas::calculate_dimensions(
        lines.len(),
        canvas_options.title_bar_style,
    );

    // シャドウはキャッシュから取得（高速）、背景ぼかし+コード描画は並列実行
    let shadow = shadow_cache.get_or_create(svg_w, svg_h)?;
    let (blur_result, code_pixmap) = std::thread::scope(|s| {
        let code_handle = s.spawn(|| {
            canvas::render_code_pixmap(
                lines,
                font_set,
                canvas_options,
                SCALE,
            )
        });
        let blur_result = blur_pixmap(bg_pixmap, blur_radius);
        let code_result =
            code_handle.join().expect("コード描画スレッドがパニック");
        (blur_result, code_result)
    });

    let (blurred_bg, blur_scale) = blur_result?;
    let code_pixmap = code_pixmap?;

    let width = code_pixmap.width();
    let height = code_pixmap.height();
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("最終Pixmap作成に失敗"))?;

    // 背景 → シャドウ → コードの順で合成
    let bg_draw_scale = SCALE * blur_scale;
    let offset = -(blur_margin as f32) * SCALE;
    final_pixmap.draw_pixmap(
        0,
        0,
        blurred_bg.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(bg_draw_scale, bg_draw_scale)
            .pre_translate(
                offset / bg_draw_scale,
                offset / bg_draw_scale,
            ),
        None,
    );

    final_pixmap.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(
            SHADOW_DRAW_SCALE,
            SHADOW_DRAW_SCALE,
        ),
        None,
    );

    final_pixmap.draw_pixmap(
        0,
        0,
        code_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    encode_png_fast(&final_pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_svg() -> String {
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <rect width="200" height="100" fill="#1e1e2e"/>
            <text x="10" y="30" font-size="14" fill="#ffffff">Hello</text>
        </svg>"##
            .to_string()
    }

    fn empty_font_db() -> Arc<resvg::usvg::fontdb::Database> {
        Arc::new(resvg::usvg::fontdb::Database::new())
    }

    #[test]
    fn encode_png_fast_produces_valid_png() {
        let pixmap = tiny_skia::Pixmap::new(100, 50)
            .expect("Pixmap作成に成功するべき");
        let png = encode_png_fast(&pixmap)
            .expect("PNGエンコードに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn rasterize_valid_svg_returns_png_bytes() {
        let svg = minimal_svg();
        let db = empty_font_db();
        let png = rasterize(&svg, db).expect("ラスタライズに成功するべき");
        assert!(!png.is_empty());
        // PNG マジックバイト
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn rasterize_invalid_svg_returns_error() {
        let db = empty_font_db();
        let result = rasterize("not valid svg", db);
        assert!(result.is_err());
    }

    #[test]
    fn rasterize_with_background_produces_png() {
        let svg = minimal_svg();
        let db = empty_font_db();

        // 小さなグラデーション背景
        let bg = super::super::background::generate_gradient_pixmap(224, 124)
            .expect("背景Pixmap生成に成功するべき");

        let png = rasterize_with_background(&svg, db, bg, 8.0, 12)
            .expect("背景合成ラスタライズに成功するべき");
        assert!(!png.is_empty());
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn blur_pixmap_modifies_image() {
        let bg = super::super::background::generate_gradient_pixmap(100, 100)
            .expect("背景Pixmap生成に成功するべき");
        let original_data = bg.data().to_vec();
        let (blurred, _scale) =
            blur_pixmap(bg, 8.0).expect("ぼかしに成功するべき");
        // ダウンスケールされるためデータ長が異なるが、ぼかしが適用されていることを確認
        assert!(
            !blurred.data().iter().all(|&b| b == 0),
            "ぼかし後のピクセルデータは空でないべき"
        );
        // 元データとサイズが異なるので直接比較はしない
        let _ = original_data;
    }

    #[test]
    fn blur_pixmap_zero_radius_returns_unchanged() {
        let bg = super::super::background::generate_gradient_pixmap(100, 100)
            .expect("背景Pixmap生成に成功するべき");
        let original_data = bg.data().to_vec();
        let (result, scale) =
            blur_pixmap(bg, 0.0).expect("ぼかしに成功するべき");
        assert_eq!(
            original_data,
            result.data(),
            "ぼかし強度0はピクセルデータを変更しないべき"
        );
        assert_eq!(scale, 1.0, "ぼかし強度0はスケール1.0を返すべき");
    }

    #[test]
    fn blur_pixmap_returns_downscaled() {
        let bg = super::super::background::generate_gradient_pixmap(200, 150)
            .expect("背景Pixmap生成に成功するべき");
        let (blurred, scale) =
            blur_pixmap(bg, 5.0).expect("ぼかしに成功するべき");
        // 1/2 にダウンスケールされ、スケール倍率 2.0 が返る
        assert_eq!(blurred.width(), 100);
        assert_eq!(blurred.height(), 75);
        assert_eq!(scale, 2.0, "ダウンスケール倍率は2.0であるべき");
    }

    #[test]
    fn create_shadow_pixmap_produces_downscaled_pixmap() {
        let shadow = create_shadow_pixmap(864.0, 192.0)
            .expect("シャドウPixmap生成に成功するべき");
        // 1/4 ダウンスケールされた Pixmap が返る
        assert_eq!(shadow.width(), 216);
        assert_eq!(shadow.height(), 48);
    }

    #[test]
    fn create_shadow_pixmap_has_opaque_center() {
        let shadow = create_shadow_pixmap(864.0, 192.0)
            .expect("シャドウPixmap生成に成功するべき");
        let cx = shadow.width() / 2;
        let cy = shadow.height() / 2;
        let idx = ((cy * shadow.width() + cx) * 4) as usize;
        let alpha = shadow.data()[idx + 3];
        assert!(
            alpha > 0,
            "シャドウの中央は不透明であるべき: alpha={alpha}"
        );
    }

    #[test]
    fn rasterize_with_background_not_all_transparent() {
        let svg = minimal_svg();
        let db = empty_font_db();
        let bg = super::super::background::generate_gradient_pixmap(224, 124)
            .expect("背景Pixmap生成に成功するべき");

        let png = rasterize_with_background(&svg, db, bg, 8.0, 12)
            .expect("背景合成ラスタライズに成功するべき");
        let pixmap = tiny_skia::Pixmap::decode_png(&png)
            .expect("PNGデコードに成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque, "合成結果は透明でないピクセルを含むべき");
    }

    // --- 直接描画パスのテスト ---

    use crate::renderer::highlight::{Color, HighlightedLine, StyledToken};

    fn sample_lines() -> Vec<HighlightedLine> {
        vec![HighlightedLine {
            tokens: vec![StyledToken {
                text: "fn main() {}".to_string(),
                color: Color { r: 205, g: 214, b: 244, a: 255 },
                bold: false,
                italic: false,
            }],
        }]
    }

    fn test_font_set() -> FontSet {
        FontSet::new()
    }

    fn test_canvas_options() -> CanvasOptions<'static> {
        CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "macos",
            language: Some("rust"),
            max_line_length: None,
            show_line_numbers: false,
        }
    }

    #[test]
    fn rasterize_direct_produces_png() {
        let font_set = test_font_set();
        let lines = sample_lines();
        let opts = test_canvas_options();
        let cache = ShadowCache::new();
        let png = rasterize_direct(&lines, &font_set, &opts, &cache)
            .expect("直接描画に成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn rasterize_direct_with_background_produces_png() {
        let font_set = test_font_set();
        let lines = sample_lines();
        let opts = test_canvas_options();
        let cache = ShadowCache::new();
        let bg =
            super::super::background::generate_gradient_pixmap(224, 124)
                .expect("背景Pixmap生成に成功するべき");
        let png = rasterize_direct_with_background(
            &lines, &font_set, &opts, &cache, bg, 8.0, 12,
        )
        .expect("背景付き直接描画に成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn rasterize_direct_not_all_transparent() {
        let font_set = test_font_set();
        let lines = sample_lines();
        let opts = test_canvas_options();
        let cache = ShadowCache::new();
        let png = rasterize_direct(&lines, &font_set, &opts, &cache)
            .expect("直接描画に成功するべき");
        let pixmap = tiny_skia::Pixmap::decode_png(&png)
            .expect("PNGデコードに成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque, "直接描画結果は透明でないピクセルを含むべき");
    }

    #[test]
    fn shadow_cache_returns_same_result() {
        let cache = ShadowCache::new();
        let s1 = cache
            .get_or_create(864.0, 192.0)
            .expect("シャドウ生成に成功するべき");
        let s2 = cache
            .get_or_create(864.0, 192.0)
            .expect("キャッシュ取得に成功するべき");
        assert_eq!(s1.width(), s2.width());
        assert_eq!(s1.height(), s2.height());
        assert_eq!(s1.data(), s2.data(), "キャッシュされたシャドウは同一であるべき");
    }

    #[test]
    fn shadow_cache_different_sizes_are_independent() {
        let cache = ShadowCache::new();
        let s1 = cache
            .get_or_create(864.0, 192.0)
            .expect("シャドウ生成に成功するべき");
        let s2 = cache
            .get_or_create(864.0, 292.0)
            .expect("別サイズのシャドウ生成に成功するべき");
        assert_ne!(
            s1.height(),
            s2.height(),
            "異なるサイズのシャドウは異なる高さであるべき"
        );
    }
}
