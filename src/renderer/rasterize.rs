use std::sync::Arc;

use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use resvg::usvg;

use crate::error::BlazeError;

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

    // ガウスぼかしを適用してシャドウを拡散
    let rgba = pixmap_to_rgba(&pixmap);
    let blurred = image::imageops::blur(&rgba, SHADOW_SIGMA);
    super::background::rgba_to_pixmap(&blurred)
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

    let mut code_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("Pixmap の作成に失敗"))?;

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut code_pixmap.as_mut(),
    );

    // シャドウを1xで生成し、2xスケールで合成
    let shadow = create_shadow_pixmap(size.width(), size.height())?;
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("最終Pixmap作成に失敗"))?;

    final_pixmap.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(SCALE, SCALE),
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
/// ダウンスケール → ぼかし → 元サイズに戻すことで計算量を約1/4に削減
/// ぼかし後は細部が消えるため、ダウンスケールによる品質劣化は視覚的に無視できる
fn blur_pixmap(
    bg: tiny_skia::Pixmap,
    blur_radius: f64,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    if blur_radius <= 0.0 {
        return Ok(bg);
    }

    let orig_w = bg.width();
    let orig_h = bg.height();

    // Premultiplied → Straight alpha
    let rgba = pixmap_to_rgba(&bg);

    // 1/2 にダウンスケールしてぼかし計算を軽量化
    let half_w = (orig_w / 2).max(1);
    let half_h = (orig_h / 2).max(1);
    let downscaled = image::imageops::resize(
        &rgba,
        half_w,
        half_h,
        image::imageops::FilterType::Triangle,
    );

    // ダウンスケール分だけ blur_radius も半分に
    let blurred =
        image::imageops::blur(&downscaled, (blur_radius / 2.0) as f32);

    // 元のサイズに戻す
    let upscaled = image::imageops::resize(
        &blurred,
        orig_w,
        orig_h,
        image::imageops::FilterType::Triangle,
    );

    // Straight → Premultiplied alpha
    super::background::rgba_to_pixmap(&upscaled)
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
    // 1. コード SVG をラスタライズ（透明背景）
    let options = usvg::Options {
        fontdb: font_db,
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|e| BlazeError::rendering(format!("SVGパース失敗: {e}")))?;

    let size = tree.size();
    let width = (size.width() * SCALE) as u32;
    let height = (size.height() * SCALE) as u32;

    let mut code_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("コードPixmap作成に失敗"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut code_pixmap.as_mut(),
    );

    // 2. 背景にぼかしを適用（1xサイズで処理、軽量）
    let blurred_bg = blur_pixmap(bg_pixmap, blur_radius)?;

    // 3. シャドウを1xで生成
    let shadow = create_shadow_pixmap(size.width(), size.height())?;

    // 4. 最終キャンバスに合成: ぼかし背景 → シャドウ → コード
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("最終Pixmap作成に失敗"))?;

    // 背景を2xスケールで描画（blur_margin 分をオフセットして中央合わせ）
    let offset = -(blur_margin as f32) * SCALE;
    final_pixmap.draw_pixmap(
        0,
        0,
        blurred_bg.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(SCALE, SCALE)
            .pre_translate(offset / SCALE, offset / SCALE),
        None,
    );

    // シャドウを2xスケールで描画
    final_pixmap.draw_pixmap(
        0,
        0,
        shadow.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        None,
    );

    // コード SVG を上に合成
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
        let blurred =
            blur_pixmap(bg, 8.0).expect("ぼかしに成功するべき");
        assert_ne!(
            original_data,
            blurred.data(),
            "ぼかし後のピクセルデータは元と異なるべき"
        );
    }

    #[test]
    fn blur_pixmap_zero_radius_returns_unchanged() {
        let bg = super::super::background::generate_gradient_pixmap(100, 100)
            .expect("背景Pixmap生成に成功するべき");
        let original_data = bg.data().to_vec();
        let result =
            blur_pixmap(bg, 0.0).expect("ぼかしに成功するべき");
        assert_eq!(
            original_data,
            result.data(),
            "ぼかし強度0はピクセルデータを変更しないべき"
        );
    }

    #[test]
    fn blur_pixmap_preserves_dimensions() {
        let bg = super::super::background::generate_gradient_pixmap(200, 150)
            .expect("背景Pixmap生成に成功するべき");
        let blurred =
            blur_pixmap(bg, 5.0).expect("ぼかしに成功するべき");
        assert_eq!(blurred.width(), 200);
        assert_eq!(blurred.height(), 150);
    }

    #[test]
    fn create_shadow_pixmap_produces_valid_pixmap() {
        let shadow = create_shadow_pixmap(864.0, 192.0)
            .expect("シャドウPixmap生成に成功するべき");
        assert_eq!(shadow.width(), 864);
        assert_eq!(shadow.height(), 192);
    }

    #[test]
    fn create_shadow_pixmap_has_opaque_center() {
        // シャドウの中央付近には不透明ピクセルがあるべき
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
}
