use crate::error::BlazeError;
use crate::Data;

/// グローバルエラーハンドラ
/// BlazeError をユーザー向けのエフェメラルメッセージに変換する
pub async fn on_error(error: poise::FrameworkError<'_, Data, BlazeError>) {
    match error {
        poise::FrameworkError::Command { error, ctx, .. } => {
            let user_message = match &error {
                // ユーザー起因のエラー — そのまま表示
                BlazeError::CodeBlockNotFound
                | BlazeError::CodeTooLong { .. }
                | BlazeError::RateLimitExceeded
                | BlazeError::InvalidTheme(_) => error.to_string(),

                // 内部エラー — 詳細はログのみ、ユーザーには汎用メッセージ
                BlazeError::Database(_)
                | BlazeError::Rendering { .. }
                | BlazeError::Config(_) => {
                    tracing::error!("内部エラー: {error:?}");
                    "内部エラーが発生しました。しばらくしてからお試しください。"
                        .to_string()
                }
            };
            let _ = ctx
                .send(
                    poise::CreateReply::default()
                        .content(user_message)
                        .ephemeral(true),
                )
                .await;
        }
        other => {
            let _ = poise::builtins::on_error(other).await;
        }
    }
}
