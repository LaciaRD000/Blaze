use std::sync::Arc;

use poise::serenity_prelude as serenity;

pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod renderer;
pub mod sanitize;

use config::Settings;
use error::BlazeError;

/// Bot の共有データ。poise Framework の `Data` 型として使用する。
pub struct Data {
    pub settings: Arc<Settings>,
    pub renderer: Arc<renderer::Renderer>,
    pub db: sqlx::PgPool,
    pub render_semaphore: Arc<tokio::sync::Semaphore>,
    pub rate_limiter: Arc<governor::DefaultKeyedRateLimiter<u64>>,
}

type Error = BlazeError;
type Context<'a> = poise::Context<'a, Data, Error>;

/// グローバルエラーハンドラ
/// BlazeError をユーザー向けのエフェメラルメッセージに変換する
async fn on_error(error: poise::FrameworkError<'_, Data, BlazeError>) {
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

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    // 設定ファイル読み込み
    let settings_str = std::fs::read_to_string("config/default.toml")
        .expect("config/default.toml の読み込みに失敗");
    let mut settings: Settings = toml::from_str(&settings_str)
        .expect("config/default.toml のパースに失敗");
    settings.apply_env_overrides();
    settings.validate().expect("設定値のバリデーションに失敗");

    // ロギング初期化
    let log_filter = settings.log_level.clone();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&log_filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let settings = Arc::new(settings);

    // データベース接続 (Supabase PostgreSQL)
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL 環境変数が設定されていません");
    let db = db::init_pool(&database_url)
        .await
        .expect("データベースの初期化に失敗");

    let token = std::env::var("DISCORD_TOKEN")
        .expect("DISCORD_TOKEN 環境変数が設定されていません");

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::render::render_message(),
                commands::theme::theme(),
            ],
            on_error: |err| Box::pin(on_error(err)),
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(
                    ctx,
                    &framework.options().commands,
                )
                .await?;
                let renderer = Arc::new(renderer::Renderer::new());
                tracing::info!("Bot が起動しました");
                let render_semaphore = Arc::new(tokio::sync::Semaphore::new(
                    settings.max_concurrent_renders,
                ));
                let quota = governor::Quota::per_minute(
                    std::num::NonZeroU32::new(settings.rate_limit_per_minute)
                        .expect("rate_limit_per_minute は 0 でないべき"),
                );
                let rate_limiter =
                    Arc::new(governor::RateLimiter::keyed(quota));
                Ok(Data {
                    settings,
                    renderer,
                    db,
                    render_semaphore,
                    rate_limiter,
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await
        .expect("Discord クライアントの作成に失敗");

    // Graceful Shutdown: Ctrl+C で停止
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("シグナルハンドラの登録に失敗");
        tracing::info!("シャットダウン中...");
        shard_manager.shutdown_all().await;
    });

    if let Err(e) = client.start().await {
        tracing::error!("Bot エラー: {e:?}");
    }
}
