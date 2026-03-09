/// ビルド時に syntect の SyntaxSet / ThemeSet をバイナリダンプする
/// ランタイムでは uncompressed データから直接デシリアライズし、
/// デフォルト構文定義の再コンパイルをスキップして起動を高速化する
///
/// SyntaxSet は two-face 経由でロードし、デフォルトに含まれない
/// TypeScript, Kotlin, Swift, TOML 等のモダンな言語もサポートする
fn main() {
    let out_dir =
        std::env::var("OUT_DIR").expect("OUT_DIR が設定されていません");
    let out_path = std::path::Path::new(&out_dir);

    // SyntaxSet: two-face の拡張構文定義（デフォルト + 80言語以上）をダンプ
    let syntax_set = two_face::syntax::extra_newlines();
    syntect::dumps::dump_to_uncompressed_file(
        &syntax_set,
        out_path.join("syntax_set.packdump"),
    )
    .expect("SyntaxSet のダンプに失敗");

    // ThemeSet: デフォルトテーマをロードしてダンプ
    let theme_set =
        syntect::highlighting::ThemeSet::load_defaults();
    syntect::dumps::dump_to_uncompressed_file(
        &theme_set,
        out_path.join("theme_set.packdump"),
    )
    .expect("ThemeSet のダンプに失敗");

    println!("cargo:rerun-if-changed=build.rs");
}
