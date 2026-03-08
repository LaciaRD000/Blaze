use unicode_normalization::UnicodeNormalization;

/// コードの入力サニタイズ
/// - 制御文字を除去する（タブ・改行は保持）
/// - タブを半角スペース4つに展開する
/// - Unicode NFC 正規化を適用する
pub fn sanitize_code(code: &str) -> String {
    let filtered: String = code
        .chars()
        .filter(|c| {
            // 改行は保持
            if *c == '\n' || *c == '\r' {
                return true;
            }
            // タブは保持（後でスペースに展開）
            if *c == '\t' {
                return true;
            }
            // ゼロ幅文字を除去
            if matches!(*c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}') {
                return false;
            }
            // その他の制御文字を除去
            !c.is_control()
        })
        .collect();

    // タブを半角スペース4つに展開
    let expanded = filtered.replace('\t', "    ");

    // Unicode NFC 正規化
    expanded.nfc().collect()
}

/// SVG出力時の特殊文字エスケープ (&, <, >, ")
pub fn escape_for_svg(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- sanitize_code ---

    #[test]
    fn sanitize_removes_control_chars() {
        let input = "hello\x00world\x07\x08";
        let result = sanitize_code(input);
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn sanitize_preserves_tab_and_newline() {
        let input = "line1\n\tline2\r\n";
        let result = sanitize_code(input);
        // タブはスペース4つに展開、改行は保持
        assert!(result.contains('\n'));
        assert!(result.contains("    ")); // タブ → 4スペース
        assert!(!result.contains('\t'));
    }

    #[test]
    fn sanitize_expands_tabs_to_four_spaces() {
        let input = "\tindented\t\ttwice";
        let result = sanitize_code(input);
        assert_eq!(result, "    indented        twice");
    }

    #[test]
    fn sanitize_applies_nfc_normalization() {
        // が (U+304B U+3099) → が (U+304C) NFC正規化
        let input = "\u{304B}\u{3099}";
        let result = sanitize_code(input);
        assert_eq!(result, "\u{304C}");
    }

    #[test]
    fn sanitize_removes_zero_width_chars() {
        let input = "hello\u{200B}world\u{FEFF}";
        let result = sanitize_code(input);
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn sanitize_preserves_normal_code() {
        let input = "fn main() {\n    println!(\"Hello\");\n}";
        let result = sanitize_code(input);
        assert_eq!(result, input);
    }

    // --- escape_for_svg ---

    #[test]
    fn escape_ampersand() {
        assert_eq!(escape_for_svg("a&b"), "a&amp;b");
    }

    #[test]
    fn escape_less_than() {
        assert_eq!(escape_for_svg("a<b"), "a&lt;b");
    }

    #[test]
    fn escape_greater_than() {
        assert_eq!(escape_for_svg("a>b"), "a&gt;b");
    }

    #[test]
    fn escape_double_quote() {
        assert_eq!(escape_for_svg("a\"b"), "a&quot;b");
    }

    #[test]
    fn escape_multiple_special_chars() {
        assert_eq!(
            escape_for_svg("<div class=\"test\">&</div>"),
            "&lt;div class=&quot;test&quot;&gt;&amp;&lt;/div&gt;"
        );
    }

    #[test]
    fn escape_no_special_chars() {
        assert_eq!(escape_for_svg("hello world"), "hello world");
    }
}
