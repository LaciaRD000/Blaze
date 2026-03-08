use base64::Engine;

use crate::error::BlazeError;

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
}
