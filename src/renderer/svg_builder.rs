use std::fmt::Write;

use crate::renderer::highlight::HighlightedLine;
use crate::sanitize::escape_for_svg;

const FONT_SIZE: f64 = 14.0;
const LINE_HEIGHT: f64 = 20.0;
const PADDING_X: f64 = 16.0;
const PADDING_Y: f64 = 16.0;
const TITLE_BAR_HEIGHT: f64 = 36.0;
const SHADOW_MARGIN: f64 = 32.0;
const BORDER_RADIUS: f64 = 12.0;

/// SVG生成オプション
pub struct SvgOptions<'a> {
    pub bg_color: &'a str,
    pub language: Option<&'a str>,
    pub title_bar_style: &'a str,
    pub opacity: f64,
    /// 背景画像のBase64文字列（PNG）。None の場合は背景画像なし
    pub background_image: Option<&'a str>,
    /// ガウスぼかしの強度（stdDeviation）
    pub blur_radius: f64,
    /// 1行あたりの最大文字数。超過分は `…` でトリミング。None で無制限
    pub max_line_length: Option<usize>,
    /// 行番号を表示するか
    pub show_line_numbers: bool,
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
            max_line_length: None,
            show_line_numbers: false,
        }
    }
}

/// 行番号の表示幅（文字数に応じた余白）
const LINE_NUMBER_WIDTH: f64 = 40.0;

/// トークン列を max_line_length に基づいてトリミングする
fn trim_tokens(
    tokens: &[crate::renderer::highlight::StyledToken],
    max_len: usize,
) -> Vec<crate::renderer::highlight::StyledToken> {
    let mut remaining = max_len;
    let mut result = Vec::new();

    for token in tokens {
        if remaining == 0 {
            break;
        }
        let char_count = token.text.chars().count();
        if char_count <= remaining {
            result.push(token.clone());
            remaining -= char_count;
        } else {
            // トリミングして省略記号を追加
            let truncated: String =
                token.text.chars().take(remaining).collect();
            let mut trimmed_token = token.clone();
            trimmed_token.text = format!("{truncated}…");
            result.push(trimmed_token);
            remaining = 0;
        }
    }

    result
}

/// ハイライト済みコード行からSVG文字列を生成する
pub fn build_svg(lines: &[HighlightedLine], options: &SvgOptions) -> String {
    let window_width = 800.0;
    let code_height = PADDING_Y * 2.0 + LINE_HEIGHT * lines.len() as f64;
    let title_bar_h = match options.title_bar_style {
        "macos" | "linux" | "plain" => TITLE_BAR_HEIGHT,
        _ => 0.0,
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
    // ぼかしの端フェードを防ぐため、画像をビューポートより大きく描画
    if let Some(bg_image) = options.background_image {
        let blur_margin = options.blur_radius * 3.0;
        let img_x = -blur_margin;
        let img_y = -blur_margin;
        let img_w = total_width + blur_margin * 2.0;
        let img_h = total_height + blur_margin * 2.0;
        let _ = write!(
            svg,
            r##"<image href="data:image/png;base64,{bg_image}" x="{img_x}" y="{img_y}" width="{img_w}" height="{img_h}" preserveAspectRatio="xMidYMid slice" filter="url(#blur)"/>"##
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
    match options.title_bar_style {
        "macos" => build_macos_title_bar(&mut svg, options.language),
        "linux" => {
            build_linux_title_bar(&mut svg, window_width, options.language)
        }
        "plain" => build_plain_title_bar(&mut svg, options.language),
        _ => {} // "none": タイトルバーなし
    }

    // コード行
    let code_x = if options.show_line_numbers {
        PADDING_X + LINE_NUMBER_WIDTH
    } else {
        PADDING_X
    };
    for (i, line) in lines.iter().enumerate() {
        let y = title_bar_h + PADDING_Y + FONT_SIZE + LINE_HEIGHT * i as f64;

        // 行番号
        if options.show_line_numbers {
            let line_num = i + 1;
            let _ = write!(
                svg,
                r##"<text x="{PADDING_X}" y="{y}" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="{FONT_SIZE}" fill="#6c7086" text-anchor="start">{line_num}</text>"##
            );
        }

        let _ = write!(
            svg,
            r##"<text x="{code_x}" y="{y}" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="{FONT_SIZE}" xml:space="preserve">"##
        );

        // max_line_length が指定されている場合はトリミング
        let trimmed;
        let tokens = if let Some(max_len) = options.max_line_length {
            trimmed = trim_tokens(&line.tokens, max_len);
            &trimmed
        } else {
            &line.tokens
        };

        for token in tokens {
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

/// プレーンタイトルバー（ボタンなし、言語名のみ中央表示）
fn build_plain_title_bar(svg: &mut String, language: Option<&str>) {
    if let Some(lang) = language {
        let escaped_lang = escape_for_svg(lang);
        let _ = write!(
            svg,
            r##"<text x="400" y="22" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="13" fill="#6c7086" text-anchor="middle">{escaped_lang}</text>"##
        );
    }
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

/// Linux WM 風タイトルバー（GNOME/Adwaita 風: 右上にアイコン付きボタン + 言語名）
fn build_linux_title_bar(
    svg: &mut String,
    window_width: f64,
    language: Option<&str>,
) {
    // 言語名テキスト（左寄せ）
    if let Some(lang) = language {
        let escaped_lang = escape_for_svg(lang);
        let _ = write!(
            svg,
            r##"<text x="16" y="22" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="13" fill="#6c7086">{escaped_lang}</text>"##
        );
    }

    // 右上のボタン（最小化・最大化・閉じる）
    let button_size: f64 = 16.0;
    let button_spacing: f64 = 24.0;
    let button_y: f64 = 10.0;
    let center_y = button_y + button_size / 2.0;
    let close_x = window_width - 28.0;
    let maximize_x = close_x - button_spacing;
    let minimize_x = maximize_x - button_spacing;

    let icon_color = "#cdd6f4";
    let button_bg = "#45475a";
    let close_bg = "#f38ba8";
    let close_icon_color = "#1e1e2e";

    // 最小化ボタン（丸背景 + 横線アイコン）
    let min_cx = minimize_x + button_size / 2.0;
    let _ = write!(
        svg,
        r##"<rect class="title-button" x="{minimize_x}" y="{button_y}" width="{button_size}" height="{button_size}" fill="{button_bg}" rx="{r}"/>"##,
        r = button_size / 2.0
    );
    let line_y = center_y;
    let _ = write!(
        svg,
        r##"<line class="title-icon-minimize" x1="{x1}" y1="{line_y}" x2="{x2}" y2="{line_y}" stroke="{icon_color}" stroke-width="1.5" stroke-linecap="round"/>"##,
        x1 = min_cx - 3.5,
        x2 = min_cx + 3.5
    );

    // 最大化ボタン（丸背景 + 四角枠アイコン）
    let max_cx = maximize_x + button_size / 2.0;
    let _ = write!(
        svg,
        r##"<rect class="title-button" x="{maximize_x}" y="{button_y}" width="{button_size}" height="{button_size}" fill="{button_bg}" rx="{r}"/>"##,
        r = button_size / 2.0
    );
    let _ = write!(
        svg,
        r##"<rect class="title-icon-maximize" x="{x}" y="{y}" width="7" height="7" fill="none" stroke="{icon_color}" stroke-width="1.5" rx="1"/>"##,
        x = max_cx - 3.5,
        y = center_y - 3.5
    );

    // 閉じるボタン（赤背景 + ×アイコン）
    let close_cx = close_x + button_size / 2.0;
    let _ = write!(
        svg,
        r##"<rect class="title-button-close" x="{close_x}" y="{button_y}" width="{button_size}" height="{button_size}" fill="{close_bg}" rx="{r}"/>"##,
        r = button_size / 2.0
    );
    let _ = write!(
        svg,
        r##"<line class="title-icon-close" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{close_icon_color}" stroke-width="1.5" stroke-linecap="round"/>"##,
        x1 = close_cx - 3.0,
        y1 = center_y - 3.0,
        x2 = close_cx + 3.0,
        y2 = center_y + 3.0
    );
    let _ = write!(
        svg,
        r##"<line class="title-icon-close" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{close_icon_color}" stroke-width="1.5" stroke-linecap="round"/>"##,
        x1 = close_cx + 3.0,
        y1 = center_y - 3.0,
        x2 = close_cx - 3.0,
        y2 = center_y + 3.0
    );
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
            max_line_length: None,
            show_line_numbers: false,
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
    fn build_svg_linux_title_bar_has_buttons() {
        let opts = SvgOptions {
            title_bar_style: "linux",
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        // Linux風は四角いボタン（rect要素）で閉じる・最大化・最小化を描画
        assert!(
            svg.contains("class=\"title-button\""),
            "Linux風タイトルバーにはボタンが含まれるべき"
        );
        // macOS 風の circle は含まれないべき
        assert!(
            !svg.contains("<circle"),
            "Linux風タイトルバーにはcircleがないべき"
        );
    }

    #[test]
    fn build_svg_linux_title_bar_has_distinct_close_button() {
        let opts = SvgOptions {
            title_bar_style: "linux",
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        // 閉じるボタンは他と異なる色（赤系）で描画されるべき
        assert!(
            svg.contains("class=\"title-button-close\""),
            "Linux風閉じるボタンは専用クラスを持つべき"
        );
    }

    #[test]
    fn build_svg_linux_title_bar_has_button_icons() {
        let opts = SvgOptions {
            title_bar_style: "linux",
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        // 最小化アイコン（横線）
        assert!(
            svg.contains("class=\"title-icon-minimize\""),
            "最小化アイコンが含まれるべき"
        );
        // 最大化アイコン（四角枠）
        assert!(
            svg.contains("class=\"title-icon-maximize\""),
            "最大化アイコンが含まれるべき"
        );
        // 閉じるアイコン（×線）
        assert!(
            svg.contains("class=\"title-icon-close\""),
            "閉じるアイコンが含まれるべき"
        );
    }

    #[test]
    fn build_svg_linux_title_bar_has_language_name() {
        let opts = SvgOptions {
            title_bar_style: "linux",
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        assert!(
            svg.contains(">rust</text>"),
            "Linux風でも言語名が含まれるべき"
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

    #[test]
    fn build_svg_trims_long_lines() {
        // 150文字の長い行を作成
        let long_text = "a".repeat(150);
        let lines = vec![HighlightedLine {
            tokens: vec![StyledToken {
                text: long_text,
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
        let opts = SvgOptions {
            max_line_length: Some(120),
            ..default_options()
        };
        let svg = build_svg(&lines, &opts);
        // トリミングされて "…" が含まれるべき
        assert!(
            svg.contains("…"),
            "120文字超の行はトリミングされて省略記号が含まれるべき"
        );
        // 元の150文字の 'a' が全部は含まれないべき
        assert!(
            !svg.contains(&"a".repeat(150)),
            "150文字すべてが含まれるべきではない"
        );
    }

    #[test]
    fn build_svg_no_trim_when_within_limit() {
        let short_text = "a".repeat(50);
        let lines = vec![HighlightedLine {
            tokens: vec![StyledToken {
                text: short_text.clone(),
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
        let opts = SvgOptions {
            max_line_length: Some(120),
            ..default_options()
        };
        let svg = build_svg(&lines, &opts);
        assert!(
            svg.contains(&short_text),
            "制限内の行はそのまま含まれるべき"
        );
        assert!(!svg.contains("…"), "制限内の行には省略記号がないべき");
    }

    #[test]
    fn build_svg_with_line_numbers_shows_numbers() {
        let opts = SvgOptions {
            show_line_numbers: true,
            ..default_options()
        };
        let svg = build_svg(&sample_lines(), &opts);
        // 行番号 "1" と "2" が含まれるべき
        assert!(svg.contains(">1</text>"), "行番号1が含まれるべき");
        assert!(svg.contains(">2</text>"), "行番号2が含まれるべき");
    }

    #[test]
    fn build_svg_without_line_numbers_has_no_numbers() {
        let svg = build_svg(&sample_lines(), &default_options());
        // 行番号用の薄い色テキストが含まれないべき
        // （コード内に "1" が含まれる可能性があるので、行番号専用の色で確認）
        let line_number_pattern = "fill=\"#6c7086\" text-anchor=\"start\">";
        assert!(
            !svg.contains(line_number_pattern),
            "行番号OFFでは行番号要素がないべき"
        );
    }

    /// SVGからheight属性の数値を抽出するヘルパー
    fn extract_height(svg: &str) -> f32 {
        let start = svg.find("height=\"").unwrap() + 8;
        let end = svg[start..].find('"').unwrap() + start;
        svg[start..end].parse().unwrap()
    }
}
