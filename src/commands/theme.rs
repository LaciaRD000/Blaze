use crate::db::models::UserTheme;
use crate::db::{SqliteThemeRepository, ThemeRepository};
use crate::error::BlazeError;
use crate::{Context, Error};

/// テーマ設定を変更
#[poise::command(slash_command, subcommands("set", "preview", "reset"))]
pub async fn theme(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// カラースキーム・背景・ぼかし等を設定
#[poise::command(slash_command)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "カラースキーム"] color_scheme: Option<String>,
    #[description = "背景画像"] background: Option<String>,
    #[description = "ぼかし強度 (0-30)"] blur: Option<f64>,
    #[description = "不透明度 (0.3-1.0)"] opacity: Option<f64>,
    #[description = "タイトルバー (macos/linux)"] title_bar: Option<String>,
) -> Result<(), Error> {
    // パラメータバリデーション
    if let Some(blur) = blur
        && !(0.0..=30.0).contains(&blur)
    {
        return Err(BlazeError::rendering(
            "ぼかし強度は 0〜30 の範囲で指定してください",
        ));
    }
    if let Some(opacity) = opacity
        && !(0.3..=1.0).contains(&opacity)
    {
        return Err(BlazeError::rendering(
            "不透明度は 0.3〜1.0 の範囲で指定してください",
        ));
    }
    if let Some(ref tb) = title_bar
        && tb != "macos"
        && tb != "linux"
    {
        return Err(BlazeError::InvalidTheme(format!(
            "タイトルバーは 'macos' または 'linux' を指定してください: {tb}"
        )));
    }
    if let Some(ref cs) = color_scheme {
        // syntect のテーマに存在するか確認
        if !ctx
            .data()
            .renderer
            .theme_set
            .themes
            .contains_key(cs.as_str())
        {
            let available: Vec<&str> = ctx
                .data()
                .renderer
                .theme_set
                .themes
                .keys()
                .map(|s| s.as_str())
                .collect();
            return Err(BlazeError::InvalidTheme(format!(
                "不明なカラースキーム: {cs}。利用可能: {}",
                available.join(", ")
            )));
        }
    }

    let user_id = ctx.author().id.get() as i64;
    let repo = SqliteThemeRepository::new(ctx.data().db.clone());

    // 既存テーマを取得、なければデフォルトで作成
    let mut theme = repo
        .get_theme(user_id as u64)
        .await?
        .unwrap_or_else(|| UserTheme::with_defaults(user_id));

    // 指定されたフィールドのみ更新
    if let Some(cs) = color_scheme {
        theme.color_scheme = cs;
    }
    if let Some(bg) = background {
        theme.background_id = bg;
    }
    if let Some(b) = blur {
        theme.blur_radius = b;
    }
    if let Some(o) = opacity {
        theme.opacity = o;
    }
    if let Some(tb) = title_bar {
        theme.title_bar_style = tb;
    }

    repo.upsert_theme(&theme).await?;

    ctx.send(
        poise::CreateReply::default()
            .content("テーマを更新しました")
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// サンプルコードの定数
const PREVIEW_CODE: &str = r#"fn main() {
    let greeting = "Hello, world!";
    println!("{greeting}");
}"#;

/// 現在のテーマでサンプルコードをプレビュー
#[poise::command(slash_command)]
pub async fn preview(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let user_id = ctx.author().id.get() as i64;
    let repo = SqliteThemeRepository::new(ctx.data().db.clone());

    let theme = repo
        .get_theme(user_id as u64)
        .await?
        .unwrap_or_else(|| UserTheme::with_defaults(user_id));

    let renderer = std::sync::Arc::clone(&ctx.data().renderer);
    let theme_name = theme.color_scheme.clone();

    let png = tokio::task::spawn_blocking(move || {
        renderer.render(PREVIEW_CODE, Some("rust"), &theme_name)
    })
    .await
    .map_err(|e| BlazeError::rendering(e.to_string()))??;

    let attachment =
        poise::serenity_prelude::CreateAttachment::bytes(png, "preview.png");
    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .ephemeral(true),
    )
    .await?;

    Ok(())
}

/// テーマをデフォルトにリセット
#[poise::command(slash_command)]
pub async fn reset(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let repo = SqliteThemeRepository::new(ctx.data().db.clone());

    repo.delete_theme(user_id).await?;

    ctx.send(
        poise::CreateReply::default()
            .content("テーマをデフォルトにリセットしました")
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
