pub mod models;

use sqlx::SqlitePool;

use crate::error::BlazeError;

/// SQLite コネクションプールを作成し、マイグレーションを実行する
pub async fn init_pool(database_url: &str) -> Result<SqlitePool, BlazeError> {
    let pool = SqlitePool::connect(database_url)
        .await
        .map_err(BlazeError::Database)?;

    // WAL モードを有効化（並行読み取り性能向上）
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .map_err(BlazeError::Database)?;

    // マイグレーション自動実行
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| BlazeError::Database(e.into()))?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_pool_with_in_memory_db_succeeds() {
        let pool = init_pool("sqlite::memory:")
            .await
            .expect("インメモリDBの初期化に成功するべき");

        // user_themes テーブルが存在することを確認
        let result = sqlx::query_scalar::<_, String>(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='user_themes'",
        )
        .fetch_optional(&pool)
        .await
        .expect("クエリに成功するべき");

        assert_eq!(
            result.as_deref(),
            Some("user_themes"),
            "user_themes テーブルが作成されているべき"
        );
    }

    #[tokio::test]
    async fn init_pool_migration_is_idempotent() {
        // 2回連続で初期化しても問題ないことを確認
        let pool = init_pool("sqlite::memory:")
            .await
            .expect("1回目の初期化に成功するべき");

        // 同じプールに対して再度マイグレーションを実行
        let result = sqlx::migrate!("./migrations").run(&pool).await;
        assert!(
            result.is_ok(),
            "マイグレーションの再実行がエラーにならないべき"
        );
    }
}
