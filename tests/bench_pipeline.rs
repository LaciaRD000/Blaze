//! レンダリングパイプラインの各ステップ計測テスト
//! `cargo test --release bench_pipeline -- --nocapture` で実行

use std::time::Instant;

use blaze_bot::renderer::background;
use blaze_bot::renderer::canvas::{self, CanvasOptions, FontSet};
use blaze_bot::renderer::highlight;
use blaze_bot::renderer::{RenderOptions, Renderer};

fn print_table(results: &[(&str, u128)], total_us: u128) {
    println!("{:-<70}", "");
    println!("{:<40} {:>10} {:>10}", "ステップ", "μs", "%");
    println!("{:-<70}", "");
    for (label, us) in results {
        let pct = *us as f64 / total_us as f64 * 100.0;
        println!("{:<40} {:>10} {:>9.1}%", label, us, pct);
    }
    println!("{:-<70}", "");
    println!("{:<40} {:>10}", "合計", total_us);
}

#[test]
fn bench_pipeline_short_code() {
    run_benchmark(
        "3行",
        "fn main() {\n    println!(\"Hello, world!\");\n}",
        Some("rust"),
    );
}

#[test]
fn bench_pipeline_medium_code() {
    run_benchmark(
        "21行",
        r#"use std::collections::HashMap;

fn main() {
    let mut map = HashMap::new();
    map.insert("key1", 1);
    map.insert("key2", 2);
    map.insert("key3", 3);

    for (key, value) in &map {
        println!("{}: {}", key, value);
    }

    let result = map.get("key1");
    match result {
        Some(v) => println!("Found: {}", v),
        None => println!("Not found"),
    }

    let sum: i32 = map.values().sum();
    println!("Sum: {}", sum);
}"#,
        Some("rust"),
    );
}

#[test]
fn bench_pipeline_long_code() {
    let mut lines = Vec::new();
    lines.push("use std::io;".to_string());
    lines.push("".to_string());
    lines.push("fn main() {".to_string());
    for i in 0..45 {
        lines.push(format!(
            "    let var_{i} = \"value_{i}\"; // コメント {i}"
        ));
    }
    lines.push("    println!(\"done\");".to_string());
    lines.push("}".to_string());
    let code = lines.join("\n");
    run_benchmark("50行", &code, Some("rust"));
}

fn run_benchmark(label: &str, code: &str, language: Option<&str>) {
    let renderer = Renderer::new();
    let theme_name = "base16-ocean.dark";
    let options = RenderOptions {
        background_image: Some("gradient".to_string()),
        ..Default::default()
    };

    // ウォームアップ
    let _ = renderer.render_with_options(code, language, theme_name, &options);

    println!("\n{:=<70}", "");
    println!("  {label} 新パイプライン（直接描画）");
    println!("{:=<70}", "");

    let pipeline_start = Instant::now();

    // 1. highlight（syntect トークン化）
    let t = Instant::now();
    let theme = renderer.theme_set.themes.get(theme_name).unwrap();
    let bg = theme.settings.background.unwrap();
    let highlighted =
        highlight::highlight(code, language, &renderer.syntax_set, theme);
    let t1 = t.elapsed().as_micros();

    // 2. canvas 直接描画（fontdue + tiny_skia）— render_code_onto_pixmap で計測
    let t = Instant::now();
    let canvas_options = CanvasOptions {
        bg_color: [bg.r, bg.g, bg.b],
        opacity: options.opacity as f32,
        title_bar_style: &options.title_bar_style,
        language,
        max_line_length: options.max_line_length,
        show_line_numbers: options.show_line_numbers,
    };
    let (total_w, total_h) = canvas::calculate_dimensions(
        highlighted.len(), canvas_options.title_bar_style,
    );
    let width = (total_w * 2.0) as u32;
    let height = (total_h * 2.0) as u32;
    let mut code_pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    canvas::render_code_onto_pixmap(
        &mut code_pixmap, &highlighted, &renderer.font_set, &canvas_options, 2.0,
    );
    let t2 = t.elapsed().as_micros();

    // 3. 背景 Pixmap 生成
    let t = Instant::now();
    let line_count = code.lines().count().max(1) as u32;
    let window_h = 36 + 16 * 2 + 20 * line_count;
    let total_w = 800 + 32 * 2;
    let total_h = window_h + 32 * 2;
    let blur_margin = (options.blur_radius * 3.0).ceil() as u32;
    let img_w = total_w + blur_margin * 2;
    let img_h = total_h + blur_margin * 2;
    let bg_pixmap = background::load_background_pixmap(
        &renderer.background_cache,
        "gradient",
        img_w,
        img_h,
    )
    .unwrap();
    let t3 = t.elapsed().as_micros();

    // 4. 背景ぼかし（downscale → blur）
    let t = Instant::now();
    let src = bg_pixmap.data();
    let bw = bg_pixmap.width();
    let bh = bg_pixmap.height();
    let mut buf = vec![0u8; src.len()];
    for i in (0..src.len()).step_by(4) {
        let a = src[i + 3];
        if a == 0 { continue; }
        let inv = 255.0 / a as f32;
        buf[i] = (src[i] as f32 * inv).min(255.0) as u8;
        buf[i + 1] = (src[i + 1] as f32 * inv).min(255.0) as u8;
        buf[i + 2] = (src[i + 2] as f32 * inv).min(255.0) as u8;
        buf[i + 3] = a;
    }
    let rgba = image::RgbaImage::from_raw(bw, bh, buf).unwrap();
    let hw = (bw / 2).max(1);
    let hh = (bh / 2).max(1);
    let ds = image::imageops::resize(
        &rgba, hw, hh, image::imageops::FilterType::Triangle,
    );
    let _bg_blurred = image::imageops::blur(&ds, (options.blur_radius / 2.0) as f32);
    let t4 = t.elapsed().as_micros();

    // 5. シャドウ生成（rect + 1/4 downscale + blur）
    let t = Instant::now();
    let (svg_w, svg_h) = canvas::calculate_dimensions(
        code.lines().count(),
        &options.title_bar_style,
    );
    let sw = svg_w as u32;
    let sh = svg_h as u32;
    let mut pm = tiny_skia::Pixmap::new(sw, sh).unwrap();
    let rect = tiny_skia::Rect::from_xywh(
        32.0, 40.0, svg_w - 64.0, svg_h - 64.0,
    ).unwrap();
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(0, 0, 0, 102);
    pm.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    let src = pm.data();
    let mut buf = vec![0u8; src.len()];
    for i in (0..src.len()).step_by(4) {
        let a = src[i + 3];
        if a == 0 { continue; }
        let inv = 255.0 / a as f32;
        buf[i] = (src[i] as f32 * inv).min(255.0) as u8;
        buf[i + 1] = (src[i + 1] as f32 * inv).min(255.0) as u8;
        buf[i + 2] = (src[i + 2] as f32 * inv).min(255.0) as u8;
        buf[i + 3] = a;
    }
    let rgba = image::RgbaImage::from_raw(sw, sh, buf).unwrap();
    let qw = (sw / 4).max(1);
    let qh = (sh / 4).max(1);
    let ds = image::imageops::resize(&rgba, qw, qh, image::imageops::FilterType::Triangle);
    let _shadow = image::imageops::blur(&ds, 4.0);
    let t5 = t.elapsed().as_micros();

    // 6. 合成（draw_pixmap ×3: 背景 + シャドウ + コード）
    let t = Instant::now();
    let width = code_pixmap.width();
    let height = code_pixmap.height();
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    // 実際の production では draw_pixmap を 3 回呼ぶ
    // ここでは final_pixmap にコードを描画して合成コストを計測
    final_pixmap.draw_pixmap(
        0, 0, code_pixmap.as_ref(), &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(), None,
    );
    let t6 = t.elapsed().as_micros();

    // 7. PNG エンコード（png crate 直接: NoFilter + Fast）
    let t = Instant::now();
    let data = final_pixmap.data();
    let mut png_buf = Vec::with_capacity(data.len() + 1024);
    let mut encoder = png::Encoder::new(&mut png_buf, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::FilterType::Sub);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    drop(writer);
    let t7 = t.elapsed().as_micros();

    let total = pipeline_start.elapsed().as_micros();

    println!("  出力: {}x{} px, PNG {} bytes", width, height, png_buf.len());
    print_table(
        &[
            ("1. highlight (syntect)", t1),
            ("2. canvas 直接描画 (fontdue+skia)", t2),
            ("3. 背景Pixmap生成", t3),
            ("4. 背景ぼかし (ds+blur)", t4),
            ("5. シャドウ (rect+ds+blur)", t5),
            ("6. 合成 (draw_pixmap)", t6),
            ("7. PNGエンコード", t7),
        ],
        total,
    );

    // 背景あり end-to-end（3回計測、メディアン）
    let _ = renderer.render_with_options(code, language, theme_name, &options);
    let mut bg_times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = renderer.render_with_options(code, language, theme_name, &options).unwrap();
        bg_times.push(t.elapsed().as_micros());
    }
    bg_times.sort();
    println!("  背景あり e2e: {}μs (median of {:?})", bg_times[1], bg_times);

    // 背景なし end-to-end（5回計測、全数表示）
    // ウォームアップが shadow cache を埋めるため、全て cache hit
    let _ = renderer.render_with_options(code, language, theme_name, &RenderOptions::default());
    let mut nobg_times = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let _ = renderer.render_with_options(code, language, theme_name, &RenderOptions::default()).unwrap();
        nobg_times.push(t.elapsed().as_micros());
    }
    nobg_times.sort();
    println!("  背景なし e2e: {}μs (median of {:?})", nobg_times[2], nobg_times);

    // シャドウ cache miss vs hit 比較（新規 Renderer で計測）
    let fresh = Renderer::new();
    let t = Instant::now();
    let _ = fresh.render_with_options(code, language, theme_name, &RenderOptions::default()).unwrap();
    let first = t.elapsed().as_micros();
    let t = Instant::now();
    let _ = fresh.render_with_options(code, language, theme_name, &RenderOptions::default()).unwrap();
    let second = t.elapsed().as_micros();
    println!("  shadow cache: miss={}μs, hit={}μs, diff={}μs", first, second, first as i128 - second as i128);
}
