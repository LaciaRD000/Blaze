use std::sync::Arc;

use resvg::usvg;

use crate::error::BlazeError;

/// SVG文字列をPNGバイト列に変換する
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
    let width = size.width() as u32;
    let height = size.height() as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("Pixmap の作成に失敗"))?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    pixmap
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
}
