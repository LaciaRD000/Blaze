use std::sync::Arc;

use crate::{
    Context, Error,
    db::{PgThemeRepository, ThemeRepository, models::UserTheme},
    error::BlazeError,
};

/// タイトルバースタイルの選択肢
#[derive(Debug, Clone, poise::ChoiceParameter)]
pub enum TitleBarStyle {
    #[name = "macOS"]
    Macos,
    #[name = "linux"]
    Linux,
    #[name = "plain"]
    Plain,
    #[name = "none"]
    None,
}

/// フォントの選択肢
#[derive(Debug, Clone, poise::ChoiceParameter)]
pub enum FontChoice {
    #[name = "Fira Code"]
    FiraCode,
    #[name = "PlemolJP"]
    PlemolJP,
    #[name = "HackGen NF"]
    HackGenNF,
}

/// カラースキームの選択肢（syntect デフォルトテーマ）
#[derive(Debug, Clone, poise::ChoiceParameter)]
pub enum ColorSchemeChoice {
    #[name = "base16-ocean.dark"]
    Base16OceanDark,
    #[name = "base16-eighties.dark"]
    Base16EightiesDark,
    #[name = "base16-mocha.dark"]
    Base16MochaDark,
    #[name = "base16-ocean.light"]
    Base16OceanLight,
    #[name = "InspiredGitHub"]
    InspiredGitHub,
    #[name = "Solarized (dark)"]
    SolarizedDark,
    #[name = "Solarized (light)"]
    SolarizedLight,
}

impl ColorSchemeChoice {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Base16OceanDark => "base16-ocean.dark",
            Self::Base16EightiesDark => "base16-eighties.dark",
            Self::Base16MochaDark => "base16-mocha.dark",
            Self::Base16OceanLight => "base16-ocean.light",
            Self::InspiredGitHub => "InspiredGitHub",
            Self::SolarizedDark => "Solarized (dark)",
            Self::SolarizedLight => "Solarized (light)",
        }
    }
}

/// レンダリングスケールの選択肢
#[derive(Debug, Clone, poise::ChoiceParameter)]
pub enum RenderScaleChoice {
    #[name = "1x（高速）"]
    Scale1x,
    #[name = "2x（高解像度）"]
    Scale2x,
}

/// 背景画像の選択肢
#[derive(Debug, Clone, poise::ChoiceParameter)]
pub enum BackgroundChoice {
    #[name = "none"]
    None,
    #[name = "gradient"]
    Gradient,
    #[name = "denim"]
    Denim,
    #[name = "repeated-square-dark"]
    RepeatedSquareDark,
}

/// テーマ設定を変更
#[poise::command(slash_command, subcommands("set", "preview", "reset"))]
pub async fn theme(_ctx: Context<'_>) -> Result<(), Error> { Ok(()) }

/// カラースキーム・背景・ぼかし等を設定
// poise のスラッシュコマンドは各パラメータが引数になるため抑制
#[allow(clippy::too_many_arguments)]
#[poise::command(slash_command)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "カラースキーム"] color_scheme: Option<ColorSchemeChoice>,
    #[description = "背景画像"] background: Option<BackgroundChoice>,
    #[description = "ぼかし強度 (0-30)"] blur: Option<f64>,
    #[description = "不透明度 (0.3-1.0)"] opacity: Option<f64>,
    #[description = "タイトルバー"] title_bar: Option<TitleBarStyle>,
    #[description = "フォント"] font: Option<FontChoice>,
    #[description = "行番号表示 (true/false)"] show_line_numbers: Option<bool>,
    #[description = "解像度スケール"] scale: Option<RenderScaleChoice>,
) -> Result<(), Error> {
    // パラメータバリデーション
    if let Some(blur) = blur
        && !(0.0..=30.0).contains(&blur)
    {
        return Err(BlazeError::InvalidTheme(
            "ぼかし強度は 0〜30 の範囲で指定してください".to_string(),
        ));
    }
    if let Some(opacity) = opacity
        && !(0.3..=1.0).contains(&opacity)
    {
        return Err(BlazeError::InvalidTheme(
            "不透明度は 0.3〜1.0 の範囲で指定してください".to_string(),
        ));
    }
    let user_id = ctx.author().id.get() as i64;
    let repo = PgThemeRepository::new(ctx.data().db.clone());

    // 既存テーマを取得、なければデフォルトで作成
    let mut theme = repo
        .get_theme(user_id as u64)
        .await?
        .unwrap_or_else(|| UserTheme::with_defaults(user_id));

    // 指定されたフィールドのみ更新
    if let Some(cs) = color_scheme {
        theme.color_scheme = cs.as_str().to_string();
    }
    if let Some(bg) = background {
        theme.background_id = match bg {
            BackgroundChoice::None => "none".to_string(),
            BackgroundChoice::Gradient => "gradient".to_string(),
            BackgroundChoice::Denim => "denim".to_string(),
            BackgroundChoice::RepeatedSquareDark => "repeated-square-dark".to_string(),
        };
    }
    if let Some(b) = blur {
        theme.blur_radius = b;
    }
    if let Some(o) = opacity {
        theme.opacity = o;
    }
    if let Some(tb) = title_bar {
        theme.title_bar_style = match tb {
            TitleBarStyle::Macos => "macos".to_string(),
            TitleBarStyle::Linux => "linux".to_string(),
            TitleBarStyle::Plain => "plain".to_string(),
            TitleBarStyle::None => "none".to_string(),
        };
    }
    if let Some(f) = font {
        theme.font_family = match f {
            FontChoice::FiraCode => "Fira Code".to_string(),
            FontChoice::PlemolJP => "PlemolJP".to_string(),
            FontChoice::HackGenNF => "HackGen Console NF".to_string(),
        };
    }
    if let Some(sln) = show_line_numbers {
        theme.show_line_numbers = if sln { 1 } else { 0 };
    }
    if let Some(s) = scale {
        theme.render_scale = match s {
            RenderScaleChoice::Scale1x => 1,
            RenderScaleChoice::Scale2x => 2,
        };
    }

    repo.upsert_theme(&theme).await?;

    tracing::info!(
        user_id,
        color_scheme = %theme.color_scheme,
        "テーマ更新"
    );

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
    // レート制限チェック
    let user_id_for_rate = ctx.author().id.get();
    if ctx
        .data()
        .rate_limiter
        .check_key(&user_id_for_rate)
        .is_err()
    {
        tracing::warn!(
            user_id = user_id_for_rate,
            "プレビュー: レート制限超過"
        );
        return Err(BlazeError::RateLimitExceeded);
    }

    ctx.defer_ephemeral().await?;

    let user_id = ctx.author().id.get() as i64;
    let repo = PgThemeRepository::new(ctx.data().db.clone());

    let theme = repo
        .get_theme(user_id as u64)
        .await?
        .unwrap_or_else(|| UserTheme::with_defaults(user_id));

    // Semaphore で同時実行数を制御
    let _permit = ctx
        .data()
        .render_semaphore
        .acquire()
        .await
        .map_err(|e| BlazeError::rendering(e.to_string()))?;

    let renderer = Arc::clone(&ctx.data().renderer);
    let theme_name = theme.color_scheme.clone();
    let max_line_length = ctx.data().settings.max_line_length;
    let render_options = crate::renderer::RenderOptions {
        title_bar_style: theme.title_bar_style.clone(),
        opacity: theme.opacity,
        blur_radius: theme.blur_radius,
        show_line_numbers: theme.show_line_numbers != 0,
        max_line_length: Some(max_line_length),
        background_image: if theme.background_id == "none" {
            None
        } else {
            Some(theme.background_id.clone())
        },
        scale: theme.render_scale as f32,
    };

    let png = tokio::task::spawn_blocking(move || {
        renderer.render_with_options(
            PREVIEW_CODE,
            Some("rust"),
            &theme_name,
            &render_options,
        )
    })
    .await
    .map_err(|e| BlazeError::rendering(e.to_string()))??;

    drop(_permit);

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
    let repo = PgThemeRepository::new(ctx.data().db.clone());

    repo.delete_theme(user_id).await?;

    tracing::info!(user_id, "テーマリセット");

    ctx.send(
        poise::CreateReply::default()
            .content("テーマをデフォルトにリセットしました")
            .ephemeral(true),
    )
    .await?;

    Ok(())
}
