use std::sync::Arc;

use base64::Engine;
use resvg::usvg;

use crate::error::BlazeError;

/// レンダリングスケール（高DPI対応）
const SCALE: f32 = 2.0;

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

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("Pixmap の作成に失敗"))?;

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(SCALE, SCALE),
        &mut pixmap.as_mut(),
    );

    pixmap
        .encode_png()
        .map_err(|e| BlazeError::rendering(format!("PNGエンコード失敗: {e}")))
}

/// 背景 Pixmap にぼかしを適用する
/// 小さなぼかし専用 SVG を生成し resvg で処理する
fn blur_pixmap(
    bg: tiny_skia::Pixmap,
    blur_radius: f64,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    if blur_radius <= 0.0 {
        return Ok(bg);
    }

    let w = bg.width();
    let h = bg.height();
    let png = bg
        .encode_png()
        .map_err(|e| BlazeError::rendering(format!("背景PNGエンコード失敗: {e}")))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);

    let blur_svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><defs><filter id="b"><feGaussianBlur stdDeviation="{blur_radius}"/></filter></defs><image href="data:image/png;base64,{b64}" width="{w}" height="{h}" filter="url(#b)"/></svg>"##
    );

    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(&blur_svg, &options)
        .map_err(|e| BlazeError::rendering(format!("ぼかしSVGパース失敗: {e}")))?;

    let mut blurred = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| BlazeError::rendering("ぼかしPixmap作成に失敗"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut blurred.as_mut(),
    );

    Ok(blurred)
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

    // 3. 最終キャンバスに合成: ぼかし背景（2xスケール）→ コード
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

    // コード SVG を上に合成
    final_pixmap.draw_pixmap(
        0,
        0,
        code_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    final_pixmap
        .encode_png()
        .map_err(|e| BlazeError::rendering(format!("PNGエンコード失敗: {e}")))
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
