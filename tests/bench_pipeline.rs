//! レンダリングパイプラインの各ステップ計測テスト
//! `cargo test --release bench_pipeline -- --nocapture` で実行

use std::sync::Arc;
use std::time::Instant;

use blaze_bot::renderer::background;
use blaze_bot::renderer::highlight;
use blaze_bot::renderer::svg_builder;
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
    println!("  {label} 背景あり");
    println!("{:=<70}", "");

    let pipeline_start = Instant::now();

    // 1. highlight
    let t = Instant::now();
    let theme = renderer.theme_set.themes.get(theme_name).unwrap();
    let bg_color = {
        let bg = theme.settings.background.unwrap();
        format!("#{:02x}{:02x}{:02x}", bg.r, bg.g, bg.b)
    };
    let highlighted =
        highlight::highlight(code, language, &renderer.syntax_set, theme);
    let t1 = t.elapsed().as_micros();

    // 2. SVG 文字列生成
    let t = Instant::now();
    let svg = svg_builder::build_svg(
        &highlighted,
        &svg_builder::SvgOptions {
            bg_color: &bg_color,
            language,
            title_bar_style: &options.title_bar_style,
            opacity: options.opacity,
            max_line_length: options.max_line_length,
            show_line_numbers: options.show_line_numbers,
        },
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

    // 4. usvg パース
    let t = Instant::now();
    let usvg_opts = resvg::usvg::Options {
        fontdb: Arc::clone(&renderer.font_db),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &usvg_opts).unwrap();
    let t4 = t.elapsed().as_micros();

    // 5. resvg ラスタライズ
    let size = tree.size();
    let width = (size.width() * 2.0) as u32;
    let height = (size.height() * 2.0) as u32;

    let t = Instant::now();
    let mut code_pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut code_pixmap.as_mut(),
    );
    let t5 = t.elapsed().as_micros();

    // 6. シャドウ生成（プロダクションコードと同じ処理）
    let t = Instant::now();
    let shadow = {
        let sw = size.width() as u32;
        let sh = size.height() as u32;
        let mut pm = tiny_skia::Pixmap::new(sw, sh).unwrap();
        let rect = tiny_skia::Rect::from_xywh(
            32.0,
            40.0,
            size.width() - 64.0,
            size.height() - 64.0,
        )
        .unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(0, 0, 0, 102);
        pm.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
        // premul→straight
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
        let ds = image::imageops::resize(
            &rgba, qw, qh, image::imageops::FilterType::Triangle,
        );
        let bl = image::imageops::blur(&ds, 4.0); // 16/4
        image::imageops::resize(
            &bl, sw, sh, image::imageops::FilterType::Triangle,
        )
    };
    let t6 = t.elapsed().as_micros();

    // 7. 背景ぼかし（プロダクションコードと同じ処理: downscale → blur, upscaleなし）
    let t = Instant::now();
    let bg_blurred = {
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
        image::imageops::blur(&ds, (options.blur_radius / 2.0) as f32)
        // upscale なし（draw_pixmap で拡大）
    };
    let t7 = t.elapsed().as_micros();

    // 8. 合成
    let t = Instant::now();
    let mut final_pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    // 背景(4xスケール) → シャドウ(2xスケール) → コード
    // 背景: straight→premul してから draw（ここではダミー計測）
    let _bg_pm = {
        let w = bg_blurred.width();
        let h = bg_blurred.height();
        let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
        let s = bg_blurred.as_raw();
        let d = pm.data_mut();
        for i in (0..s.len()).step_by(4) {
            let a = s[i + 3];
            let alpha = a as f32 / 255.0;
            d[i] = (s[i] as f32 * alpha + 0.5) as u8;
            d[i + 1] = (s[i + 1] as f32 * alpha + 0.5) as u8;
            d[i + 2] = (s[i + 2] as f32 * alpha + 0.5) as u8;
            d[i + 3] = a;
        }
        pm
    };
    let bg_draw_scale = 4.0_f32;
    let off = -(blur_margin as f32) * 2.0;
    final_pixmap.draw_pixmap(
        0, 0, _bg_pm.as_ref(), &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(bg_draw_scale, bg_draw_scale)
            .pre_translate(off / bg_draw_scale, off / bg_draw_scale),
        None,
    );
    // シャドウ straight→premul
    let _sh_pm = {
        let w = shadow.width();
        let h = shadow.height();
        let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
        let s = shadow.as_raw();
        let d = pm.data_mut();
        for i in (0..s.len()).step_by(4) {
            let a = s[i + 3];
            let alpha = a as f32 / 255.0;
            d[i] = (s[i] as f32 * alpha + 0.5) as u8;
            d[i + 1] = (s[i + 1] as f32 * alpha + 0.5) as u8;
            d[i + 2] = (s[i + 2] as f32 * alpha + 0.5) as u8;
            d[i + 3] = a;
        }
        pm
    };
    final_pixmap.draw_pixmap(
        0, 0, _sh_pm.as_ref(), &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_scale(2.0, 2.0), None,
    );
    final_pixmap.draw_pixmap(
        0, 0, code_pixmap.as_ref(), &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(), None,
    );
    let t8 = t.elapsed().as_micros();

    // 9. PNG エンコード
    let t = Instant::now();
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut png_buf = Vec::new();
    PngEncoder::new_with_quality(&mut png_buf, CompressionType::Fast, FilterType::Sub)
        .write_image(
            final_pixmap.data(),
            width, height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let t9 = t.elapsed().as_micros();

    let total = pipeline_start.elapsed().as_micros();

    println!("  出力: {}x{} px, PNG {} bytes", width, height, png_buf.len());
    print_table(
        &[
            ("1. highlight", t1),
            ("2. SVG文字列生成", t2),
            ("3. 背景Pixmap生成", t3),
            ("4. usvg パース+フォント解決", t4),
            ("5. resvg ラスタライズ (2x)", t5),
            ("6. シャドウ (rect+blur+resize)", t6),
            ("7. 背景ぼかし (ds+blur)", t7),
            ("8. 合成 (premul+draw×3)", t8),
            ("9. PNGエンコード", t9),
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

    // 背景なし end-to-end（3回計測、メディアン）
    let _ = renderer.render_with_options(code, language, theme_name, &RenderOptions::default());
    let mut nobg_times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = renderer.render_with_options(code, language, theme_name, &RenderOptions::default()).unwrap();
        nobg_times.push(t.elapsed().as_micros());
    }
    nobg_times.sort();
    println!("  背景なし e2e: {}μs (median of {:?})", nobg_times[1], nobg_times);
}

/// e2e のみの軽量ベンチ（ステップ別計測を除外し、安定した計測を行う）
#[test]
fn bench_e2e_only() {
    let renderer = Renderer::new();
    let theme_name = "base16-ocean.dark";

    let codes: Vec<(&str, String)> = vec![
        ("3行", "fn main() {\n    println!(\"Hello, world!\");\n}".to_string()),
        ("21行", {
            let mut lines = Vec::new();
            lines.push("use std::collections::HashMap;".to_string());
            lines.push("".to_string());
            lines.push("fn main() {".to_string());
            for i in 0..16 {
                lines.push(format!("    let v{i} = {i};"));
            }
            lines.push("    println!(\"done\");".to_string());
            lines.push("}".to_string());
            lines.join("\n")
        }),
        ("50行", {
            let mut lines = Vec::new();
            lines.push("use std::io;".to_string());
            lines.push("".to_string());
            lines.push("fn main() {".to_string());
            for i in 0..45 {
                lines.push(format!("    let var_{i} = \"value_{i}\"; // コメント {i}"));
            }
            lines.push("    println!(\"done\");".to_string());
            lines.push("}".to_string());
            lines.join("\n")
        }),
    ];

    let bg_opts = RenderOptions {
        background_image: Some("gradient".to_string()),
        ..Default::default()
    };

    println!("\n{:=<60}", "");
    println!("  e2e ベンチ（5回計測、メディアン）");
    println!("{:=<60}", "");

    for (label, code) in &codes {
        // ウォームアップ
        let _ = renderer.render_with_options(code, Some("rust"), theme_name, &bg_opts);
        let _ = renderer.render_with_options(code, Some("rust"), theme_name, &RenderOptions::default());

        // 背景あり 5回
        let mut bg_times: Vec<u128> = (0..5).map(|_| {
            let t = Instant::now();
            let _ = renderer.render_with_options(code, Some("rust"), theme_name, &bg_opts).unwrap();
            t.elapsed().as_micros()
        }).collect();
        bg_times.sort();

        // 背景なし 5回
        let mut nobg_times: Vec<u128> = (0..5).map(|_| {
            let t = Instant::now();
            let _ = renderer.render_with_options(code, Some("rust"), theme_name, &RenderOptions::default()).unwrap();
            t.elapsed().as_micros()
        }).collect();
        nobg_times.sort();

        println!(
            "  {label:<6} 背景あり: {:>6}μs  背景なし: {:>6}μs",
            bg_times[2], nobg_times[2]
        );
    }
    println!("{:=<60}", "");
}
