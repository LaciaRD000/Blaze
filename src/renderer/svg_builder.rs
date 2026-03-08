use std::fmt::Write;

use crate::renderer::highlight::HighlightedLine;
use crate::sanitize::escape_for_svg;

const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
const PADDING_X: f32 = 16.0;
const PADDING_Y: f32 = 16.0;

/// ハイライト済みコード行からSVG文字列を生成する（最小限：背景色 + 色付きテキスト）
pub fn build_svg(lines: &[HighlightedLine], bg_color: &str) -> String {
    let width = 800.0;
    let height = PADDING_Y * 2.0 + LINE_HEIGHT * lines.len() as f32;

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">"#
    );

    // 背景
    let _ = write!(
        svg,
        r#"<rect width="{width}" height="{height}" fill="{bg_color}"/>"#
    );

    // コード行
    for (i, line) in lines.iter().enumerate() {
        let y = PADDING_Y + FONT_SIZE + LINE_HEIGHT * i as f32;
        let _ = write!(
            svg,
            r#"<text x="{PADDING_X}" y="{y}" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="{FONT_SIZE}" xml:space="preserve">"#
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
                    r#"<tspan fill="{color_hex}" font-weight="bold" font-style="italic">{escaped}</tspan>"#
                );
            } else if token.bold {
                let _ = write!(
                    svg,
                    r#"<tspan fill="{color_hex}" font-weight="bold">{escaped}</tspan>"#
                );
            } else if token.italic {
                let _ = write!(
                    svg,
                    r#"<tspan fill="{color_hex}" font-style="italic">{escaped}</tspan>"#
                );
            } else {
                let _ = write!(
                    svg,
                    r#"<tspan fill="{color_hex}">{escaped}</tspan>"#
                );
            }
        }

        svg.push_str("</text>");
    }

    svg.push_str("</svg>");
    svg
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

    #[test]
    fn build_svg_contains_svg_root() {
        let svg = build_svg(&sample_lines(), "#1e1e2e");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn build_svg_height_depends_on_line_count() {
        let one_line = vec![sample_lines().into_iter().next().unwrap()];
        let two_lines = sample_lines();

        let svg1 = build_svg(&one_line, "#1e1e2e");
        let svg2 = build_svg(&two_lines, "#1e1e2e");

        // 2行の方が高さが大きい
        let h1 = extract_height(&svg1);
        let h2 = extract_height(&svg2);
        assert!(h2 > h1, "2行のSVGの方が高さが大きいべき: h1={h1}, h2={h2}");
    }

    #[test]
    fn build_svg_contains_tspan_with_color() {
        let svg = build_svg(&sample_lines(), "#1e1e2e");
        // tspan に fill 属性が含まれる
        assert!(svg.contains("<tspan"));
        assert!(svg.contains("fill=\"#"));
    }

    #[test]
    fn build_svg_contains_xml_space_preserve() {
        let svg = build_svg(&sample_lines(), "#1e1e2e");
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
        let svg = build_svg(&lines, "#1e1e2e");
        assert!(svg.contains("&lt;div&gt;"));
        assert!(svg.contains("&amp;"));
        assert!(svg.contains("&quot;"));
    }

    #[test]
    fn build_svg_bold_has_font_weight() {
        let svg = build_svg(&sample_lines(), "#1e1e2e");
        assert!(svg.contains("font-weight=\"bold\""));
    }

    /// SVGからheight属性の数値を抽出するヘルパー
    fn extract_height(svg: &str) -> f32 {
        let start = svg.find("height=\"").unwrap() + 8;
        let end = svg[start..].find('"').unwrap() + start;
        svg[start..end].parse().unwrap()
    }
}
