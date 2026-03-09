//! レンダリングパイプラインの計測テスト
//! `cargo test --release bench_pipeline -- --nocapture` で実行

use std::time::Instant;

use blaze_bot::renderer::{RenderOptions, Renderer};

/// 短いコード（3行）
#[test]
fn bench_pipeline_short_code() {
    let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
    run_benchmark("短いコード (3行)", code, Some("rust"));
}

/// 中程度のコード（20行）
#[test]
fn bench_pipeline_medium_code() {
    let code = r#"use std::collections::HashMap;

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
}"#;
    run_benchmark("中程度のコード (20行)", code, Some("rust"));
}

/// 長いコード（50行）
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
    run_benchmark("長いコード (50行)", &code, Some("rust"));
}

/// render_with_options の end-to-end 計測（3回中央値）
fn run_benchmark(label: &str, code: &str, language: Option<&str>) {
    let renderer = Renderer::new();
    let theme_name = "base16-ocean.dark";
    let opts_bg = RenderOptions {
        background_image: Some("gradient".to_string()),
        ..Default::default()
    };
    let opts_no_bg = RenderOptions::default();

    println!("\n{:=<60}", "");
    println!("  {label} (行数: {})", code.lines().count());
    println!("{:=<60}", "");

    // ウォームアップ
    let _ = renderer.render_with_options(code, language, theme_name, &opts_bg);
    let _ = renderer.render_with_options(code, language, theme_name, &opts_no_bg);

    // 背景あり: 3回計測 → 中央値
    let mut bg_times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let png = renderer
            .render_with_options(code, language, theme_name, &opts_bg)
            .unwrap();
        bg_times.push((t.elapsed().as_micros(), png.len()));
    }
    bg_times.sort_by_key(|(t, _)| *t);
    let (bg_us, png_size) = bg_times[1];

    // 背景なし: 3回計測 → 中央値
    let mut no_bg_times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = renderer
            .render_with_options(code, language, theme_name, &opts_no_bg)
            .unwrap();
        no_bg_times.push(t.elapsed().as_micros());
    }
    no_bg_times.sort();
    let no_bg_us = no_bg_times[1];

    println!("  背景あり: {}μs  (PNG {}bytes)", bg_us, png_size);
    println!("  背景なし: {}μs", no_bg_us);
    println!(
        "  背景処理差分: {}μs",
        bg_us.saturating_sub(no_bg_us)
    );
}
