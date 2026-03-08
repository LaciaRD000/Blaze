//! Blaze Gateway — Discord との I/O を担当する独立プロセス
//!
//! コンテキストメニュー・スラッシュコマンドを受け付け、
//! CPU バウンドなレンダリング処理は Redis キュー経由で Worker に委譲する。
//! Gateway は I/O バウンドのみのため、Worker のパニックや過負荷が
//! Discord WebSocket 接続に影響しない。

use std::sync::Arc;

use poise::serenity_prelude as serenity;

use blaze_bot::commands;
use blaze_bot::config::Settings;
use blaze_bot::error::BlazeError;
use blaze_bot::protocol::{
    RenderJob, RenderJobOptions, RenderResult, JOBS_QUEUE,
};
use blaze_bot::{db, Data};

/// RenderJob を Redis キューに投入し、結果を待つ
async fn submit_render_job(
    redis: &mut redis::aio::MultiplexedConnection,
    job: &RenderJob,
    timeout_secs: u64,
) -> Result<RenderResult, BlazeError> {
    // ジョブをシリアライズして Redis キューに LPUSH
    let job_json = serde_json::to_string(job)
        .map_err(|e| BlazeError::rendering(format!("ジョブのシリアライズに失敗: {e}")))?;

    let _: () = redis::cmd("LPUSH")
        .arg(JOBS_QUEUE)
        .arg(&job_json)
        .query_async(redis)
        .await
        .map_err(|e| BlazeError::rendering(format!("Redis LPUSH エラー: {e}")))?;

    // 結果を BRPOP で待機
    let result_key = job.result_key();
    let result: Option<(String, String)> = redis::cmd("BRPOP")
        .arg(&result_key)
        .arg(timeout_secs)
        .query_async(redis)
        .await
        .map_err(|e| BlazeError::rendering(format!("Redis BRPOP エラー: {e}")))?;

    match result {
        Some((_key, json)) => {
            let render_result: RenderResult = serde_json::from_str(&json)
                .map_err(|e| {
                    BlazeError::rendering(format!(
                        "結果のデシリアライズに失敗: {e}"
                    ))
                })?;
            Ok(render_result)
        }
        None => Err(BlazeError::rendering(
            "レンダリングタイムアウト: Worker が応答しませんでした",
        )),
    }
}

/// コンテキストメニュー「ターミナル画像化」— Gateway 版
/// 入力バリデーション後、Redis 経由で Worker にレンダリングを委譲する
#[poise::command(
    context_menu_command = "ターミナル画像化",
    category = "Render"
)]
async fn render_message(
    ctx: poise::Context<'_, Data, BlazeError>,
    #[description = "対象メッセージ"] msg: serenity::Message,
) -> Result<(), BlazeError> {
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
        return Err(BlazeError::RateLimitExceeded);
    }

    // 1. コードブロック抽出
    let code_block = commands::render::extract_code_block(&msg.content)
        .ok_or(BlazeError::CodeBlockNotFound)?;

    // 2. 入力バリデーション
    let settings = &ctx.data().settings;
    if code_block.code.lines().count() > settings.max_code_lines
        || code_block.code.len() > settings.max_code_chars
    {
        return Err(BlazeError::CodeTooLong {
            max_lines: settings.max_code_lines,
            max_chars: settings.max_code_chars,
        });
    }

    // バリデーション通過 — defer する
    ctx.defer().await?;

    // 3. 入力サニタイズ
    let code_block = code_block.sanitized();

    // 4. ユーザーテーマ取得
    let user_id = ctx.author().id.get() as i64;
    let theme = {
        let repo = db::PgThemeRepository::new(ctx.data().db.clone());
        db::ThemeRepository::get_theme(&repo, user_id as u64)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| db::models::UserTheme::with_defaults(user_id))
    };

    // 5. RenderJob を構築して Redis キューに投入
    let job = RenderJob::new(
        code_block.code,
        code_block.language,
        theme.color_scheme.clone(),
        RenderJobOptions {
            title_bar_style: theme.title_bar_style.clone(),
            opacity: theme.opacity,
            blur_radius: theme.blur_radius,
            show_line_numbers: theme.show_line_numbers != 0,
            max_line_length: Some(settings.max_line_length),
            background_image: if theme.background_id == "none" {
                None
            } else {
                Some(theme.background_id.clone())
            },
        },
    );

    // Gateway 起動時に保持した Redis 接続を取得
    // NOTE: GatewayData は Data のラッパーだが、poise の型制約上
    //       redis 接続は別途管理する必要がある。
    //       ここでは環境変数から再接続する（簡易実装）
    let redis_url = settings
        .redis_url
        .as_deref()
        .unwrap_or("redis://127.0.0.1/");
    let redis_client = redis::Client::open(redis_url)
        .map_err(|e| BlazeError::rendering(format!("Redis クライアント作成エラー: {e}")))?;
    let mut redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| BlazeError::rendering(format!("Redis 接続エラー: {e}")))?;

    let render_result = submit_render_job(&mut redis_conn, &job, 30).await?;

    let elapsed = start.elapsed();
    tracing::info!(
        user_id = user_id_for_rate,
        job_id = %job.job_id,
        elapsed_ms = elapsed.as_millis() as u64,
        "Gateway: レンダリング完了"
    );

    // 6. 結果に応じてレスポンスを送信
    match render_result {
        RenderResult::Success { png_bytes } => {
            let attachment =
                serenity::CreateAttachment::bytes(png_bytes, "code.png");
            ctx.send(
                poise::CreateReply::default()
                    .attachment(attachment)
                    .reply(true),
            )
            .await?;
        }
        RenderResult::Error { message } => {
            return Err(BlazeError::rendering(message));
        }
    }

    Ok(())
}

/// グローバルエラーハンドラ
async fn on_error(error: poise::FrameworkError<'_, Data, BlazeError>) {
    match error {
        poise::FrameworkError::Command { error, ctx, .. } => {
            let user_message = match &error {
                BlazeError::CodeBlockNotFound
                | BlazeError::CodeTooLong { .. }
                | BlazeError::RateLimitExceeded
                | BlazeError::InvalidTheme(_) => error.to_string(),
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

    // 設定読み込み
    let settings_str = std::fs::read_to_string("config/default.toml")
        .expect("config/default.toml の読み込みに失敗");
    let mut settings: Settings =
        toml::from_str(&settings_str).expect("config/default.toml のパースに失敗");
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

    // データベース接続
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL 環境変数が設定されていません");
    let db = db::init_pool(&database_url)
        .await
        .expect("データベースの初期化に失敗");

    let token =
        std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN 環境変数が設定されていません");

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                render_message(),
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

                // Gateway はレンダラーを持たない（Worker に委譲）
                // ダミーの Renderer を作成（/theme preview で使用）
                let renderer =
                    Arc::new(blaze_bot::renderer::Renderer::new());

                tracing::info!("Blaze Gateway 起動");

                let render_semaphore =
                    Arc::new(tokio::sync::Semaphore::new(
                        settings.max_concurrent_renders,
                    ));
                let quota = governor::Quota::per_minute(
                    std::num::NonZeroU32::new(
                        settings.rate_limit_per_minute,
                    )
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

    // Graceful Shutdown
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("SIGTERM ハンドラの登録に失敗");

        tokio::select! {
            _ = ctrl_c => tracing::info!("SIGINT 受信、シャットダウン中..."),
            _ = sigterm.recv() => tracing::info!("SIGTERM 受信、シャットダウン中..."),
        }
        shard_manager.shutdown_all().await;
    });

    if let Err(e) = client.start().await {
        tracing::error!("Gateway エラー: {e:?}");
    }
}
