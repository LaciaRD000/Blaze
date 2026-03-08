use std::sync::Arc;

use poise::serenity_prelude as serenity;
use regex::Regex;

use crate::db::models::UserTheme;
use crate::db::{PgThemeRepository, ThemeRepository};
use crate::error::BlazeError;
use crate::sanitize::sanitize_code;
use crate::{Context, Error};

pub struct CodeBlock {
    pub language: Option<String>,
    pub code: String,
}

impl CodeBlock {
    /// 制御文字除去・Unicode正規化を適用した新しい CodeBlock を返す
    pub fn sanitized(&self) -> Self {
        Self {
            language: self.language.clone(),
            code: sanitize_code(&self.code),
        }
    }
}

/// メッセージ本文から最初のコードブロックを抽出する
/// 正規表現: ```(\w*)\n([\s\S]*?)```
pub fn extract_code_block(content: &str) -> Option<CodeBlock> {
    let re = Regex::new(r"```(\w*)\n([\s\S]*?)```")
        .expect("正規表現のコンパイルに失敗");
    let caps = re.captures(content)?;

    let language = caps.get(1).and_then(|m| {
        let lang = m.as_str();
        if lang.is_empty() {
            None
        } else {
            Some(lang.to_string())
        }
    });

    let code = caps
        .get(2)
        .map(|m| m.as_str().trim_end_matches('\n').to_string())
        .unwrap_or_default();

    Some(CodeBlock { language, code })
}

/// コンテキストメニュー「ターミナル画像化」
/// Phase 2 で実画像生成に接続する。現在はスタブ（テキスト返信）。
#[poise::command(
    context_menu_command = "ターミナル画像化",
    category = "Render"
)]
pub async fn render_message(
    ctx: Context<'_>,
    #[description = "対象メッセージ"] msg: serenity::Message,
) -> Result<(), Error> {
    let start = std::time::Instant::now();

    // 0. レート制限チェック
    let user_id_for_rate = ctx.author().id.get();
    if ctx
        .data()
        .rate_limiter
        .check_key(&user_id_for_rate)
        .is_err()
    {
        tracing::warn!(user_id = user_id_for_rate, "レート制限超過");
        ctx.send(
            poise::CreateReply::default()
                .content("レート制限に達しました。しばらくお待ちください。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // 1. コードブロック抽出
    let code_block = match extract_code_block(&msg.content) {
        Some(block) => block,
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .content("メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    // 2. 入力バリデーション
    let settings = &ctx.data().settings;
    if code_block.code.lines().count() > settings.max_code_lines
        || code_block.code.len() > settings.max_code_chars
    {
        ctx.send(
            poise::CreateReply::default()
                .content(format!(
                    "コードが長すぎます（上限: {}行 / {}文字）",
                    settings.max_code_lines, settings.max_code_chars
                ))
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    // バリデーション通過 — ここから時間がかかるので defer する（非エフェメラル）
    ctx.defer().await?;

    // 3. 入力サニタイズ
    let code_block = code_block.sanitized();

    // 4. ユーザーテーマ取得（DB障害時はデフォルトにフォールバック）
    let user_id = ctx.author().id.get() as i64;
    let theme = {
        let repo = PgThemeRepository::new(ctx.data().db.clone());
        repo.get_theme(user_id as u64)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| UserTheme::with_defaults(user_id))
    };

    // 5. Semaphore で同時実行数を制御してレンダリング
    let _permit = ctx
        .data()
        .render_semaphore
        .acquire()
        .await
        .map_err(|e| BlazeError::rendering(e.to_string()))?;

    let renderer = Arc::clone(&ctx.data().renderer);
    let code = code_block.code.clone();
    let language = code_block.language.clone();
    let theme_name = theme.color_scheme.clone();
    let render_options = crate::renderer::RenderOptions {
        title_bar_style: theme.title_bar_style.clone(),
        opacity: theme.opacity,
        blur_radius: theme.blur_radius,
        show_line_numbers: theme.show_line_numbers != 0,
    };

    let png = tokio::task::spawn_blocking(move || {
        renderer.render_with_options(
            &code,
            language.as_deref(),
            &theme_name,
            &render_options,
        )
    })
    .await
    .map_err(|e| BlazeError::rendering(e.to_string()))??;

    drop(_permit);

    let elapsed = start.elapsed();
    tracing::info!(
        user_id = user_id_for_rate,
        elapsed_ms = elapsed.as_millis() as u64,
        "レンダリング完了"
    );

    // 6. 画像をリプライとして送信
    let attachment = serenity::CreateAttachment::bytes(png, "code.png");
    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .reply(true),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_empty_input_returns_none() {
        assert!(extract_code_block("").is_none());
    }

    #[test]
    fn extract_no_code_block_returns_none() {
        assert!(extract_code_block("hello world").is_none());
    }

    #[test]
    fn extract_with_language_tag() {
        let input = "```rust\nfn main() {}\n```";
        let block =
            extract_code_block(input).expect("コードブロックが見つかるべき");
        assert_eq!(block.language.as_deref(), Some("rust"));
        assert_eq!(block.code, "fn main() {}");
    }

    #[test]
    fn extract_without_language_tag() {
        let input = "```\nhello world\n```";
        let block =
            extract_code_block(input).expect("コードブロックが見つかるべき");
        assert!(block.language.is_none());
        assert_eq!(block.code, "hello world");
    }

    #[test]
    fn extract_multiple_blocks_returns_first() {
        let input = "```rust\nfirst\n```\nsome text\n```python\nsecond\n```";
        let block =
            extract_code_block(input).expect("コードブロックが見つかるべき");
        assert_eq!(block.language.as_deref(), Some("rust"));
        assert_eq!(block.code, "first");
    }

    #[test]
    fn extract_multiline_code() {
        let input =
            "```js\nconst x = 1;\nconst y = 2;\nconsole.log(x + y);\n```";
        let block =
            extract_code_block(input).expect("コードブロックが見つかるべき");
        assert_eq!(block.language.as_deref(), Some("js"));
        assert_eq!(
            block.code,
            "const x = 1;\nconst y = 2;\nconsole.log(x + y);"
        );
    }

    #[test]
    fn extract_with_surrounding_text() {
        let input = "Check this out:\n```python\nprint('hi')\n```\nCool right?";
        let block =
            extract_code_block(input).expect("コードブロックが見つかるべき");
        assert_eq!(block.language.as_deref(), Some("python"));
        assert_eq!(block.code, "print('hi')");
    }

    #[test]
    fn extract_unclosed_block_returns_none() {
        let input = "```rust\nfn main() {}";
        assert!(extract_code_block(input).is_none());
    }

    #[test]
    fn sanitized_applies_sanitize_code() {
        let block = CodeBlock {
            language: Some("rust".to_string()),
            code: "hello\x00world".to_string(),
        };
        let sanitized = block.sanitized();
        assert_eq!(sanitized.code, "helloworld");
        assert_eq!(sanitized.language.as_deref(), Some("rust"));
    }
}
