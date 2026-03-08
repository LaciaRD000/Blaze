pub mod models;

use sqlx::SqlitePool;

use crate::error::BlazeError;
use models::UserTheme;

/// テーマの永続化を抽象化するトレイト
pub trait ThemeRepository {
    fn get_theme(
        &self,
        user_id: u64,
    ) -> impl std::future::Future<Output = Result<Option<UserTheme>, BlazeError>>
    + Send;

    fn upsert_theme(
        &self,
        theme: &UserTheme,
    ) -> impl std::future::Future<Output = Result<(), BlazeError>> + Send;

    fn delete_theme(
        &self,
        user_id: u64,
    ) -> impl std::future::Future<Output = Result<(), BlazeError>> + Send;
}

/// SQLite による ThemeRepository 実装
pub struct SqliteThemeRepository {
    pool: SqlitePool,
}

impl SqliteThemeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl ThemeRepository for SqliteThemeRepository {
    async fn get_theme(
        &self,
        user_id: u64,
    ) -> Result<Option<UserTheme>, BlazeError> {
        let user_id = user_id as i64;
        sqlx::query_as::<_, UserTheme>(
            "SELECT user_id, color_scheme, background_id, blur_radius, opacity, \
             font_family, font_size, title_bar_style, show_line_numbers, updated_at \
             FROM user_themes WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(BlazeError::Database)
    }

    async fn upsert_theme(&self, theme: &UserTheme) -> Result<(), BlazeError> {
        sqlx::query(
            "INSERT INTO user_themes (user_id, color_scheme, background_id, blur_radius, \
             opacity, font_family, font_size, title_bar_style, show_line_numbers, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now')) \
             ON CONFLICT(user_id) DO UPDATE SET \
             color_scheme = excluded.color_scheme, \
             background_id = excluded.background_id, \
             blur_radius = excluded.blur_radius, \
             opacity = excluded.opacity, \
             font_family = excluded.font_family, \
             font_size = excluded.font_size, \
             title_bar_style = excluded.title_bar_style, \
             show_line_numbers = excluded.show_line_numbers, \
             updated_at = datetime('now')",
        )
        .bind(theme.user_id)
        .bind(&theme.color_scheme)
        .bind(&theme.background_id)
        .bind(theme.blur_radius)
        .bind(theme.opacity)
        .bind(&theme.font_family)
        .bind(theme.font_size)
        .bind(&theme.title_bar_style)
        .bind(theme.show_line_numbers)
        .execute(&self.pool)
        .await
        .map_err(BlazeError::Database)?;

        Ok(())
    }

    async fn delete_theme(&self, user_id: u64) -> Result<(), BlazeError> {
        let user_id = user_id as i64;
        sqlx::query("DELETE FROM user_themes WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(BlazeError::Database)?;

        Ok(())
    }
}

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

    /// テスト用の SqliteThemeRepository を作成するヘルパー
    async fn setup_repo() -> SqliteThemeRepository {
        let pool = init_pool("sqlite::memory:")
            .await
            .expect("テスト用DBの初期化に成功するべき");
        SqliteThemeRepository::new(pool)
    }

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

    #[tokio::test]
    async fn get_theme_nonexistent_returns_none() {
        let repo = setup_repo().await;
        let result = repo.get_theme(12345).await.expect("クエリに成功するべき");
        assert!(result.is_none(), "存在しないユーザーは None を返すべき");
    }

    #[tokio::test]
    async fn upsert_then_get_theme_succeeds() {
        let repo = setup_repo().await;
        let theme = UserTheme::with_defaults(12345);

        repo.upsert_theme(&theme)
            .await
            .expect("テーマの挿入に成功するべき");

        let fetched = repo
            .get_theme(12345)
            .await
            .expect("クエリに成功するべき")
            .expect("テーマが見つかるべき");

        assert_eq!(fetched.user_id, 12345);
        assert_eq!(fetched.color_scheme, "base16-ocean.dark");
        assert!((fetched.blur_radius - 8.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn upsert_theme_updates_existing() {
        let repo = setup_repo().await;
        let mut theme = UserTheme::with_defaults(12345);
        repo.upsert_theme(&theme)
            .await
            .expect("テーマの挿入に成功するべき");

        // テーマを変更して再度 upsert
        theme.color_scheme = "Solarized (dark)".to_string();
        theme.opacity = 0.5;
        repo.upsert_theme(&theme)
            .await
            .expect("テーマの更新に成功するべき");

        let fetched = repo
            .get_theme(12345)
            .await
            .expect("クエリに成功するべき")
            .expect("テーマが見つかるべき");

        assert_eq!(fetched.color_scheme, "Solarized (dark)");
        assert!((fetched.opacity - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn delete_theme_removes_record() {
        let repo = setup_repo().await;
        let theme = UserTheme::with_defaults(12345);
        repo.upsert_theme(&theme)
            .await
            .expect("テーマの挿入に成功するべき");

        repo.delete_theme(12345)
            .await
            .expect("テーマの削除に成功するべき");

        let result = repo.get_theme(12345).await.expect("クエリに成功するべき");
        assert!(result.is_none(), "削除後は None を返すべき");
    }

    #[tokio::test]
    async fn delete_nonexistent_theme_does_not_error() {
        let repo = setup_repo().await;
        // 存在しないテーマの削除もエラーにならない
        repo.delete_theme(99999)
            .await
            .expect("存在しないテーマの削除もエラーにならないべき");
    }
}
