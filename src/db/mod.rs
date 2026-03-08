pub mod models;

use sqlx::PgPool;

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

/// PostgreSQL (Supabase) による ThemeRepository 実装
pub struct PgThemeRepository {
    pool: PgPool,
}

impl PgThemeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ThemeRepository for PgThemeRepository {
    async fn get_theme(
        &self,
        user_id: u64,
    ) -> Result<Option<UserTheme>, BlazeError> {
        let user_id = user_id as i64;
        sqlx::query_as::<_, UserTheme>(
            "SELECT user_id, color_scheme, background_id, blur_radius, opacity, \
             font_family, font_size, title_bar_style, show_line_numbers, render_scale, \
             updated_at FROM user_themes WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(BlazeError::Database)
    }

    async fn upsert_theme(&self, theme: &UserTheme) -> Result<(), BlazeError> {
        sqlx::query(
            "INSERT INTO user_themes (user_id, color_scheme, background_id, blur_radius, \
             opacity, font_family, font_size, title_bar_style, show_line_numbers, \
             render_scale, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now()) \
             ON CONFLICT (user_id) DO UPDATE SET \
             color_scheme = EXCLUDED.color_scheme, \
             background_id = EXCLUDED.background_id, \
             blur_radius = EXCLUDED.blur_radius, \
             opacity = EXCLUDED.opacity, \
             font_family = EXCLUDED.font_family, \
             font_size = EXCLUDED.font_size, \
             title_bar_style = EXCLUDED.title_bar_style, \
             show_line_numbers = EXCLUDED.show_line_numbers, \
             render_scale = EXCLUDED.render_scale, \
             updated_at = now()",
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
        .bind(theme.render_scale)
        .execute(&self.pool)
        .await
        .map_err(BlazeError::Database)?;

        Ok(())
    }

    async fn delete_theme(&self, user_id: u64) -> Result<(), BlazeError> {
        let user_id = user_id as i64;
        sqlx::query("DELETE FROM user_themes WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(BlazeError::Database)?;

        Ok(())
    }
}

/// PostgreSQL コネクションプールを作成し、マイグレーションを実行する
pub async fn init_pool(database_url: &str) -> Result<PgPool, BlazeError> {
    let pool = PgPool::connect(database_url)
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

    /// テスト用の PgThemeRepository を作成するヘルパー
    /// DATABASE_URL 環境変数が設定されていない場合は None を返す（テストスキップ）
    async fn setup_repo() -> Option<PgThemeRepository> {
        let url = std::env::var("DATABASE_URL").ok()?;
        let pool = init_pool(&url).await.ok()?;
        Some(PgThemeRepository::new(pool))
    }

    /// テスト用のユニークなユーザーID（プロセスIDを含めて衝突回避）
    fn test_user_id() -> u64 {
        9_999_999_000 + std::process::id() as u64
    }

    #[tokio::test]
    async fn init_pool_connects_to_database() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL が未設定のためスキップ");
            return;
        };
        let pool = init_pool(&url).await.expect("DBの初期化に成功するべき");

        // user_themes テーブルが存在することを確認
        let result = sqlx::query_scalar::<_, String>(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_name = 'user_themes' AND table_schema = 'public'",
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
    async fn get_theme_nonexistent_returns_none() {
        let Some(repo) = setup_repo().await else {
            return;
        };
        let result = repo
            .get_theme(99999999)
            .await
            .expect("クエリに成功するべき");
        assert!(result.is_none(), "存在しないユーザーは None を返すべき");
    }

    #[tokio::test]
    async fn upsert_then_get_then_delete_theme() {
        let Some(repo) = setup_repo().await else {
            return;
        };
        let uid = test_user_id();

        // upsert
        let theme = UserTheme::with_defaults(uid as i64);
        repo.upsert_theme(&theme)
            .await
            .expect("テーマの挿入に成功するべき");

        // get
        let fetched = repo
            .get_theme(uid)
            .await
            .expect("クエリに成功するべき")
            .expect("テーマが見つかるべき");
        assert_eq!(fetched.user_id, uid as i64);
        assert_eq!(fetched.color_scheme, "base16-ocean.dark");

        // update
        let mut updated = fetched;
        updated.color_scheme = "Solarized (dark)".to_string();
        repo.upsert_theme(&updated)
            .await
            .expect("テーマの更新に成功するべき");

        let fetched2 = repo
            .get_theme(uid)
            .await
            .expect("クエリに成功するべき")
            .expect("テーマが見つかるべき");
        assert_eq!(fetched2.color_scheme, "Solarized (dark)");

        // delete（テストデータのクリーンアップ）
        repo.delete_theme(uid)
            .await
            .expect("テーマの削除に成功するべき");

        let result = repo.get_theme(uid).await.expect("クエリに成功するべき");
        assert!(result.is_none(), "削除後は None を返すべき");
    }
}
