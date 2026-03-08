//! レンダリングパイプラインの統合テスト
//! highlight → SVG → PNG の一連の流れを検証する

use blaze_bot::renderer::{RenderOptions, Renderer};

/// Rust コードを PNG に変換できることを検証（デフォルトオプション）
#[test]
fn render_pipeline_rust_code_default_options() {
    let renderer = Renderer::new();
    let code = "fn main() {\n    println!(\"Hello, world!\");\n}";

    let png = renderer
        .render(code, Some("rust"), "base16-ocean.dark")
        .expect("パイプライン全体が成功するべき");

    // PNG マジックバイト
    assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
    // 最低限のサイズがあること
    assert!(png.len() > 1000, "PNG は十分なサイズがあるべき");
}

/// カスタムオプション（Linux タイトルバー + 行番号 + 低 opacity）でレンダリング
#[test]
fn render_pipeline_custom_options() {
    let renderer = Renderer::new();
    let code = "def hello():\n    print('hi')";

    let opts = RenderOptions {
        title_bar_style: "linux".to_string(),
        opacity: 0.5,
        blur_radius: 4.0,
        show_line_numbers: true,
        max_line_length: Some(80),
        background_image: None,
        scale: 2.0,
    };

    let png = renderer
        .render_with_options(code, Some("python"), "base16-ocean.dark", &opts)
        .expect("カスタムオプションでのレンダリングが成功するべき");

    assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

/// 背景画像付きでレンダリングが成功することを検証
#[test]
fn render_pipeline_with_background_image() {
    let renderer = Renderer::new();
    let code = "console.log('test');";

    let opts = RenderOptions {
        background_image: Some("gradient".to_string()),
        ..Default::default()
    };

    let png = renderer
        .render_with_options(code, Some("js"), "base16-ocean.dark", &opts)
        .expect("背景画像付きレンダリングが成功するべき");

    assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

/// 長い行がトリミングされた状態でレンダリングが成功することを検証
#[test]
fn render_pipeline_long_line_trimmed() {
    let renderer = Renderer::new();
    let long_line = format!("let x = \"{}\";", "a".repeat(200));

    let opts = RenderOptions {
        max_line_length: Some(120),
        ..Default::default()
    };

    let svg = renderer
        .render_svg_with_options(
            &long_line,
            Some("rust"),
            "base16-ocean.dark",
            &opts,
        )
        .expect("SVG 生成が成功するべき");

    assert!(svg.contains("…"), "長い行がトリミングされるべき");
}

/// 存在しないテーマでフォールバックが動作することを検証
#[test]
fn render_pipeline_invalid_theme_fallback() {
    let renderer = Renderer::new();
    let code = "puts 'hello'";

    let png = renderer
        .render(code, Some("ruby"), "totally-nonexistent-theme")
        .expect("フォールバックテーマでレンダリングが成功するべき");

    assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

/// 言語指定なし（プレーンテキスト）でレンダリングが成功することを検証
#[test]
fn render_pipeline_no_language() {
    let renderer = Renderer::new();
    let code = "just some plain text\nwith multiple lines";

    let png = renderer
        .render(code, None, "base16-ocean.dark")
        .expect("言語指定なしでもレンダリングが成功するべき");

    assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
}

/// 空文字列でもパニックしないことを検証
#[test]
fn render_pipeline_empty_code() {
    let renderer = Renderer::new();
    let png = renderer
        .render("", None, "base16-ocean.dark")
        .expect("空文字列でもレンダリングが成功するべき");

    assert_eq!(&png[..4], &[0x89, 0x50, 0x4E, 0x47]);
}
