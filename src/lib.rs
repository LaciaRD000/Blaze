use std::sync::Arc;

pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod protocol;
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
    /// Gateway モードで Worker に委譲する際の Redis 接続（Monolith では None）
    /// MultiplexedConnection は Clone 可能で内部で多重化されるため Mutex 不要
    pub redis: Option<redis::aio::MultiplexedConnection>,
}

pub type Error = BlazeError;
pub type Context<'a> = poise::Context<'a, Data, Error>;
