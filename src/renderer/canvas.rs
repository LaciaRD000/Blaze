/// SVG を介さずに tiny_skia + fontdue で直接コード画像を描画するモジュール
/// usvg パース (~50ms) と resvg レンダリングを完全に排除し、高速化を実現
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::error::BlazeError;
use crate::renderer::highlight::{HighlightedLine, StyledToken};

/// レイアウト定数（svg_builder.rs と同じ値）
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
const PADDING_X: f32 = 16.0;
const PADDING_Y: f32 = 16.0;
const TITLE_BAR_HEIGHT: f32 = 36.0;
const SHADOW_MARGIN: f32 = 32.0;
const BORDER_RADIUS: f32 = 12.0;
const WINDOW_WIDTH: f32 = 800.0;
const LINE_NUMBER_WIDTH: f32 = 40.0;

/// グリフキャッシュの型エイリアス: (char, f32ビット表現) → (Metrics, bitmap)
type GlyphCache = RwLock<HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>>;

/// ユーザーが選択可能なフォントファミリー
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFamily {
    FiraCode,
    PlemolJP,
    HackGenNF,
}

impl FontFamily {
    /// DB/設定文字列からフォントファミリーを解決する。不明な値は Fira Code にフォールバック
    pub fn from_name(name: &str) -> Self {
        match name {
            "PlemolJP" => Self::PlemolJP,
            "HackGen Console NF" => Self::HackGenNF,
            _ => Self::FiraCode,
        }
    }
}

/// フォントセット（プライマリ + フォールバック + グリフキャッシュ）
/// グリフキャッシュにより同一 (char, font_px) のラスタライズ結果を再利用する
pub struct FontSet {
    primary: fontdue::Font,
    fallback: fontdue::Font,
    /// RwLock により読み取りは共有ロック、書き込みのみ排他ロック
    glyph_cache: GlyphCache,
}

/// フォントバイナリ（compile-time 埋め込み）
static FIRA_CODE: &[u8] =
    include_bytes!("../../assets/fonts/FiraCode-Regular.ttf");
static PLEMOLJP: &[u8] =
    include_bytes!("../../assets/fonts/PlemolJP-Regular.ttf");
static HACKGEN_NF: &[u8] =
    include_bytes!("../../assets/fonts/HackGenConsoleNF-Regular.ttf");

impl Default for FontSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FontSet {
    pub fn new() -> Self {
        Self::with_family(FontFamily::FiraCode)
    }

    /// 指定フォントファミリーをプライマリとした FontSet を構築する
    /// プライマリにないグリフはフォールバックでラスタライズされる
    pub fn with_family(family: FontFamily) -> Self {
        let (primary_bytes, fallback_bytes): (&[u8], &[u8]) = match family {
            FontFamily::FiraCode => (FIRA_CODE, PLEMOLJP),
            FontFamily::PlemolJP => (PLEMOLJP, FIRA_CODE),
            FontFamily::HackGenNF => (HACKGEN_NF, PLEMOLJP),
        };
        let primary = fontdue::Font::from_bytes(
            primary_bytes,
            fontdue::FontSettings::default(),
        )
        .expect("プライマリフォントの読み込みに失敗");
        let fallback = fontdue::Font::from_bytes(
            fallback_bytes,
            fontdue::FontSettings::default(),
        )
        .expect("フォールバックフォントの読み込みに失敗");
        Self {
            primary,
            fallback,
            glyph_cache: RwLock::new(HashMap::new()),
        }
    }

    /// 文字に対応するフォントを選択し、ラスタライズする
    fn rasterize_char(
        &self,
        ch: char,
        px: f32,
    ) -> (fontdue::Metrics, Vec<u8>) {
        if self.primary.lookup_glyph_index(ch) != 0 {
            self.primary.rasterize(ch, px)
        } else {
            self.fallback.rasterize(ch, px)
        }
    }

    /// キャッシュ付きラスタライズ: 同一 (char, px) は再利用する
    /// 戻り値は (Metrics, &[u8]) への参照ではなくクローン（RwLock 内部のため）
    pub fn rasterize_cached(
        &self,
        ch: char,
        px: f32,
    ) -> (fontdue::Metrics, Vec<u8>) {
        let key = (ch, px.to_bits());

        // Fast path: 読み取りロックでキャッシュヒットを確認
        {
            let cache = self.glyph_cache.read().expect("グリフキャッシュ read lock");
            if let Some((metrics, bitmap)) = cache.get(&key) {
                return (*metrics, bitmap.clone());
            }
        }

        // Slow path: ラスタライズしてキャッシュに格納
        let result = self.rasterize_char(ch, px);
        {
            let mut cache = self.glyph_cache.write().expect("グリフキャッシュ write lock");
            cache.insert(key, result.clone());
        }
        result
    }

    /// 文字のアドバンス幅を取得する
    fn advance_width(&self, ch: char, px: f32) -> f32 {
        if self.primary.lookup_glyph_index(ch) != 0 {
            let metrics = self.primary.metrics(ch, px);
            metrics.advance_width
        } else {
            let metrics = self.fallback.metrics(ch, px);
            metrics.advance_width
        }
    }
}

/// キャンバス描画オプション
pub struct CanvasOptions<'a> {
    pub bg_color: [u8; 3],
    pub opacity: f32,
    pub title_bar_style: &'a str,
    pub language: Option<&'a str>,
    pub max_line_length: Option<usize>,
    pub show_line_numbers: bool,
}

/// SVG と同じ寸法を計算して返す (total_width, total_height)
pub fn calculate_dimensions(
    line_count: usize,
    title_bar_style: &str,
) -> (f32, f32) {
    let title_bar_h = match title_bar_style {
        "macos" | "linux" | "plain" => TITLE_BAR_HEIGHT,
        _ => 0.0,
    };
    let code_height =
        PADDING_Y * 2.0 + LINE_HEIGHT * line_count.max(1) as f32;
    let window_height = title_bar_h + code_height;
    let total_width = WINDOW_WIDTH + SHADOW_MARGIN * 2.0;
    let total_height = window_height + SHADOW_MARGIN * 2.0;
    (total_width, total_height)
}

/// ハイライト済みコード行を直接 Pixmap に描画する
/// scale: レンダリングスケール（通常 2.0）
pub fn render_code_pixmap(
    lines: &[HighlightedLine],
    font_set: &FontSet,
    options: &CanvasOptions,
    scale: f32,
) -> Result<tiny_skia::Pixmap, BlazeError> {
    let (total_w, total_h) =
        calculate_dimensions(lines.len(), options.title_bar_style);
    let width = (total_w * scale) as u32;
    let height = (total_h * scale) as u32;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| BlazeError::rendering("キャンバスPixmap作成に失敗"))?;

    render_code_onto_pixmap(&mut pixmap, lines, font_set, options, scale);
    Ok(pixmap)
}

/// 既存の Pixmap にハイライト済みコード行を直接描画する
/// Pixmap の確保を呼び出し側に委ねることで、二重確保を回避できる
pub fn render_code_onto_pixmap(
    pixmap: &mut tiny_skia::Pixmap,
    lines: &[HighlightedLine],
    font_set: &FontSet,
    options: &CanvasOptions,
    scale: f32,
) {
    let (_, total_h) =
        calculate_dimensions(lines.len(), options.title_bar_style);

    let s = scale; // 短縮エイリアス

    // ウィンドウ背景（角丸 + 半透明）
    draw_rounded_rect(
        pixmap,
        SHADOW_MARGIN * s,
        SHADOW_MARGIN * s,
        WINDOW_WIDTH * s,
        (total_h - SHADOW_MARGIN * 2.0) * s,
        BORDER_RADIUS * s,
        options.bg_color,
        (options.opacity * 255.0) as u8,
    );

    // タイトルバー
    let title_bar_h = match options.title_bar_style {
        "macos" | "linux" | "plain" => TITLE_BAR_HEIGHT,
        _ => 0.0,
    };
    match options.title_bar_style {
        "macos" => draw_macos_title_bar(
            pixmap,
            font_set,
            s,
            options.language,
        ),
        "linux" => draw_linux_title_bar(
            pixmap,
            font_set,
            s,
            options.language,
        ),
        "plain" => draw_plain_title_bar(
            pixmap,
            font_set,
            s,
            options.language,
        ),
        _ => {}
    }

    // コード行
    let font_px = FONT_SIZE * s;
    let code_x = if options.show_line_numbers {
        PADDING_X + LINE_NUMBER_WIDTH
    } else {
        PADDING_X
    };

    for (i, line) in lines.iter().enumerate() {
        let y = title_bar_h + PADDING_Y + FONT_SIZE
            + LINE_HEIGHT * i as f32;

        // 行番号
        if options.show_line_numbers {
            let num_str = format!("{}", i + 1);
            draw_text(
                pixmap,
                font_set,
                &num_str,
                (SHADOW_MARGIN + PADDING_X) * s,
                (SHADOW_MARGIN + y) * s,
                font_px,
                [0x6c, 0x70, 0x86],
            );
        }

        // トークン描画
        let mut x = (SHADOW_MARGIN + code_x) * s;
        let tokens: Cow<[StyledToken]> =
            if let Some(max_len) = options.max_line_length {
                Cow::Owned(trim_tokens(&line.tokens, max_len))
            } else {
                Cow::Borrowed(&line.tokens)
            };

        for token in tokens.iter() {
            let color = [token.color.r, token.color.g, token.color.b];
            let text_y = (SHADOW_MARGIN + y) * s;

            for ch in token.text.chars() {
                let (metrics, bitmap) =
                    font_set.rasterize_cached(ch, font_px);
                draw_glyph(
                    pixmap,
                    &bitmap,
                    &metrics,
                    x as i32 + metrics.xmin,
                    text_y as i32 - metrics.ymin
                        - metrics.height as i32,
                    color,
                );
                x += metrics.advance_width;
            }
        }
    }
}

/// 角丸矩形を描画する
#[allow(clippy::too_many_arguments)] // 描画プリミティブは座標+色+属性で引数が多い
fn draw_rounded_rect(
    pixmap: &mut tiny_skia::Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: [u8; 3],
    alpha: u8,
) {
    let path = {
        let mut pb = tiny_skia::PathBuilder::new();
        // 右上から時計回りに角丸パスを構築
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        pb.finish()
    };
    let Some(path) = path else { return };

    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], alpha);
    paint.anti_alias = true;

    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
}

/// 円を描画する
fn draw_circle(
    pixmap: &mut tiny_skia::Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    color: [u8; 3],
) {
    let path = {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_circle(cx, cy, r);
        pb.finish()
    };
    let Some(path) = path else { return };

    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;

    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
}

/// 線を描画する
fn draw_line(
    pixmap: &mut tiny_skia::Pixmap,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: [u8; 3],
    width: f32,
) {
    let path = {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        pb.finish()
    };
    let Some(path) = path else { return };

    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;

    let stroke = tiny_skia::Stroke {
        width,
        line_cap: tiny_skia::LineCap::Round,
        ..Default::default()
    };

    pixmap.stroke_path(
        &path,
        &paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );
}

/// 矩形の枠線を描画する
#[allow(clippy::too_many_arguments)] // 描画プリミティブは座標+色+属性で引数が多い
fn draw_rect_stroke(
    pixmap: &mut tiny_skia::Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    color: [u8; 3],
    stroke_width: f32,
) {
    let path = if rx > 0.0 {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(x + rx, y);
        pb.line_to(x + w - rx, y);
        pb.quad_to(x + w, y, x + w, y + rx);
        pb.line_to(x + w, y + h - rx);
        pb.quad_to(x + w, y + h, x + w - rx, y + h);
        pb.line_to(x + rx, y + h);
        pb.quad_to(x, y + h, x, y + h - rx);
        pb.line_to(x, y + rx);
        pb.quad_to(x, y, x + rx, y);
        pb.close();
        pb.finish()
    } else {
        let rect = tiny_skia::Rect::from_xywh(x, y, w, h);
        rect.and_then(|r| {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_rect(r);
            pb.finish()
        })
    };
    let Some(path) = path else { return };

    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;

    let stroke = tiny_skia::Stroke {
        width: stroke_width,
        ..Default::default()
    };

    pixmap.stroke_path(
        &path,
        &paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );
}

/// テキストを描画する
fn draw_text(
    pixmap: &mut tiny_skia::Pixmap,
    font_set: &FontSet,
    text: &str,
    x: f32,
    y: f32,
    font_px: f32,
    color: [u8; 3],
) {
    let mut cursor_x = x;
    for ch in text.chars() {
        let (metrics, bitmap) = font_set.rasterize_cached(ch, font_px);
        draw_glyph(
            pixmap,
            &bitmap,
            &metrics,
            cursor_x as i32 + metrics.xmin,
            y as i32 - metrics.ymin - metrics.height as i32,
            color,
        );
        cursor_x += metrics.advance_width;
    }
}

/// グリフビットマップを Pixmap に α ブレンドで描画する
/// tiny_skia は premultiplied alpha 形式
/// ループ前にクリッピング範囲を事前計算し、内部ループの per-pixel bounds check を排除
fn draw_glyph(
    pixmap: &mut tiny_skia::Pixmap,
    bitmap: &[u8],
    metrics: &fontdue::Metrics,
    dest_x: i32,
    dest_y: i32,
    color: [u8; 3],
) {
    let pw = pixmap.width() as i32;
    let ph = pixmap.height() as i32;

    // 事前にクリッピング範囲を計算（ループ内の per-pixel bounds check を排除）
    let y_start = 0i32.max(-dest_y) as usize;
    let y_end = (metrics.height as i32).min(ph - dest_y).max(0) as usize;
    let x_start = 0i32.max(-dest_x) as usize;
    let x_end = (metrics.width as i32).min(pw - dest_x).max(0) as usize;

    let data = pixmap.data_mut();

    for gy in y_start..y_end {
        let py = (dest_y + gy as i32) as usize;
        let row_offset = py * pw as usize;
        let bmp_row_offset = gy * metrics.width;

        for gx in x_start..x_end {
            let coverage = bitmap[bmp_row_offset + gx];
            if coverage == 0 {
                continue;
            }

            let idx = (row_offset + dest_x as usize + gx) * 4;
            let a = coverage as u16;
            let inv_a = 255 - a;

            // source-over compositing (premultiplied alpha)
            data[idx] = ((color[0] as u16 * a
                + data[idx] as u16 * inv_a)
                / 255) as u8;
            data[idx + 1] = ((color[1] as u16 * a
                + data[idx + 1] as u16 * inv_a)
                / 255) as u8;
            data[idx + 2] = ((color[2] as u16 * a
                + data[idx + 2] as u16 * inv_a)
                / 255) as u8;
            data[idx + 3] = (a
                + data[idx + 3] as u16 * inv_a / 255)
                as u8;
        }
    }
}

/// macOS 風タイトルバー（赤・黄・緑ボタン + 言語名）
fn draw_macos_title_bar(
    pixmap: &mut tiny_skia::Pixmap,
    font_set: &FontSet,
    s: f32,
    language: Option<&str>,
) {
    let ox = SHADOW_MARGIN * s;
    let oy = SHADOW_MARGIN * s;

    // 3つの円ボタン
    draw_circle(pixmap, ox + 20.0 * s, oy + 18.0 * s, 6.0 * s, [0xff, 0x5f, 0x57]);
    draw_circle(pixmap, ox + 40.0 * s, oy + 18.0 * s, 6.0 * s, [0xfe, 0xbc, 0x2e]);
    draw_circle(pixmap, ox + 60.0 * s, oy + 18.0 * s, 6.0 * s, [0x28, 0xc8, 0x40]);

    // 言語名テキスト（中央）
    if let Some(lang) = language {
        let title_px = 13.0 * s;
        // 中央揃え: テキスト幅を計算してオフセット
        let text_width: f32 = lang
            .chars()
            .map(|ch| font_set.advance_width(ch, title_px))
            .sum();
        let center_x = ox + 400.0 * s - text_width / 2.0;
        draw_text(
            pixmap,
            font_set,
            lang,
            center_x,
            oy + 22.0 * s,
            title_px,
            [0x6c, 0x70, 0x86],
        );
    }
}

/// Linux WM 風タイトルバー
fn draw_linux_title_bar(
    pixmap: &mut tiny_skia::Pixmap,
    font_set: &FontSet,
    s: f32,
    language: Option<&str>,
) {
    let ox = SHADOW_MARGIN * s;
    let oy = SHADOW_MARGIN * s;

    // 言語名テキスト（左寄せ）
    if let Some(lang) = language {
        let title_px = 13.0 * s;
        draw_text(
            pixmap,
            font_set,
            lang,
            ox + 16.0 * s,
            oy + 22.0 * s,
            title_px,
            [0x6c, 0x70, 0x86],
        );
    }

    // ボタン定数
    let button_size = 16.0 * s;
    let button_spacing = 24.0 * s;
    let button_y = oy + 10.0 * s;
    let center_y = button_y + button_size / 2.0;
    let close_x = ox + WINDOW_WIDTH * s - 28.0 * s;
    let maximize_x = close_x - button_spacing;
    let minimize_x = maximize_x - button_spacing;

    let icon_color = [0xcd, 0xd6, 0xf4];
    let button_bg = [0x45, 0x47, 0x5a];
    let close_bg = [0xf3, 0x8b, 0xa8];
    let close_icon_color = [0x1e, 0x1e, 0x2e];
    let r = button_size / 2.0;
    let stroke_w = 1.5 * s;

    // 最小化ボタン
    draw_rounded_rect(pixmap, minimize_x, button_y, button_size, button_size, r, button_bg, 255);
    let min_cx = minimize_x + button_size / 2.0;
    draw_line(pixmap, min_cx - 3.5 * s, center_y, min_cx + 3.5 * s, center_y, icon_color, stroke_w);

    // 最大化ボタン
    draw_rounded_rect(pixmap, maximize_x, button_y, button_size, button_size, r, button_bg, 255);
    let max_cx = maximize_x + button_size / 2.0;
    draw_rect_stroke(
        pixmap,
        max_cx - 3.5 * s,
        center_y - 3.5 * s,
        7.0 * s,
        7.0 * s,
        1.0 * s,
        icon_color,
        stroke_w,
    );

    // 閉じるボタン
    draw_rounded_rect(pixmap, close_x, button_y, button_size, button_size, r, close_bg, 255);
    let close_cx = close_x + button_size / 2.0;
    draw_line(
        pixmap,
        close_cx - 3.0 * s, center_y - 3.0 * s,
        close_cx + 3.0 * s, center_y + 3.0 * s,
        close_icon_color, stroke_w,
    );
    draw_line(
        pixmap,
        close_cx + 3.0 * s, center_y - 3.0 * s,
        close_cx - 3.0 * s, center_y + 3.0 * s,
        close_icon_color, stroke_w,
    );
}

/// プレーンタイトルバー（言語名のみ中央表示）
fn draw_plain_title_bar(
    pixmap: &mut tiny_skia::Pixmap,
    font_set: &FontSet,
    s: f32,
    language: Option<&str>,
) {
    if let Some(lang) = language {
        let ox = SHADOW_MARGIN * s;
        let oy = SHADOW_MARGIN * s;
        let title_px = 13.0 * s;
        let text_width: f32 = lang
            .chars()
            .map(|ch| font_set.advance_width(ch, title_px))
            .sum();
        let center_x = ox + 400.0 * s - text_width / 2.0;
        draw_text(
            pixmap,
            font_set,
            lang,
            center_x,
            oy + 22.0 * s,
            title_px,
            [0x6c, 0x70, 0x86],
        );
    }
}

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
            let truncated: String =
                token.text.chars().take(remaining).collect();
            let mut trimmed = token.clone();
            trimmed.text = format!("{truncated}…");
            result.push(trimmed);
            remaining = 0;
        }
    }

    result
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
                        color: Color { r: 203, g: 166, b: 247, a: 255 },
                        bold: true,
                        italic: false,
                    },
                    StyledToken {
                        text: " main() {}".to_string(),
                        color: Color { r: 205, g: 214, b: 244, a: 255 },
                        bold: false,
                        italic: false,
                    },
                ],
            },
            HighlightedLine {
                tokens: vec![StyledToken {
                    text: "    println!(\"hello\");".to_string(),
                    color: Color { r: 205, g: 214, b: 244, a: 255 },
                    bold: false,
                    italic: false,
                }],
            },
        ]
    }

    #[test]
    fn font_set_new_succeeds() {
        let _fs = FontSet::new();
    }

    #[test]
    fn calculate_dimensions_macos_two_lines() {
        let (w, h) = calculate_dimensions(2, "macos");
        assert_eq!(w, 864.0); // 800 + 32*2
        // 32 + 36 + 16*2 + 20*2 + 32 = 32+36+32+40+32 = 172
        let expected_h = SHADOW_MARGIN * 2.0
            + TITLE_BAR_HEIGHT
            + PADDING_Y * 2.0
            + LINE_HEIGHT * 2.0;
        assert_eq!(h, expected_h);
    }

    #[test]
    fn calculate_dimensions_none_title_bar() {
        let (_, h) = calculate_dimensions(1, "none");
        let expected_h = SHADOW_MARGIN * 2.0
            + PADDING_Y * 2.0
            + LINE_HEIGHT;
        assert_eq!(h, expected_h);
    }

    #[test]
    fn render_code_pixmap_produces_non_empty() {
        let font_set = FontSet::new();
        let lines = sample_lines();
        let options = CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "macos",
            language: Some("rust"),
            max_line_length: None,
            show_line_numbers: false,
        };
        let pixmap = render_code_pixmap(&lines, &font_set, &options, 2.0)
            .expect("描画に成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque, "描画結果は不透明ピクセルを含むべき");
    }

    #[test]
    fn render_code_pixmap_with_line_numbers() {
        let font_set = FontSet::new();
        let lines = sample_lines();
        let options = CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "macos",
            language: Some("rust"),
            max_line_length: None,
            show_line_numbers: true,
        };
        let pixmap = render_code_pixmap(&lines, &font_set, &options, 2.0)
            .expect("行番号付き描画に成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque);
    }

    #[test]
    fn render_code_pixmap_linux_title_bar() {
        let font_set = FontSet::new();
        let lines = sample_lines();
        let options = CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "linux",
            language: Some("rust"),
            max_line_length: None,
            show_line_numbers: false,
        };
        let pixmap = render_code_pixmap(&lines, &font_set, &options, 2.0)
            .expect("Linux タイトルバー描画に成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque);
    }

    #[test]
    fn render_code_pixmap_plain_title_bar() {
        let font_set = FontSet::new();
        let lines = sample_lines();
        let options = CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "plain",
            language: Some("rust"),
            max_line_length: None,
            show_line_numbers: false,
        };
        let pixmap = render_code_pixmap(&lines, &font_set, &options, 2.0)
            .expect("プレーンタイトルバー描画に成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque);
    }

    #[test]
    fn render_code_pixmap_no_title_bar() {
        let font_set = FontSet::new();
        let lines = sample_lines();
        let options = CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "none",
            language: None,
            max_line_length: None,
            show_line_numbers: false,
        };
        let pixmap = render_code_pixmap(&lines, &font_set, &options, 2.0)
            .expect("タイトルバーなし描画に成功するべき");
        let has_opaque = pixmap.data().chunks(4).any(|px| px[3] > 0);
        assert!(has_opaque);
    }

    #[test]
    fn render_code_pixmap_dimensions_match_svg() {
        let font_set = FontSet::new();
        let lines = sample_lines();
        let options = CanvasOptions {
            bg_color: [0x1e, 0x1e, 0x2e],
            opacity: 0.75,
            title_bar_style: "macos",
            language: Some("rust"),
            max_line_length: None,
            show_line_numbers: false,
        };
        let pixmap = render_code_pixmap(&lines, &font_set, &options, 2.0)
            .expect("描画に成功するべき");
        let (tw, th) = calculate_dimensions(2, "macos");
        assert_eq!(pixmap.width(), (tw * 2.0) as u32);
        assert_eq!(pixmap.height(), (th * 2.0) as u32);
    }

    #[test]
    fn rasterize_cached_returns_same_as_rasterize_char() {
        let font_set = FontSet::new();
        let ch = 'A';
        let px = 28.0;
        let (metrics_direct, bitmap_direct) =
            font_set.rasterize_char(ch, px);
        let (metrics_cached, bitmap_cached) =
            font_set.rasterize_cached(ch, px);
        assert_eq!(metrics_direct.width, metrics_cached.width);
        assert_eq!(metrics_direct.height, metrics_cached.height);
        assert_eq!(
            metrics_direct.advance_width,
            metrics_cached.advance_width
        );
        assert_eq!(bitmap_direct, bitmap_cached.to_vec());
    }

    #[test]
    fn rasterize_cached_hits_cache_on_second_call() {
        let font_set = FontSet::new();
        let ch = 'B';
        let px = 28.0;
        let (m1, b1) = font_set.rasterize_cached(ch, px);
        let (m2, b2) = font_set.rasterize_cached(ch, px);
        assert_eq!(m1.width, m2.width);
        assert_eq!(m1.height, m2.height);
        assert_eq!(b1.as_slice(), b2.as_slice());
    }

    #[test]
    fn font_set_with_hackgen_nf_succeeds() {
        let _fs = FontSet::with_family(FontFamily::HackGenNF);
    }

    #[test]
    fn font_set_with_hackgen_nf_rasterizes_ascii() {
        let fs = FontSet::with_family(FontFamily::HackGenNF);
        let (metrics, bitmap) = fs.rasterize_cached('A', 28.0);
        assert!(metrics.width > 0, "HackGen NF で ASCII 文字を描画できるべき");
        assert!(!bitmap.is_empty());
    }

    #[test]
    fn font_set_with_hackgen_nf_rasterizes_cjk() {
        let fs = FontSet::with_family(FontFamily::HackGenNF);
        let (metrics, bitmap) = fs.rasterize_cached('あ', 28.0);
        assert!(metrics.width > 0, "HackGen NF で日本語文字を描画できるべき");
        assert!(!bitmap.is_empty());
    }

    #[test]
    fn font_set_with_plemoljp_succeeds() {
        let _fs = FontSet::with_family(FontFamily::PlemolJP);
    }

    #[test]
    fn rasterize_cached_fallback_font() {
        let font_set = FontSet::new();
        // 日本語文字はフォールバックフォント（PlemolJP）でラスタライズされる
        let ch = 'あ';
        let px = 28.0;
        let (metrics_direct, bitmap_direct) =
            font_set.rasterize_char(ch, px);
        let (metrics_cached, bitmap_cached) =
            font_set.rasterize_cached(ch, px);
        assert_eq!(metrics_direct.width, metrics_cached.width);
        assert_eq!(bitmap_direct, bitmap_cached.to_vec());
    }

    #[test]
    fn draw_glyph_blends_onto_pixmap() {
        let mut pixmap =
            tiny_skia::Pixmap::new(100, 100).expect("Pixmap作成に成功するべき");
        // 背景を白で塗る
        pixmap.fill(tiny_skia::Color::WHITE);

        let font_set = FontSet::new();
        let (metrics, bitmap) = font_set.rasterize_char('A', 28.0);

        draw_glyph(
            &mut pixmap,
            &bitmap,
            &metrics,
            10,
            10,
            [0, 0, 0], // 黒
        );

        // 黒ピクセルがあるはず
        let has_dark = pixmap.data().chunks(4).any(|px| {
            px[3] > 0 && (px[0] < 200 || px[1] < 200 || px[2] < 200)
        });
        assert!(has_dark, "グリフ描画後にダークピクセルがあるべき");
    }
}
