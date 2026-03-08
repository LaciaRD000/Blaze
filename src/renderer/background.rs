use std::io::Cursor;

use base64::Engine;

use crate::error::BlazeError;

/// 埋め込み背景画像データ
static DENIM_WEBP: &[u8] =
    include_bytes!("../../assets/backgrounds/denim.webp");
static REPEATED_SQUARE_DARK_WEBP: &[u8] =
    include_bytes!("../../assets/backgrounds/repeated-square-dark.webp");

/// WebP 画像データをデコードする
fn decode_webp(
    webp_data: &[u8],
) -> Result<image::DynamicImage, BlazeError> {
    image::load_from_memory_with_format(webp_data, image::ImageFormat::WebP)
        .map_err(|e| {
            BlazeError::rendering(format!("WebP画像のデコードに失敗: {e}"))
        })
}

/// タイル画像を指定サイズまで繰り返し敷き詰めてPNGバイト列を返す
fn tile_to_png(
    img: &image::DynamicImage,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, BlazeError> {
    let tile = img.to_rgba8();
    let tile_w = tile.width();
    let tile_h = tile.height();
    let mut canvas = image::RgbaImage::new(width, height);

    for y in (0..height).step_by(tile_h as usize) {
        for x in (0..width).step_by(tile_w as usize) {
            image::imageops::overlay(
                &mut canvas,
                &tile,
                x as i64,
                y as i64,
            );
        }
    }

    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| {
            BlazeError::rendering(format!("PNGエンコードに失敗: {e}"))
        })?;
    Ok(buf)
}

/// 背景画像IDに対応するPNGバイト列を返す
/// "gradient" → 動的生成、"denim" / "repeated-square-dark" → タイリングで敷き詰め
pub fn load_background(
    background_id: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, BlazeError> {
    match background_id {
        "gradient" => Ok(generate_default_background(width, height)),
        "denim" => {
            let img = decode_webp(DENIM_WEBP)?;
            tile_to_png(&img, width, height)
        }
        "repeated-square-dark" => {
            let img = decode_webp(REPEATED_SQUARE_DARK_WEBP)?;
            tile_to_png(&img, width, height)
        }
        other => Err(BlazeError::rendering(format!(
            "不明な背景画像ID: {other}"
        ))),
    }
}

/// PNG画像データを指定サイズにリサイズし、Base64文字列として返す
pub fn resize_to_base64(
    png_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<String, BlazeError> {
    // 元のPNGをデコード
    let src_pixmap = tiny_skia::Pixmap::decode_png(png_data).map_err(|e| {
        BlazeError::rendering(format!("背景画像のデコードに失敗: {e}"))
    })?;

    let src_width = src_pixmap.width();
    let src_height = src_pixmap.height();

    // リサイズ不要な場合はそのままエンコード
    if src_width == target_width && src_height == target_height {
        return Ok(base64::engine::general_purpose::STANDARD.encode(png_data));
    }

    // 新しい Pixmap を作成
    let mut dst_pixmap = tiny_skia::Pixmap::new(target_width, target_height)
        .ok_or_else(|| BlazeError::rendering("リサイズ先のPixmap作成に失敗"))?;

    // tiny-skia の PixmapPaint でスケーリング描画
    let sx = target_width as f32 / src_width as f32;
    let sy = target_height as f32 / src_height as f32;
    let transform = tiny_skia::Transform::from_scale(sx, sy);

    dst_pixmap.draw_pixmap(
        0,
        0,
        src_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        transform,
        None,
    );

    // PNGにエンコードしてBase64化
    let resized_png = dst_pixmap.encode_png().map_err(|e| {
        BlazeError::rendering(format!("リサイズ画像のPNGエンコードに失敗: {e}"))
    })?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&resized_png))
}

/// デフォルト背景画像（グラデーション）を生成する
pub fn generate_default_background(width: u32, height: u32) -> Vec<u8> {
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).expect("Pixmap作成に失敗");

    // 暗い紫〜青のグラデーション背景
    let mut paint = tiny_skia::Paint::default();
    if let Some(gradient) = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(0.0, 0.0),
        tiny_skia::Point::from_xy(width as f32, height as f32),
        vec![
            tiny_skia::GradientStop::new(
                0.0,
                tiny_skia::Color::from_rgba8(30, 20, 60, 255),
            ),
            tiny_skia::GradientStop::new(
                0.5,
                tiny_skia::Color::from_rgba8(20, 40, 80, 255),
            ),
            tiny_skia::GradientStop::new(
                1.0,
                tiny_skia::Color::from_rgba8(40, 20, 60, 255),
            ),
        ],
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    ) {
        paint.shader = gradient;
    }

    let rect =
        tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32)
            .expect("Rect作成に失敗");
    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);

    pixmap.encode_png().expect("PNGエンコードに失敗")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_default_background_produces_valid_png() {
        let png = generate_default_background(200, 100);
        assert!(!png.is_empty());
        // PNGマジックバイト
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn resize_to_base64_same_size_returns_base64() {
        let png = generate_default_background(100, 50);
        let b64 =
            resize_to_base64(&png, 100, 50).expect("リサイズに成功するべき");
        assert!(!b64.is_empty());
        // Base64 デコードして PNG であることを確認
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .expect("Base64デコードに成功するべき");
        assert_eq!(&decoded[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn resize_to_base64_downscale_produces_smaller_image() {
        let png = generate_default_background(400, 200);
        let b64 =
            resize_to_base64(&png, 100, 50).expect("リサイズに成功するべき");

        // デコードしてピクセルサイズを確認
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .expect("Base64デコードに成功するべき");
        let pixmap = tiny_skia::Pixmap::decode_png(&decoded)
            .expect("PNGデコードに成功するべき");
        assert_eq!(pixmap.width(), 100);
        assert_eq!(pixmap.height(), 50);
    }

    #[test]
    fn resize_to_base64_upscale_produces_larger_image() {
        let png = generate_default_background(50, 25);
        let b64 =
            resize_to_base64(&png, 200, 100).expect("リサイズに成功するべき");

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .expect("Base64デコードに成功するべき");
        let pixmap = tiny_skia::Pixmap::decode_png(&decoded)
            .expect("PNGデコードに成功するべき");
        assert_eq!(pixmap.width(), 200);
        assert_eq!(pixmap.height(), 100);
    }

    #[test]
    fn resize_to_base64_invalid_png_returns_error() {
        let result = resize_to_base64(b"not a png", 100, 50);
        assert!(result.is_err(), "不正なPNGデータはエラーを返すべき");
    }

    #[test]
    fn load_background_gradient_produces_valid_png() {
        let png = load_background("gradient", 200, 100)
            .expect("グラデーション背景の読み込みに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn load_background_denim_produces_valid_png() {
        let png = load_background("denim", 200, 100)
            .expect("denim背景の読み込みに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn load_background_repeated_square_dark_produces_valid_png() {
        let png = load_background("repeated-square-dark", 200, 100)
            .expect("repeated-square-dark背景の読み込みに成功するべき");
        assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn load_background_unknown_returns_error() {
        let result = load_background("unknown", 200, 100);
        assert!(result.is_err(), "不明な背景IDはエラーを返すべき");
    }

    #[test]
    fn load_background_denim_tiled_matches_target_size() {
        let png = load_background("denim", 864, 200)
            .expect("denim背景のタイリングに成功するべき");
        let pixmap = tiny_skia::Pixmap::decode_png(&png)
            .expect("PNGデコードに成功するべき");
        assert_eq!(pixmap.width(), 864);
        assert_eq!(pixmap.height(), 200);
    }

    #[test]
    fn load_background_repeated_square_dark_tiled_matches_target_size() {
        let png = load_background("repeated-square-dark", 864, 300)
            .expect("repeated-square-dark背景のタイリングに成功するべき");
        let pixmap = tiny_skia::Pixmap::decode_png(&png)
            .expect("PNGデコードに成功するべき");
        assert_eq!(pixmap.width(), 864);
        assert_eq!(pixmap.height(), 300);
    }
}
