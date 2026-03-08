/// RGBA カラー
#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// スタイル付きテキストトークン
#[derive(Debug, Clone)]
pub struct StyledToken {
    pub text: String,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
}

/// ハイライト済みの1行
#[derive(Debug, Clone)]
pub struct HighlightedLine {
    pub tokens: Vec<StyledToken>,
}

/// コードをシンタックスハイライトしてトークン列に変換する
pub fn highlight(
    code: &str,
    language: Option<&str>,
    syntax_set: &syntect::parsing::SyntaxSet,
    theme: &syntect::highlighting::Theme,
) -> Vec<HighlightedLine> {
    use syntect::easy::HighlightLines;
    use syntect::highlighting::FontStyle;

    // 言語に対応する構文定義を取得。見つからなければプレーンテキスト
    let syntax = language
        .and_then(|lang| syntax_set.find_syntax_by_token(lang))
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);

    code.lines()
        .map(|line| {
            // syntect は行末に \n を期待する
            let line_with_newline = format!("{line}\n");
            let ranges = highlighter
                .highlight_line(&line_with_newline, syntax_set)
                .unwrap_or_default();

            let tokens = ranges
                .into_iter()
                .map(|(style, text)| {
                    // 末尾の改行を除去
                    let text = text.trim_end_matches('\n').to_string();
                    StyledToken {
                        text,
                        color: Color {
                            r: style.foreground.r,
                            g: style.foreground.g,
                            b: style.foreground.b,
                            a: style.foreground.a,
                        },
                        bold: style.font_style.contains(FontStyle::BOLD),
                        italic: style.font_style.contains(FontStyle::ITALIC),
                    }
                })
                // 空テキストのトークンを除去
                .filter(|t| !t.text.is_empty())
                .collect();

            HighlightedLine { tokens }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::ThemeSet;
    use syntect::parsing::SyntaxSet;

    fn default_syntax_set() -> SyntaxSet {
        syntect::dumps::from_uncompressed_data(super::super::SYNTAX_SET_DUMP)
            .expect("SyntaxSet のデシリアライズに失敗")
    }

    fn default_theme() -> syntect::highlighting::Theme {
        let ts: ThemeSet =
            syntect::dumps::from_uncompressed_data(super::super::THEME_SET_DUMP)
                .expect("ThemeSet のデシリアライズに失敗");
        ts.themes["base16-ocean.dark"].clone()
    }

    #[test]
    fn highlight_rust_code_produces_tokens() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let code = "fn main() {}";
        let lines = highlight(code, Some("rust"), &ss, &theme);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].tokens.is_empty());
    }

    #[test]
    fn highlight_multiline_code() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let lines = highlight(code, Some("rust"), &ss, &theme);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn highlight_python_code_produces_tokens() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let code = "def hello():\n    print('hi')";
        let lines = highlight(code, Some("python"), &ss, &theme);
        assert_eq!(lines.len(), 2);
        assert!(!lines[0].tokens.is_empty());
    }

    #[test]
    fn highlight_unknown_language_falls_back_to_plain_text() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let code = "some plain text";
        let lines = highlight(code, Some("nonexistent_lang_xyz"), &ss, &theme);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].tokens.is_empty());
    }

    #[test]
    fn highlight_no_language_falls_back_to_plain_text() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let code = "just text";
        let lines = highlight(code, None, &ss, &theme);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].tokens.is_empty());
    }

    #[test]
    fn highlight_rust_fn_keyword_is_colored() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let code = "fn main() {}";
        let lines = highlight(code, Some("rust"), &ss, &theme);
        // "fn" キーワードは少なくとも1つのトークンに含まれるはず
        let has_fn = lines[0].tokens.iter().any(|t| t.text.contains("fn"));
        assert!(has_fn, "fn キーワードがトークンに含まれるべき");
    }

    #[test]
    fn highlight_empty_code_returns_empty() {
        let ss = default_syntax_set();
        let theme = default_theme();
        let lines = highlight("", Some("rust"), &ss, &theme);
        assert!(
            lines.is_empty()
                || (lines.len() == 1 && lines[0].tokens.is_empty())
        );
    }
}
