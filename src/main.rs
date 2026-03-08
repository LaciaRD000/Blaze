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
/// 後のフェーズで rate_limiter を追加する。
pub struct Data {
    pub settings: Arc<Settings>,
    pub renderer: Arc<renderer::Renderer>,
    pub db: sqlx::SqlitePool,
    pub render_semaphore: Arc<tokio::sync::Semaphore>,
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
                    eprintln!("内部エラー: {error:?}");
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
    let settings: Settings = toml::from_str(&settings_str)
        .expect("config/default.toml のパースに失敗");
    settings.validate().expect("設定値のバリデーションに失敗");
    let settings = Arc::new(settings);

    // データベース接続
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:blaze-bot.db?mode=rwc".to_string());
    let db = db::init_pool(&database_url)
        .await
        .expect("データベースの初期化に失敗");

    let token = std::env::var("DISCORD_TOKEN")
        .expect("DISCORD_TOKEN 環境変数が設定されていません");

    let intents = serenity::GatewayIntents::non_privileged();

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
                println!("Bot が起動しました");
                let render_semaphore = Arc::new(tokio::sync::Semaphore::new(
                    settings.max_concurrent_renders,
                ));
                Ok(Data {
                    settings,
                    renderer,
                    db,
                    render_semaphore,
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
        println!("シャットダウン中...");
        shard_manager.shutdown_all().await;
    });

    if let Err(e) = client.start().await {
        eprintln!("Bot エラー: {e:?}");
    }
}
