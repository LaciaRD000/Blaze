//! 画像生成テスト: 実際のPNG画像をファイルに出力して目視確認する
//! `cargo test --release gen_image -- --nocapture` で実行

use blaze_bot::renderer::{RenderOptions, Renderer};

#[test]
fn gen_test_images() {
    let renderer = Renderer::new();
    let code = r#"use std::collections::HashMap;

fn main() {
    let mut map = HashMap::new();
    map.insert("key1", 1);
    map.insert("key2", 2);

    for (key, value) in &map {
        println!("{}: {}", key, value);
    }
}"#;

    // 背景なし macOS
    let png = renderer
        .render_with_options(code, Some("rust"), "base16-ocean.dark", &RenderOptions::default())
        .unwrap();
    std::fs::write("/tmp/blaze_nobg_macos.png", &png).unwrap();
    println!("生成: /tmp/blaze_nobg_macos.png ({} bytes)", png.len());

    // 背景あり macOS
    let opts = RenderOptions {
        background_image: Some("gradient".to_string()),
        ..Default::default()
    };
    let png = renderer
        .render_with_options(code, Some("rust"), "base16-ocean.dark", &opts)
        .unwrap();
    std::fs::write("/tmp/blaze_bg_macos.png", &png).unwrap();
    println!("生成: /tmp/blaze_bg_macos.png ({} bytes)", png.len());

    // Linux タイトルバー
    let opts = RenderOptions {
        title_bar_style: "linux".to_string(),
        ..Default::default()
    };
    let png = renderer
        .render_with_options(code, Some("rust"), "base16-ocean.dark", &opts)
        .unwrap();
    std::fs::write("/tmp/blaze_nobg_linux.png", &png).unwrap();
    println!("生成: /tmp/blaze_nobg_linux.png ({} bytes)", png.len());

    // 行番号付き
    let opts = RenderOptions {
        show_line_numbers: true,
        ..Default::default()
    };
    let png = renderer
        .render_with_options(code, Some("rust"), "base16-ocean.dark", &opts)
        .unwrap();
    std::fs::write("/tmp/blaze_line_numbers.png", &png).unwrap();
    println!("生成: /tmp/blaze_line_numbers.png ({} bytes)", png.len());
}
