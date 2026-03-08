# Blaze Bot - アーキテクチャ設計

## 概要

Discord上に投稿されたコードブロックを、SwayFX/Wezterm風の美しいターミナルウィンドウ画像へ変換するBot。
外部APIに依存せず、Rust内部でネイティブにレンダリングを行う。

---

## プロジェクト構造

```
blaze-bot/
├── Cargo.toml
├── config/
│   └── default.toml              # デフォルト設定値
├── .env                          # DISCORD_TOKEN, DATABASE_URL (git管理外)
├── migrations/
│   ├── 001_create_user_themes.up.sql    # CREATE TABLE
│   └── 001_create_user_themes.down.sql  # DROP TABLE (ロールバック用)
├── assets/
│   ├── fonts/                    # 埋め込みフォント (FiraCode, PlemolJP, HackGen NF)
│   └── backgrounds/              # デフォルト背景画像
├── src/
│   ├── main.rs                   # エントリポイント、Bot起動
│   ├── config.rs                 # 設定管理 (Settings構造体、バリデーション)
│   ├── error.rs                  # BlazeError 独自エラー型 (thiserror)
│   ├── sanitize.rs               # 入力サニタイズ・SVGエスケープ
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── render.rs             # コンテキストメニュー「ターミナル画像化」
│   │   └── theme.rs              # /theme set, /theme preview 等
│   ├── renderer/
│   │   ├── mod.rs                # レンダリングパイプライン統括
│   │   ├── highlight.rs          # syntect によるトークン化・色付け
│   │   ├── svg_builder.rs        # 動的SVG文字列の組み立て
│   │   └── rasterize.rs          # resvg/tiny-skia で SVG→PNG変換
│   └── db/
│       ├── mod.rs                # ThemeRepository トレイト、PgPoolコネクションプール
│       └── models.rs             # UserTheme 構造体・CRUD
├── tests/
│   ├── extract_code_block.rs     # コードブロック抽出テスト
│   ├── highlight.rs              # シンタックスハイライトテスト
│   ├── svg_builder.rs            # SVG生成テスト
│   ├── render_pipeline.rs        # レンダリングパイプライン統合テスト
│   ├── theme_repository.rs       # DB操作テスト
│   └── common/
│       └── mod.rs                # テストヘルパー・フィクスチャ
```

---

## データフロー

```
ユーザー右クリック
  → [Discord Gateway]
  → poise コンテキストメニューハンドラ (commands/render.rs)
  → レート制限チェック (governor, ユーザーごと 10req/min)
      超過時 → エフェメラル（実行者のみ）で「レート制限に達しました」と通知して Ok(()) で終了
  → メッセージからコードブロック抽出 (```lang\ncode```)
      無い場合 → エフェメラル（実行者のみ）で通知して Ok(()) で終了
      複数ブロック → 最初のブロックを使用（将来: 選択UIまたは連結画像）
  → 入力バリデーション (最大100行 / 最大4000文字)
      超過時 → エフェメラル（実行者のみ）で「コードが長すぎます」と通知して Ok(()) で終了
  → 入力サニタイズ (制御文字除去、Unicode正規化、SVGエスケープ)
  → 言語自動判定 (syntect SyntaxSet)
  → トークン化 & 色付け (renderer/highlight.rs)
  → ユーザーテーマ取得 (db/ → キャッシュ層 → UserTheme or デフォルト)
  → SVG文字列生成 (renderer/svg_builder.rs)
      - 背景画像 + ガウスぼかし (SVG feGaussianBlur)
      - 半透明ウィンドウ矩形 (fill-opacity)
      - タイトルバー + 角丸 + ドロップシャドウ
      - 色付きテキスト (<tspan>) の配置
      - フォント: font-family="Fira Code", "PlemolJP", sans-serif
  → PNG ラスタライズ (renderer/rasterize.rs)
      - resvg::render() → tiny_skia::Pixmap → PNG bytes
      - 2x スケールで高解像度レンダリング（Discord の高DPI表示に対応）
  → Discord に通常メッセージとしてリプライ (画像添付、全員に表示)
  → メトリクス記録 (レンダリング回数、処理時間)
```

---

## 主要な型定義

### Bot データ (src/main.rs)

```rust
pub struct Data {
    pub db: sqlx::PgPool,
    pub renderer: Arc<renderer::Renderer>,
    pub rate_limiter: Arc<governor::DefaultKeyedRateLimiter<u64>>,
    pub render_semaphore: Arc<tokio::sync::Semaphore>,  // spawn_blocking 同時実行数制御
    pub settings: Arc<Settings>,
}

// BlazeError は Into<Box<dyn Error>> を自動実装するため、poise との互換性あり
type Error = BlazeError;
type Context<'a> = poise::Context<'a, Data, Error>;
```

### 独自エラー型 (src/error.rs)

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlazeError {
    #[error("メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした")]
    CodeBlockNotFound,

    #[error("コードが長すぎます（上限: {max_lines}行 / {max_chars}文字）")]
    CodeTooLong { max_lines: usize, max_chars: usize },

    #[error("データベースエラー: {0}")]
    Database(#[from] sqlx::Error),

    #[error("レンダリングエラー: {message}")]
    Rendering {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("レート制限に達しました。しばらくお待ちください。")]
    RateLimitExceeded,

    #[error("無効なテーマ設定: {0}")]
    InvalidTheme(String),

    #[error("設定エラー: {0}")]
    Config(String),
}

impl From<syntect::Error> for BlazeError {
    fn from(e: syntect::Error) -> Self {
        BlazeError::Rendering {
            message: e.to_string(),
            source: Some(Box::new(e)),
        }
    }
}

impl BlazeError {
    /// ソースエラーなしのレンダリングエラーを作成する
    pub fn rendering(message: impl Into<String>) -> Self {
        BlazeError::Rendering { message: message.into(), source: None }
    }
}
```

### 設定管理 (src/config.rs)

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    // discord_token / database_url はシークレットのため config/default.toml には載せない
    // 環境変数 (DISCORD_TOKEN, DATABASE_URL) から直接読み取る
    pub max_code_lines: usize,      // デフォルト: 100
    pub max_code_chars: usize,      // デフォルト: 4000
    pub max_line_length: usize,     // デフォルト: 120 (超過行はトリミング + "...")
    pub rate_limit_per_minute: u32, // デフォルト: 10
    pub max_concurrent_renders: usize, // デフォルト: 4
    pub log_level: String,          // デフォルト: "info"
}

impl Settings {
    /// 設定値の範囲を検証する。Bot起動時に呼び出し、不正値なら即座にパニックさせる
    pub fn validate(&self) -> Result<(), BlazeError> {
        if self.max_code_lines == 0 || self.max_code_lines > 500 {
            return Err(BlazeError::Config(
                format!("max_code_lines は 1〜500 の範囲: {}", self.max_code_lines)
            ));
        }
        if self.max_code_chars == 0 || self.max_code_chars > 20_000 {
            return Err(BlazeError::Config(
                format!("max_code_chars は 1〜20000 の範囲: {}", self.max_code_chars)
            ));
        }
        if self.rate_limit_per_minute == 0 || self.rate_limit_per_minute > 120 {
            return Err(BlazeError::Config(
                format!("rate_limit_per_minute は 1〜120 の範囲: {}", self.rate_limit_per_minute)
            ));
        }
        if self.max_line_length == 0 || self.max_line_length > 500 {
            return Err(BlazeError::Config(
                format!("max_line_length は 1〜500 の範囲: {}", self.max_line_length)
            ));
        }
        if self.max_concurrent_renders == 0 || self.max_concurrent_renders > 32 {
            return Err(BlazeError::Config(
                format!("max_concurrent_renders は 1〜32 の範囲: {}", self.max_concurrent_renders)
            ));
        }
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.log_level.as_str()) {
            return Err(BlazeError::Config(
                format!("log_level は {:?} のいずれか: {}", valid_levels, self.log_level)
            ));
        }
        Ok(())
    }
}
```

### コードブロック (src/commands/render.rs)

```rust
pub struct CodeBlock {
    pub language: Option<String>,  // 言語タグ (e.g. "rust", "python")
    pub code: String,              // コード本体
}

impl CodeBlock {
    /// 制御文字除去・Unicode正規化を適用した新しい CodeBlock を返す
    pub fn sanitized(&self) -> Self {
        Self {
            language: self.language.clone(),
            code: sanitize_code(&self.code),
        }
    }
}

/// メッセージ本文から最初のコードブロックを抽出する
/// 正規表現: ```(\w*)\n([\s\S]*?)```
pub fn extract_code_block(content: &str) -> Option<CodeBlock>;
```

### 入力サニタイズ (src/sanitize.rs)

```rust
/// 入力コードを正規化する:
/// - 制御文字を除去する（改行は保持）
/// - タブ文字を半角スペース4つに展開する（Expand Tabs）
/// - Unicode NFC 正規化を適用する
/// xml:space="preserve" と組み合わせてインデントを保持する
pub fn sanitize_code(code: &str) -> String;

/// SVG出力時の特殊文字エスケープ (&, <, >, ")
pub fn escape_for_svg(text: &str) -> String;
```

### ユーザーテーマ (src/db/models.rs)

```rust
#[derive(sqlx::FromRow, Clone)]
pub struct UserTheme {
    pub user_id: i64,           // Discord user ID
    pub color_scheme: String,   // syntect テーマ名 (e.g. "base16-ocean.dark")
    pub background_id: String,  // 背景画像識別子 (e.g. "default", "mountain")
    pub blur_radius: f32,       // ガウスぼかし強度 (0.0 - 30.0)
    pub opacity: f32,           // ウィンドウ不透明度 (0.3 - 1.0)
    pub font_family: String,    // フォント名 (e.g. "Fira Code")
    pub font_size: f32,         // フォントサイズ (pt)
    pub title_bar_style: String,    // "macos" | "linux"
    pub show_line_numbers: bool,    // 行番号表示 (Phase 8 で実装、スキーマは先行準備)
}
```

### レンダラー (src/renderer/mod.rs)

```rust
pub struct Renderer {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub font_db: fontdb::Database,
}

impl Renderer {
    pub fn new() -> Self {
        // usvg::Options に fontdb を統合する
        // resvg はテキスト描画時にこの fontdb を参照してグリフを解決する
        let mut options = usvg::Options::default();
        load_fonts(options.fontdb_mut());

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let font_db = std::mem::take(options.fontdb_mut()); // Renderer 内でも保持

        Self { syntax_set, theme_set, font_db }
    }
}

/// フォント読み込みを独立した関数に切り出す
/// 初期実装: include_bytes! による静的埋め込み（単一バイナリでデプロイ可能）
/// 将来: アセット肥大化時に std::fs::read による動的読み込みへ切り替え可能
fn load_fonts(font_db: &mut fontdb::Database) {
    font_db.load_font_data(include_bytes!("../assets/fonts/FiraCode-Regular.ttf").to_vec());
    font_db.load_font_data(include_bytes!("../assets/fonts/PlemolJP-Regular.ttf").to_vec());
    font_db.load_font_data(include_bytes!("../assets/fonts/HackGenConsoleNF-Regular.ttf").to_vec());
}

// リソース制限値は Settings.max_code_lines / Settings.max_code_chars を参照する
// ハードコードされた定数は持たない（Single Source of Truth = Settings）
```

### ハイライト結果 (src/renderer/highlight.rs)

```rust
pub struct HighlightedLine {
    pub tokens: Vec<StyledToken>,
}

pub struct StyledToken {
    pub text: String,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
}
```

---

## コマンド設計

### 1. コンテキストメニュー（右クリック → ターミナル画像化）

```rust
// src/commands/render.rs
#[poise::command(
    context_menu_command = "ターミナル画像化",
    category = "Render"
)]
pub async fn render_message(
    ctx: Context<'_>,
    msg: serenity::Message,
) -> Result<(), Error> {
    // 0. レート制限チェック
    // ※ defer はバリデーション通過後に呼ぶ。
    //   エラー時はエフェメラル、成功時は通常メッセージと使い分けるため。
    let user_id = ctx.author().id.get();
    if ctx.data().rate_limiter.check_key(&user_id).is_err() {
        ctx.send(
            poise::CreateReply::default()
                .content("レート制限に達しました。しばらくお待ちください。")
                .ephemeral(true)
        ).await?;
        return Ok(());
    }

    // 1. コードブロック抽出 — 見つからない場合はエフェメラルで通知して正常終了
    let code_block = match extract_code_block(&msg.content) {
        Some(block) => block,
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .content("メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした")
                    .ephemeral(true)
            ).await?;
            return Ok(());
        }
    };

    // 2. 入力バリデーション — リソース制限 (Settings から読み取り)
    let settings = &ctx.data().settings;
    if code_block.code.lines().count() > settings.max_code_lines
        || code_block.code.len() > settings.max_code_chars
    {
        ctx.send(
            poise::CreateReply::default()
                .content(format!(
                    "コードが長すぎます（上限: {}行 / {}文字）",
                    settings.max_code_lines, settings.max_code_chars
                ))
                .ephemeral(true)
        ).await?;
        return Ok(());
    }

    // バリデーション通過 — ここから時間がかかるのでdeferする（非エフェメラル）
    ctx.defer().await?;

    // 3. 入力サニタイズ (制御文字除去、Unicode正規化)
    let code_block = code_block.sanitized();

    // 4. ユーザーテーマ取得 (無ければデフォルト)
    let theme = db::get_user_theme(&ctx.data().db, user_id)
        .await?
        .unwrap_or_default();

    // 5. レンダリング → PNG bytes
    //    CPUバウンドなので spawn_blocking で実行
    //    セマフォで同時実行数を制御し、CPUの過負荷を防止
    let permit = ctx.data().render_semaphore.acquire().await
        .map_err(|_| BlazeError::rendering("セマフォ取得失敗"))?;
    let renderer = ctx.data().renderer.clone();
    let png = tokio::task::spawn_blocking(move || {
        let result = renderer.render(&code_block, &theme);
        drop(permit); // レンダリング完了後にセマフォを解放
        result
    }).await.map_err(|e| BlazeError::rendering(e.to_string()))??;

    // 6. 画像をリプライとして送信
    let attachment = serenity::CreateAttachment::bytes(png, "code.png");
    ctx.send(
        poise::CreateReply::default()
            .attachment(attachment)
            .reply(true)
    ).await?;
    Ok(())
}
```

### グローバルエラーハンドラ (src/main.rs)

`BlazeError` をユーザー向けのエフェメラルメッセージに変換する。内部エラーの詳細はログに記録し、ユーザーには汎用メッセージのみ返す。

```rust
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
                    "内部エラーが発生しました。しばらくしてからお試しください。".to_string()
                }
            };
            // エラーメッセージは実行者のみに表示（エフェメラル）
            let _ = ctx.send(
                poise::CreateReply::default()
                    .content(user_message)
                    .ephemeral(true)
            ).await;
        }
        other => {
            let _ = poise::builtins::on_error(other).await;
        }
    }
}

// Framework構築時に設定:
// poise::FrameworkOptions {
//     on_error: |err| Box::pin(on_error(err)),
//     ..
// }
```

### 2. テーマ管理スラッシュコマンド

```rust
// src/commands/theme.rs

/// テーマ設定を変更
#[poise::command(slash_command, subcommands("set", "preview", "reset"))]
pub async fn theme(_ctx: Context<'_>) -> Result<(), Error> { Ok(()) }

/// カラースキーム・背景・ぼかし等を設定
/// 文字列パラメータは poise::ChoiceParameter による Discord ドロップダウン選択肢で受け付ける
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
) -> Result<(), Error> { /* DB更新 */ }

/// 現在のテーマでサンプルコードをプレビュー
#[poise::command(slash_command)]
pub async fn preview(ctx: Context<'_>) -> Result<(), Error> { /* サンプル描画 */ }

/// テーマをデフォルトにリセット
#[poise::command(slash_command)]
pub async fn reset(ctx: Context<'_>) -> Result<(), Error> { /* DB削除 */ }
```

---

## SVGテンプレート構造 (svg_builder.rs)

生成するSVGの論理構造:

```svg
<svg width="..." height="...">
  <defs>
    <!-- ガウスぼかしフィルタ -->
    <filter id="blur">
      <feGaussianBlur stdDeviation="{blur_radius}" />
    </filter>
    <!-- ドロップシャドウ -->
    <filter id="shadow">
      <feDropShadow dx="0" dy="8" stdDeviation="16" flood-opacity="0.4" />
    </filter>
    <!-- 角丸クリップ -->
    <clipPath id="rounded">
      <rect rx="12" ry="12" ... />
    </clipPath>
  </defs>

  <!-- レイヤー1: 背景画像 (ぼかし付き) -->
  <image href="data:image/png;base64,..." filter="url(#blur)" />

  <!-- レイヤー2: ウィンドウ本体 (影 + 角丸 + 半透明) -->
  <g filter="url(#shadow)">
    <rect rx="12" fill="rgba(30,30,46,{opacity})" clip-path="url(#rounded)" />

    <!-- タイトルバー (macos: 円ボタン / linux: GNOME風ボタン / plain: 言語名のみ / none: 省略) -->
    <circle cx="20" cy="16" r="6" fill="#ff5f57" />  <!-- 閉じる -->
    <circle cx="38" cy="16" r="6" fill="#febc2e" />  <!-- 最小化 -->
    <circle cx="56" cy="16" r="6" fill="#28c840" />  <!-- 最大化 -->
    <text x="..." y="16" fill="#cdd6f4">main.rs</text>

    <!-- コード行 (syntect の色情報を反映) -->
    <!-- xml:space="preserve" で連続スペースの圧縮を防止 -->
    <!-- タブは sanitize_code() で半角スペース4つに展開済み -->
    <text y="50" font-family="'Fira Code', 'PlemolJP', sans-serif" font-size="14" xml:space="preserve">
      <tspan fill="#cba6f7" font-weight="bold">fn</tspan>
      <tspan fill="#89b4fa"> main</tspan>
      <tspan fill="#cdd6f4">()</tspan>
      <tspan fill="#cdd6f4"> {</tspan>
    </text>
    <!-- ... 以下各行 ... -->
  </g>
</svg>
```

---

## データベーススキーマ

```sql
-- migrations/001_create_user_themes.up.sql
CREATE TABLE IF NOT EXISTS user_themes (
    user_id         BIGINT PRIMARY KEY,  -- Discord user ID
    color_scheme    TEXT NOT NULL DEFAULT 'base16-ocean.dark',
    background_id   TEXT NOT NULL DEFAULT 'default',
    blur_radius     DOUBLE PRECISION NOT NULL DEFAULT 8.0,
    opacity         DOUBLE PRECISION NOT NULL DEFAULT 0.75,
    font_family     TEXT NOT NULL DEFAULT 'Fira Code',
    font_size       DOUBLE PRECISION NOT NULL DEFAULT 14.0,
    title_bar_style     TEXT NOT NULL DEFAULT 'macos',
    show_line_numbers   BOOLEAN NOT NULL DEFAULT FALSE,  -- Phase 8 で実装、スキーマは先行準備
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### マイグレーション戦略

- `sqlx migrate run` で順方向マイグレーションを適用する
- 各マイグレーションファイルには対応するロールバック用 `.down.sql` を必ず用意する:
  ```
  migrations/
  ├── 001_create_user_themes.up.sql    # CREATE TABLE ...
  └── 001_create_user_themes.down.sql  # DROP TABLE IF EXISTS user_themes;
  ```
- ロールバック: `sqlx migrate revert` で直前のマイグレーションを取り消す
- Bot起動時に `sqlx::migrate!().run(&pool).await` で自動適用

### バックアップ・障害復旧

- Supabase ダッシュボードの自動バックアップ機能を利用可能
- 運用スクリプトで日次バックアップを推奨:
  ```bash
  # cron等で日次実行
  pg_dump "$DATABASE_URL" > "backup/blaze_$(date +%Y%m%d).sql"
  ```
- テーマデータは復旧不能でもサービス継続に支障なし（デフォルトテーマにフォールバック）
- PostgreSQL は MVCC により読み取り/書き込みの並行性をネイティブに確保

---

## 依存クレート (Cargo.toml)

```toml
[dependencies]
# Discord (serenity は poise が re-export するため直接依存しない)
poise = "0.6"
tokio = { version = "1", features = ["full"] }

# 構文解析
syntect = { version = "5", default-features = false, features = ["default-syntaxes", "default-themes", "regex-onig"] }

# 画像生成
resvg = "0.45"
tiny-skia = "0.11"

# データベース
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "migrate", "chrono"] }

# エラーハンドリング
thiserror = "2"

# レート制限
governor = "0.8"

# 設定管理
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# 入力サニタイズ
unicode-normalization = "0.1"

# ロギング・監視
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }

# ユーティリティ
dotenvy = "0.15"
base64 = "0.22"
regex = "1"

[dev-dependencies]
insta = "1"            # スナップショットテスト
```

---

## 実装フェーズ

| フェーズ | 内容 | 目標 |
|---------|------|------|
| **Phase 1** | Bot起動 + コンテキストメニュー登録 + コードブロック抽出 | Discordとの接続確立 |
| **Phase 2** | syntectでハイライト → 最小限のSVG生成 → PNG変換 | 基本的な画像出力 |
| **Phase 3** | ガラスエフェクト・タイトルバー・影・角丸の実装 | ビジュアル完成 |
| **Phase 4** | PostgreSQL (Supabase) + テーマ保存/読み込み + `/theme` コマンド群 | パーソナライズ |
| **Phase 5** | フォント埋め込み・背景画像バリエーション・パフォーマンス最適化 | 本番品質 |
| **Phase 6** | 独自エラー型 + レート制限 + 入力サニタイズ + 設定管理強化 | 堅牢性・セキュリティ |
| **Phase 7** | ロギング基盤整備 + メトリクス収集 | 運用監視 |
| **Phase 8** | 行番号表示オプション + 複数コードブロック対応 | UX向上 |

※ 各フェーズはRGBCサイクル（Red→Green→Blue→Commit）で進行する

---

## 設計上のポイント

- **`Renderer` は `Arc` で共有**: `SyntaxSet` / `ThemeSet` / `fontdb` は起動時に一度だけロードし、全リクエストで再利用。読み取り専用のためロック不要
- **SVGは文字列として動的生成**: テンプレートエンジン不要。`format!` / `write!` で組み立てるのが最もシンプルかつ高速
- **背景画像はBase64でSVGに埋め込み**: 外部ファイル参照を避け、resvgが単体でレンダリングできるようにする
- **レンダリングは `spawn_blocking`**: resvgのラスタライズはCPUバウンドなので `tokio::task::spawn_blocking` で実行し、非同期ランタイムをブロックしない
- **コードブロック抽出は正規表現**: `` ```(\w*)\n([\s\S]*?)``` `` で言語タグとコード本体を分離
- **Graceful Shutdown**: `tokio::signal` で SIGINT / SIGTERM を受け取り、処理中の `spawn_blocking` タスク（レンダリング）が完了してからBotプロセスを終了する
- **Discord API 429 リトライ**: serenity/poise はDiscord APIからの `429 Too Many Requests` レスポンスに対して、`Retry-After` ヘッダに基づく自動リトライを内蔵している。独自のリトライロジックは実装しない。ただし、Bot側のレート制限（`governor`）でリクエスト頻度を事前に抑制し、429を極力発生させない設計とする
- **レンダリング同時実行数制御**: `tokio::sync::Semaphore` で `spawn_blocking` の同時実行数を `Settings.max_concurrent_renders`（デフォルト: 4）に制限し、CPU飽和を防止する
- **fontdb と resvg の連携**: `usvg::Options::fontdb_mut()` 経由でフォントを登録する。`resvg::render()` はこの `fontdb` を参照してグリフを解決するため、`load_fonts()` で登録したフォントが確実にテキスト描画に使われる

---

## テスト戦略

### ユニットテスト（各ファイル内の `#[cfg(test)] mod tests`）

| 対象 | テストケース |
|------|------------|
| `extract_code_block` | 空文字、言語タグあり/なし、最大文字数境界、複数ブロック、ネストしたバッククォート |
| `sanitize_code` | 制御文字除去、タブ/改行保持、Unicode正規化（NFC）、ゼロ幅文字 |
| `escape_for_svg` | `&`, `<`, `>`, `"` のエスケープ |
| `highlight` | Rust/Python/Go 各言語のトークン化、未知言語のフォールバック |
| `svg_builder` | 行数に応じたSVG高さ計算、テーマ設定の反映、フォント指定 |
| `BlazeError` | 各バリアントの `Display` 出力、`From` 変換 |
| `Settings` | デフォルト値、バリデーション（不正値の拒否） |

### 統合テスト（`tests/` ディレクトリ）

| ファイル | 内容 |
|---------|------|
| `render_pipeline.rs` | コード入力 → PNG バイト列出力までのE2Eテスト |
| `theme_repository.rs` | PostgreSQLに対するCRUD操作（テスト用DB使用） |

### スナップショットテスト（`insta` クレート）

レンダリング出力の視覚的回帰を検知する。依存ライブラリ更新やロジック変更で意図しない出力変化が起きた際にCIで自動検出する。

- 特定の入力コード + テーマの組み合わせに対する **SVG文字列** をスナップショットとして保存
- `cargo insta test` で差分を検出、`cargo insta review` で意図した変更を承認
- PNG のバイナリ比較は環境差が出やすいため、SVG文字列レベルでのスナップショットを推奨

### テスト命名規則

`{対象}_{条件}_{期待結果}` の形式（例: `extract_code_block_empty_input_returns_none`）

---

## 監視・ロギング設計

### ログレベル運用

| レベル | 用途 |
|-------|------|
| `ERROR` | レンダリング失敗、DB接続エラー |
| `WARN` | レート制限超過、リソース制限超過 |
| `INFO` | Bot起動完了、レンダリング成功、テーマ変更 |
| `DEBUG` | 詳細な処理フロー（SVG生成内容、クエリパラメータ等） |

### メトリクス（将来導入）

| メトリクス名 | タイプ | 説明 |
|-------------|--------|------|
| `blaze_render_total` | Counter | 総レンダリング回数 |
| `blaze_render_duration_seconds` | Histogram | レンダリング処理時間 |
| `blaze_render_errors_total` | Counter | エラー発生回数（種別ラベル付き） |
| `blaze_active_users` | Gauge | アクティブユーザー数 |
| `blaze_db_query_duration` | Histogram | DBクエリ時間 |

---

## 本番環境向け改善要件

### 1. 悪意のある入力への対策（リソース制限）

不特定多数のユーザーからの巨大な入力によるOOMや処理遅延を防ぐ。

- コード抽出直後にハードリミット（最大100行 / 最大4000文字）のバリデーションを実施
- 超過時はエフェメラルメッセージで通知し、`Ok(())` で正常終了（システムエラーにしない）

### 2. CJK（日本語等）フォントのフォールバック処理

コード内の日本語コメントの文字化け（豆腐化）を防ぐ。

- SVG の `font-family` 属性: `"Fira Code", "PlemolJP", "HackGen Console NF", sans-serif`（ユーザー設定に応じて変動）
- `Renderer::new()` 内で `load_fonts()` を呼び出し、英字・日本語フォントの全3種をロード
- フォント読み込みの実装詳細は「レンダラー」型定義および改善要件 #11 を参照

### 3. UXを考慮したエラーハンドリング

対象メッセージにコードブロックが無い場合の挙動改善。

- `extract_code_block` は `Option<CodeBlock>` を返す（`Result` ではなく）
- `None` の場合は「メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした」とエフェメラルで通知し、`Ok(())` で終了
- 内部ログにエラーノイズを残さない

### 4. パフォーマンス最適化（DB層の分離とキャッシュ準備）

DBアクセスの抽象化により、将来的なインメモリキャッシュ導入を容易にする。

- テーマ取得を `ThemeRepository` トレイトで抽象化:
  ```rust
  // db/mod.rs
  pub trait ThemeRepository {
      async fn get_theme(&self, user_id: u64) -> Result<Option<UserTheme>, BlazeError>;
      async fn upsert_theme(&self, theme: &UserTheme) -> Result<(), BlazeError>;
      async fn delete_theme(&self, user_id: u64) -> Result<(), BlazeError>;
  }
  ```
- 初期実装は `PgThemeRepository`（PostgreSQL直叩き）
- 将来的に `moka` 等のインメモリキャッシュを前段に挟む `CachedThemeRepository` に差し替え可能

### 5. レート制限

`governor` クレートでユーザーごとのリクエスト頻度を制限する。

- デフォルト: 1分間に10リクエスト（`Settings.rate_limit_per_minute` で変更可能）
- `Data` 構造体に `DefaultKeyedRateLimiter<u64>` を保持
- 超過時はエフェメラルメッセージで通知し、`Ok(())` で正常終了

### 6. 独自エラー型

`Box<dyn Error>` を `BlazeError`（`thiserror` ベース）に置換する。

- バリアント: `CodeBlockNotFound`, `CodeTooLong { max_lines, max_chars }`, `Database`, `Rendering { message, source }`, `RateLimitExceeded`, `InvalidTheme`, `Config`
- `sqlx::Error`, `syntect::Error` からの `From` 実装で自動変換
- ユーザー向けメッセージとログ向けメッセージを分離可能

### 7. 入力サニタイズ

SVGインジェクション防止と文字化け防止のための入力正規化。

- `sanitize_code`: 制御文字除去（タブ・改行は許可）、Unicode NFC正規化
- `escape_for_svg`: `&`, `<`, `>`, `"` をSVGエンティティにエスケープ
- SVG生成時にすべてのユーザー入力テキストを `escape_for_svg` 経由で出力

### 8. 設定管理の強化

ハードコードされた定数を設定ファイル + 環境変数で管理する。

- `config/default.toml` にデフォルト値を定義
- 環境変数 `BLAZE_*` でオーバーライド可能（例: `BLAZE_MAX_CODE_LINES=150`）
- `Settings` 構造体で型安全にデシリアライズ
- Bot起動時に `Settings::validate()` を呼び出し、不正値なら起動を中断する（実装詳細は「設定管理」型定義を参照）

### 9. SVGでの空白・インデント崩れ対策

PythonやGoなどインデントが重要な言語で、コードが左詰めにレンダリングされる問題を防止する。

- `svg_builder.rs`: コード描画の `<text>` タグに `xml:space="preserve"` を必ず付与
- `sanitize_code()`: タブ文字（`\t`）を半角スペース4つに展開（Expand Tabs）
- この2つを組み合わせることで、連続スペースの圧縮を防ぎ、タブ幅の環境依存も排除

### 10. フォント埋め込み方式とバイナリサイズ

`include_bytes!` によるフォント埋め込みは単一バイナリでデプロイできる利点があるが、日本語フォントや複数ウェイト追加でバイナリが肥大化するリスクがある。

- 初期実装: `include_bytes!` による静的埋め込み（デプロイの容易さ優先）
- `load_fonts()` 関数として切り出し、`Renderer` 本体と疎結合にしておく
- 将来: `std::fs::read("assets/fonts/...")` による動的読み込みへシームレスに切り替え可能

### 11. 複数コードブロック対応（将来）

1メッセージに複数の ``` ブロックがある場合の挙動。

- 初期実装: 最初のコードブロックのみ処理
- 将来: Discord のボタンUIで選択、または全ブロックを連結した1枚の画像を生成

### 12. 画像キャッシュによる重複レンダリング防止（将来）

同一コード + 同一テーマの組み合わせで重複レンダリングを回避する。

- キャッシュキー: `blake3::hash(code_bytes + theme_hash)` — コード内容とテーマ設定のハッシュ値
- キャッシュストア: `moka` のインメモリキャッシュ（TTL: 10分、最大エントリ数: 200）
- PNG バイト列をキャッシュ値として保持（1画像あたり概算 50KB〜200KB、最大約40MB）
- ヒット時は `spawn_blocking` を完全にスキップし、キャッシュから直接返却
- 初期実装ではキャッシュなしで進め、パフォーマンス計測後に導入を判断する

### 13. 横方向の長い行の制御

ミニファイされたコード等の極端に長い行による画像幅の爆発を防止する。

- `Settings` に `max_line_length`（デフォルト: 120文字）を追加
- `svg_builder` で超過行をトリミングし、末尾に `...` を表示する
- 画像幅は `max_line_length` に基づいて固定し、Discordの表示領域を超過しない

### 14. 背景画像のメモリ効率最適化

高解像度の背景画像によるSVGデータ肥大化とメモリ消費を抑制する。

- SVG埋め込み前に、ウィンドウサイズ（出力解像度）に合わせて背景画像を事前リサイズする
- `image` クレート等で縮小 → Base64エンコード → SVG埋め込みの順で処理
- 不要なピクセルデータを削減し、ラスタライズ時のメモリと処理時間を最適化

### 15. 行番号表示（将来）

コード画像に行番号を付加するオプション。

- DBスキーマに `show_line_numbers` カラムを初期段階から含めておく（マイグレーションコスト回避）
- `/theme set` コマンドに `show_line_numbers: Option<bool>` パラメータを追加
- SVG生成時に行番号列を左側に追加（固定幅・薄い色で表示）

### 16. 国際化（i18n）対応（将来）

ユーザー向けメッセージの多言語対応。

- 現状: 対象ユーザーが日本語話者のため、日本語ハードコードで問題ない
- 将来: グローバル展開時は `rust-i18n` 等のクレートでメッセージカタログ（YAML/JSON）を導入
- Discordサーバーのロケール or ユーザーのロケール設定に基づいて表示言語を切り替え
