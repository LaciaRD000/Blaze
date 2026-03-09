//! レンダリングパイプラインの各ステップ計測テスト
//! `cargo test --release bench_pipeline -- --nocapture` で実行

use std::time::Instant;

use blaze_bot::renderer::{RenderOptions, Renderer};

/// 計測結果を保持する構造体
struct TimingResult {
    label: &'static str,
    duration_us: u128,
}

impl TimingResult {
    fn new(label: &'static str, duration_us: u128) -> Self {
        Self { label, duration_us }
    }
}

/// 計測結果を表形式で表示する
fn print_results(results: &[TimingResult], total_us: u128) {
    println!("\n{:-<60}", "");
    println!(
        "{:<35} {:>10} {:>10}",
        "ステップ", "時間(μs)", "割合(%)"
    );
    println!("{:-<60}", "");
    for r in results {
        let pct = r.duration_us as f64 / total_us as f64 * 100.0;
        println!("{:<35} {:>10} {:>9.1}%", r.label, r.duration_us, pct);
    }
    println!("{:-<60}", "");
    println!("{:<35} {:>10} {:>9}", "合計", total_us, "100.0%");
    println!("{:-<60}", "");
}

/// 短いコード（3行）のベンチマーク
#[test]
fn bench_pipeline_short_code() {
    let code = "fn main() {\n    println!(\"Hello, world!\");\n}";
    run_benchmark("短いコード (3行)", code, Some("rust"));
}

/// 中程度のコード（20行）のベンチマーク
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

/// 長いコード（50行）のベンチマーク
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

/// ベンチマーク本体: render_with_options の全体計測 + 背景あり/なし比較
fn run_benchmark(label: &str, code: &str, language: Option<&str>) {
    let renderer = Renderer::new();
    let theme_name = "base16-ocean.dark";
    let options_bg = RenderOptions {
        background_image: Some("gradient".to_string()),
        ..Default::default()
    };
    let options_no_bg = RenderOptions::default();

    println!("\n{:=<60}", "");
    println!("  ベンチマーク: {label}");
    println!("  行数: {}", code.lines().count());
    println!("{:=<60}", "");

    // ウォームアップ（キャッシュ効果のため1回捨てる）
    let _ = renderer.render_with_options(code, language, theme_name, &options_bg);
    let _ = renderer.render_with_options(code, language, theme_name, &options_no_bg);

    // --- 背景あり: 3回計測して中央値を取る ---
    let mut bg_times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let png = renderer
            .render_with_options(code, language, theme_name, &options_bg)
            .unwrap();
        let elapsed = t.elapsed().as_micros();
        bg_times.push((elapsed, png.len()));
    }
    bg_times.sort_by_key(|(t, _)| *t);
    let (bg_median, png_size) = bg_times[1];

    // --- 背景なし: 3回計測して中央値を取る ---
    let mut no_bg_times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = renderer
            .render_with_options(code, language, theme_name, &options_no_bg)
            .unwrap();
        no_bg_times.push(t.elapsed().as_micros());
    }
    no_bg_times.sort();
    let no_bg_median = no_bg_times[1];

    let mut results = Vec::new();
    results.push(TimingResult::new("背景あり (中央値)", bg_median));
    results.push(TimingResult::new("背景なし (中央値)", no_bg_median));
    results.push(TimingResult::new(
        "背景処理の差分",
        bg_median.saturating_sub(no_bg_median),
    ));

    println!("     PNG サイズ: {} bytes", png_size);
    print_results(&results, bg_median);
}
