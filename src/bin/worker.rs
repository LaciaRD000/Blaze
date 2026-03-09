//! Blaze Worker — CPU バウンドなレンダリング処理を担当する独立プロセス
//!
//! Redis キュー `blaze:jobs` からジョブを取り出し、
//! SVG 生成 → PNG ラスタライズを実行して結果を返す。
//! 複数インスタンスを起動して水平スケール可能。

use std::sync::Arc;

use redis::AsyncCommands;

use blaze_bot::config::Settings;
use blaze_bot::protocol::{
    RenderJob, RenderJobOptions, RenderResult, JOBS_QUEUE, RESULT_TTL_SECS,
};
use blaze_bot::renderer::{RenderOptions, Renderer};

/// RenderJobOptions → RenderOptions への変換
fn to_render_options(opts: &RenderJobOptions) -> RenderOptions {
    RenderOptions {
        title_bar_style: opts.title_bar_style.clone(),
        opacity: opts.opacity,
        blur_radius: opts.blur_radius,
        show_line_numbers: opts.show_line_numbers,
        max_line_length: opts.max_line_length,
        background_image: opts.background_image.clone(),
        font_family: opts.font_family.clone(),
    }
}

/// ジョブを処理してレンダリングを実行する
fn process_job(renderer: &Renderer, job: &RenderJob) -> RenderResult {
    let options = to_render_options(&job.options);
    match renderer.render_with_options(
        &job.code,
        job.language.as_deref(),
        &job.theme_name,
        &options,
    ) {
        Ok(png_bytes) => RenderResult::Success { png_bytes },
        Err(e) => RenderResult::Error {
            message: e.to_string(),
        },
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

    // ロギング初期化
    let log_filter = settings.log_level.clone();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&log_filter)
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Redis 接続
    let redis_url = settings
        .redis_url
        .as_deref()
        .unwrap_or("redis://127.0.0.1/");
    let redis_client = redis::Client::open(redis_url)
        .expect("Redis クライアントの作成に失敗");
    let mut conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .expect("Redis 接続に失敗");

    // レンダラー初期化（フォント・背景・構文定義をすべてメモリにロード）
    let renderer = Arc::new(Renderer::new());

    tracing::info!(
        redis_url = redis_url,
        "Blaze Worker 起動"
    );

    // メインループ: ジョブを1つずつ取り出して処理する
    // 並行処理は Worker プロセスの複数起動で実現する
    loop {
        // BRPOP: ジョブが来るまでブロック（タイムアウト 0 = 無限待機）
        let result: Result<Option<(String, String)>, redis::RedisError> =
            redis::cmd("BRPOP")
                .arg(JOBS_QUEUE)
                .arg(0)
                .query_async(&mut conn)
                .await;

        let job_json = match result {
            Ok(Some((_key, value))) => value,
            Ok(None) => continue,
            Err(e) => {
                tracing::error!("Redis BRPOP エラー: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        // ジョブのデシリアライズ
        let job: RenderJob = match serde_json::from_str(&job_json) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("ジョブのデシリアライズに失敗: {e}");
                continue;
            }
        };

        let job_id = job.job_id.clone();
        let result_key = job.result_key();

        // spawn_blocking で CPU バウンドなレンダリングを実行
        let renderer_clone = Arc::clone(&renderer);
        let start = std::time::Instant::now();
        let render_result =
            match tokio::task::spawn_blocking(move || {
                process_job(&renderer_clone, &job)
            })
            .await
            {
                Ok(r) => r,
                Err(e) => RenderResult::Error {
                    message: format!("spawn_blocking エラー: {e}"),
                },
            };

        let elapsed = start.elapsed();
        match &render_result {
            RenderResult::Success { png_bytes } => {
                tracing::info!(
                    job_id = %job_id,
                    elapsed_ms = elapsed.as_millis() as u64,
                    png_size = png_bytes.len(),
                    "レンダリング完了"
                );
            }
            RenderResult::Error { message } => {
                tracing::error!(
                    job_id = %job_id,
                    "レンダリング失敗: {message}"
                );
            }
        }

        // 結果を Redis に書き込み
        let result_json = match serde_json::to_string(&render_result) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(
                    job_id = %job_id,
                    "結果のシリアライズに失敗: {e}"
                );
                continue;
            }
        };

        // LPUSH + EXPIRE で結果を格納（Gateway が BRPOP で待機中）
        if let Err(e) = conn
            .lpush::<_, _, ()>(&result_key, &result_json)
            .await
        {
            tracing::error!(
                job_id = %job_id,
                "結果の LPUSH に失敗: {e}"
            );
            continue;
        }
        if let Err(e) = conn
            .expire::<_, ()>(&result_key, RESULT_TTL_SECS as i64)
            .await
        {
            tracing::error!(
                job_id = %job_id,
                "EXPIRE 設定に失敗: {e}"
            );
        }
    }
}
