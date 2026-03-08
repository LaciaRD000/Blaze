use std::fmt::Write;

use crate::renderer::highlight::HighlightedLine;
use crate::sanitize::escape_for_svg;

const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
const PADDING_X: f32 = 16.0;
const PADDING_Y: f32 = 16.0;
const TITLE_BAR_HEIGHT: f32 = 36.0;
const SHADOW_MARGIN: f32 = 32.0;
const BORDER_RADIUS: f32 = 12.0;

/// SVG生成オプション
pub struct SvgOptions<'a> {
    pub bg_color: &'a str,
    pub language: Option<&'a str>,
    pub title_bar_style: &'a str,
    pub opacity: f32,
    /// 背景画像のBase64文字列（PNG）。None の場合は背景画像なし
    pub background_image: Option<&'a str>,
    /// ガウスぼかしの強度（stdDeviation）
    pub blur_radius: f32,
}

impl Default for SvgOptions<'_> {
    fn default() -> Self {
        Self {
            bg_color: "#1e1e2e",
            language: None,
            title_bar_style: "macos",
            opacity: 0.75,
            background_image: None,
            blur_radius: 8.0,
        }
    }
}

/// ハイライト済みコード行からSVG文字列を生成する
pub fn build_svg(lines: &[HighlightedLine], options: &SvgOptions) -> String {
    let window_width = 800.0;
    let code_height = PADDING_Y * 2.0 + LINE_HEIGHT * lines.len() as f32;
    let title_bar_h = if options.title_bar_style == "none" {
        0.0
    } else {
        TITLE_BAR_HEIGHT
    };
    let window_height = title_bar_h + code_height;

    // 外側マージン（シャドウが見えるように）
    let total_width = window_width + SHADOW_MARGIN * 2.0;
    let total_height = window_height + SHADOW_MARGIN * 2.0;

    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_width}" height="{total_height}">"##
    );

    // defs: フィルタ定義
    let _ = write!(svg, r##"<defs>"##);
    // ドロップシャドウフィルタ
    let _ = write!(
        svg,
        r##"<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="8" stdDeviation="16" flood-opacity="0.4"/></filter>"##
    );
    // ガウスぼかしフィルタ（背景画像がある場合のみ）
    if options.background_image.is_some() {
        let blur = options.blur_radius;
        let _ = write!(
            svg,
            r##"<filter id="blur"><feGaussianBlur stdDeviation="{blur}"/></filter>"##
        );
    }
    let _ = write!(svg, r##"</defs>"##);

    // 背景画像レイヤー（ぼかし付き）
    if let Some(bg_image) = options.background_image {
        let _ = write!(
            svg,
            r##"<image href="data:image/png;base64,{bg_image}" x="0" y="0" width="{total_width}" height="{total_height}" filter="url(#blur)"/>"##
        );
    }

    // ウィンドウグループ（シャドウ + 角丸）
    let _ = write!(
        svg,
        r##"<g transform="translate({SHADOW_MARGIN},{SHADOW_MARGIN})" filter="url(#shadow)">"##
    );

    // ウィンドウ背景（角丸 + 半透明）
    let bg = options.bg_color;
    let opacity = options.opacity;
    let _ = write!(
        svg,
        r##"<rect width="{window_width}" height="{window_height}" rx="{BORDER_RADIUS}" fill="{bg}" fill-opacity="{opacity}"/>"##
    );

    // タイトルバー
    if options.title_bar_style == "macos" {
        build_macos_title_bar(&mut svg, options.language);
    }

    // コード行
    for (i, line) in lines.iter().enumerate() {
        let y = title_bar_h + PADDING_Y + FONT_SIZE + LINE_HEIGHT * i as f32;
        let _ = write!(
            svg,
            r##"<text x="{PADDING_X}" y="{y}" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="{FONT_SIZE}" xml:space="preserve">"##
        );

        for token in &line.tokens {
            let color_hex = format!(
                "#{:02x}{:02x}{:02x}",
                token.color.r, token.color.g, token.color.b
            );
            let escaped = escape_for_svg(&token.text);

            if token.bold && token.italic {
                let _ = write!(
                    svg,
                    r##"<tspan fill="{color_hex}" font-weight="bold" font-style="italic">{escaped}</tspan>"##
                );
            } else if token.bold {
                let _ = write!(
                    svg,
                    r##"<tspan fill="{color_hex}" font-weight="bold">{escaped}</tspan>"##
                );
            } else if token.italic {
                let _ = write!(
                    svg,
                    r##"<tspan fill="{color_hex}" font-style="italic">{escaped}</tspan>"##
                );
            } else {
                let _ = write!(
                    svg,
                    r##"<tspan fill="{color_hex}">{escaped}</tspan>"##
                );
            }
        }

        svg.push_str("</text>");
    }

    // ウィンドウグループ閉じ
    svg.push_str("</g>");

    svg.push_str("</svg>");
    svg
}

/// macOS風タイトルバー（赤・黄・緑の円ボタン + 言語名）
fn build_macos_title_bar(svg: &mut String, language: Option<&str>) {
    // 3つの円ボタン
    let _ = write!(svg, r##"<circle cx="20" cy="18" r="6" fill="#ff5f57"/>"##);
    let _ = write!(svg, r##"<circle cx="40" cy="18" r="6" fill="#febc2e"/>"##);
    let _ = write!(svg, r##"<circle cx="60" cy="18" r="6" fill="#28c840"/>"##);

    // 言語名テキスト
    if let Some(lang) = language {
        let escaped_lang = escape_for_svg(lang);
        let _ = write!(
            svg,
            r##"<text x="400" y="22" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="13" fill="#6c7086" text-anchor="middle">{escaped_lang}</text>"##
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::highlight::{Color, HighlightedLine, StyledToken};

    fn sample_lines() -> Vec<HighlightedLine> {
        vec![
            HighlightedLine {
                tokens: vec![
                    StyledToken {
                        text: "fn".to_string(),
                        color: Color {
                            r: 203,
                            g: 166,
                            b: 247,
                            a: 255,
                        },
                        bold: true,
                        italic: false,
                    },
                    StyledToken {
                        text: " main() {}".to_string(),
                        color: Color {
                            r: 205,
                            g: 214,
                            b: 244,
                            a: 255,
                        },
                        bold: false,
                        italic: false,
                    },
                ],
            },
            HighlightedLine {
                tokens: vec![StyledToken {
                    text: "    println!(\"hello\");".to_string(),
                    color: Color {
                        r: 205,
                        g: 214,
                        b: 244,
                        a: 255,
                    },
                    bold: false,
                    italic: false,
                }],
            },
        ]
    }

    fn default_options() -> SvgOptions<'static> {
        SvgOptions {
            bg_color: "#1e1e2e",
            language: Some("rust"),
            title_bar_style: "macos",
            opacity: 0.75,
            background_image: None,
            blur_radius: 8.0,
        }
    }

    #[test]
    fn build_svg_contains_svg_root() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn build_svg_height_depends_on_line_count() {
        let one_line = vec![sample_lines().into_iter().next().unwrap()];
        let two_lines = sample_lines();
        let opts = default_options();

        let svg1 = build_svg(&one_line, &opts);
        let svg2 = build_svg(&two_lines, &opts);

        let h1 = extract_height(&svg1);
        let h2 = extract_height(&svg2);
        assert!(h2 > h1, "2行のSVGの方が高さが大きいべき: h1={h1}, h2={h2}");
    }

    #[test]
    fn build_svg_contains_tspan_with_color() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(svg.contains("<tspan"));
        assert!(svg.contains("fill=\"#"));
    }

    #[test]
    fn build_svg_contains_xml_space_preserve() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(
            svg.contains("xml:space=\"preserve\""),
            "xml:space=\"preserve\" が含まれるべき"
        );
    }

    #[test]
    fn build_svg_escapes_special_chars() {
        let lines = vec![HighlightedLine {
            tokens: vec![StyledToken {
                text: "<div>&\"test\"</div>".to_string(),
                color: Color {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                bold: false,
                italic: false,
            }],
        }];
        let svg = build_svg(&lines, &default_options());
        assert!(svg.contains("&lt;div&gt;"));
        assert!(svg.contains("&amp;"));
        assert!(svg.contains("&quot;"));
    }

    #[test]
    fn build_svg_bold_has_font_weight() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(svg.contains("font-weight=\"bold\""));
    }

    #[test]
    fn build_svg_macos_title_bar_has_circles() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(
            svg.contains(r##"fill="#ff5f57""##),
            "赤ボタンが含まれるべき"
        );
        assert!(
            svg.contains(r##"fill="#febc2e""##),
            "黄ボタンが含まれるべき"
        );
        assert!(
            svg.contains(r##"fill="#28c840""##),
            "緑ボタンが含まれるべき"
        );
    }

    #[test]
    fn build_svg_macos_title_bar_has_language_name() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(svg.contains(">rust</text>"), "言語名が含まれるべき");
    }

    #[test]
    fn build_svg_no_title_bar_has_no_circles() {
        let opts = SvgOptions {
            title_bar_style: "none",
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        assert!(
            !svg.contains("circle"),
            "タイトルバーなしでは circle がないべき"
        );
    }

    #[test]
    fn build_svg_has_rounded_corners() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(svg.contains("rx=\"12\""), "角丸のrx属性が含まれるべき");
    }

    #[test]
    fn build_svg_has_drop_shadow_filter() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(
            svg.contains("feDropShadow") || svg.contains("feGaussianBlur"),
            "ドロップシャドウフィルタが含まれるべき"
        );
        assert!(
            svg.contains("filter=\"url(#shadow)\""),
            "シャドウフィルタが適用されているべき"
        );
    }

    #[test]
    fn build_svg_has_fill_opacity() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(
            svg.contains("fill-opacity=\"0.75\""),
            "fill-opacity が含まれるべき"
        );
    }

    #[test]
    fn build_svg_with_background_has_gaussian_blur() {
        let opts = SvgOptions {
            // ダミーの1x1 PNG（Base64）
            background_image: Some(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            ),
            blur_radius: 8.0,
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        assert!(
            svg.contains("feGaussianBlur"),
            "ガウスぼかしフィルタが含まれるべき"
        );
        assert!(
            svg.contains("filter=\"url(#blur)\""),
            "ぼかしフィルタが背景画像に適用されるべき"
        );
        assert!(svg.contains("<image"), "背景画像要素が含まれるべき");
        assert!(
            svg.contains("data:image/png;base64,"),
            "Base64埋め込みの背景画像が含まれるべき"
        );
    }

    #[test]
    fn build_svg_without_background_has_no_blur() {
        let svg = build_svg(&sample_lines(), &default_options());
        assert!(
            !svg.contains("feGaussianBlur"),
            "背景画像なしではガウスぼかしフィルタがないべき"
        );
        assert!(
            !svg.contains("<image"),
            "背景画像なしでは image 要素がないべき"
        );
    }

    /// SVGからheight属性の数値を抽出するヘルパー
    fn extract_height(svg: &str) -> f32 {
        let start = svg.find("height=\"").unwrap() + 8;
        let end = svg[start..].find('"').unwrap() + start;
        svg[start..end].parse().unwrap()
    }
}
