use std::sync::Arc;

pub mod commands;
pub mod config;
pub mod db;
pub mod error;
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
}

pub type Error = BlazeError;
pub type Context<'a> = poise::Context<'a, Data, Error>;
