use crate::error::BlazeError;

/// 埋め込み背景画像データ
static DENIM_WEBP: &[u8] =
    include_bytes!("../../assets/backgrounds/denim.webp");
static REPEATED_SQUARE_DARK_WEBP: &[u8] =
    include_bytes!("../../assets/backgrounds/repeated-square-dark.webp");

/// 起動時にWebPをデコードしてキャッシュする構造体
pub struct BackgroundCache {
    denim: image::RgbaImage,
    repeated_square_dark: image::RgbaImage,
}

impl Default for BackgroundCache {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundCache {
    /// WebP 画像を起動時に1回だけデコードしてキャッシュする
    pub fn new() -> Self {
        let denim = image::load_from_memory_with_format(
            DENIM_WEBP,
            image::ImageFormat::WebP,
        )
        .expect("denim.webp のデコードに失敗")
        .to_rgba8();

        let repeated_square_dark = image::load_from_memory_with_format(
            REPEATED_SQUARE_DARK_WEBP,
            image::ImageFormat::WebP,
        )
        .expect("repeated-square-dark.webp のデコードに失敗")
        .to_rgba8();

        Self {
            denim,
            repeated_square_dark,
        }
    }
}

/// image::RgbaImage (straight alpha) を tiny_skia::Pixmap (premultiplied alpha) に変換
fn rgba_to_pixmap(
    img: &image::RgbaImage,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    let w = img.width();
    let h = img.height();
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| BlazeError::rendering("Pixmap作成に失敗"))?;
    let src = img.as_raw();
    let dst = pixmap.data_mut();
    for i in (0..src.len()).step_by(4) {
        let a = src[i + 3];
        let alpha = a as f32 / 255.0;
        dst[i] = (src[i] as f32 * alpha + 0.5) as u8;
        dst[i + 1] = (src[i + 1] as f32 * alpha + 0.5) as u8;
        dst[i + 2] = (src[i + 2] as f32 * alpha + 0.5) as u8;
        dst[i + 3] = a;
    }
    Ok(pixmap)
}

/// タイル画像を指定サイズまで敷き詰めて tiny_skia::Pixmap を返す
fn tile_to_pixmap(
    tile: &image::RgbaImage,
    width: u32,
    height: u32,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    let tile_w = tile.width();
    let tile_h = tile.height();
    let mut canvas = image::RgbaImage::new(width, height);

    for y in (0..height).step_by(tile_h as usize) {
        for x in (0..width).step_by(tile_w as usize) {
            image::imageops::overlay(&mut canvas, tile, x as i64, y as i64);
        }
    }

    rgba_to_pixmap(&canvas)
}

/// グラデーション背景を Pixmap として直接生成する
pub fn generate_gradient_pixmap(
    width: u32,
    height: u32,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("Pixmap作成に失敗"))?;

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
            .ok_or_else(|| BlazeError::rendering("Rect作成に失敗"))?;
    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);

    Ok(pixmap)
}

/// 背景画像IDに対応する Pixmap を返す
/// キャッシュ済みタイルを使い、PNG エンコード/デコードを一切行わない
pub fn load_background_pixmap(
    cache: &BackgroundCache,
    background_id: &str,
    width: u32,
    height: u32,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    match background_id {
        "gradient" => generate_gradient_pixmap(width, height),
        "denim" => tile_to_pixmap(&cache.denim, width, height),
        "repeated-square-dark" => {
            tile_to_pixmap(&cache.repeated_square_dark, width, height)
        }
        other => Err(BlazeError::rendering(format!(
            "不明な背景画像ID: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_cache_new_succeeds() {
        let _cache = BackgroundCache::new();
    }

    #[test]
    fn generate_gradient_pixmap_has_correct_size() {
        let pixmap = generate_gradient_pixmap(200, 100)
            .expect("グラデーションPixmap生成に成功するべき");
        assert_eq!(pixmap.width(), 200);
        assert_eq!(pixmap.height(), 100);
    }

    #[test]
    fn load_background_pixmap_gradient_has_correct_size() {
        let cache = BackgroundCache::new();
        let pixmap = load_background_pixmap(&cache, "gradient", 864, 200)
            .expect("グラデーション背景の読み込みに成功するべき");
        assert_eq!(pixmap.width(), 864);
        assert_eq!(pixmap.height(), 200);
    }

    #[test]
    fn load_background_pixmap_denim_has_correct_size() {
        let cache = BackgroundCache::new();
        let pixmap = load_background_pixmap(&cache, "denim", 864, 200)
            .expect("denim背景の読み込みに成功するべき");
        assert_eq!(pixmap.width(), 864);
        assert_eq!(pixmap.height(), 200);
    }

    #[test]
    fn load_background_pixmap_repeated_square_dark_has_correct_size() {
        let cache = BackgroundCache::new();
        let pixmap =
            load_background_pixmap(&cache, "repeated-square-dark", 864, 300)
                .expect("repeated-square-dark背景の読み込みに成功するべき");
        assert_eq!(pixmap.width(), 864);
        assert_eq!(pixmap.height(), 300);
    }

    #[test]
    fn load_background_pixmap_unknown_returns_error() {
        let cache = BackgroundCache::new();
        let result = load_background_pixmap(&cache, "unknown", 200, 100);
        assert!(result.is_err(), "不明な背景IDはエラーを返すべき");
    }

    #[test]
    fn load_background_pixmap_denim_not_all_transparent() {
        let cache = BackgroundCache::new();
        let pixmap = load_background_pixmap(&cache, "denim", 200, 100)
            .expect("denim背景の読み込みに成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque, "背景画像は透明でないピクセルを含むべき");
    }
}
