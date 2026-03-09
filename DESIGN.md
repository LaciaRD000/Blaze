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
│   └── backgrounds/              # 背景画像 (denim.webp, repeated-square-dark.webp)
├── build.rs                      # ビルド時に syntect の SyntaxSet/ThemeSet を packdump 生成
├── src/
│   ├── main.rs                   # エントリポイント、Bot起動（モノリスモード）
│   ├── bin/
│   │   ├── gateway.rs            # Gateway バイナリ（Discord I/O + Redis キュー）
│   │   └── worker.rs             # Worker バイナリ（レンダリング処理）
│   ├── handlers.rs               # 共通エラーハンドラ (on_error)
│   ├── protocol.rs               # マイクロサービス間通信の型定義 (RenderJob, RenderResult)
│   ├── config.rs                 # 設定管理 (Settings構造体、バリデーション)
│   ├── error.rs                  # BlazeError 独自エラー型 (thiserror)
│   ├── sanitize.rs               # 入力サニタイズ・SVGエスケープ
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── render.rs             # コンテキストメニュー「ターミナル画像化」
│   │   └── theme.rs              # /theme set, /theme preview 等
│   ├── renderer/
│   │   ├── mod.rs                # レンダリングパイプライン統括 (Renderer + BackgroundCache + ShadowCache + FontSet)
│   │   ├── background.rs         # 背景画像キャッシュ・タイリング・グラデーション生成
│   │   ├── canvas.rs             # fontdue + tiny_skia による直接描画（SVGパイプライン排除）
│   │   ├── highlight.rs          # syntect によるトークン化・色付け
│   │   ├── svg_builder.rs        # SVG文字列の組み立て（スナップショットテスト専用）
│   │   └── rasterize.rs          # 直接描画 + シャドウ合成 + 背景合成 → PNG変換 (ShadowCache)
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
  → 入力サニタイズ (制御文字除去、Unicode正規化)
  → 言語自動判定 (syntect SyntaxSet)
  → トークン化 & 色付け (renderer/highlight.rs)
  → ユーザーテーマ取得 (db/ → キャッシュ層 → UserTheme or デフォルト)
  → 直接描画 (renderer/canvas.rs)
      - FontSet (fontdue) でグリフをラスタライズし、tiny_skia::Pixmap に直接描画
      - 角丸矩形、タイトルバー（macOS/Linux/plain/none）、コード行をすべて tiny_skia PathBuilder で構築
      - フォントフォールバック: ユーザー設定のプライマリフォント → フォールバック（Fira Code→PlemolJP, PlemolJP→Fira Code, HackGen NF→PlemolJP）
      - ※ SVG (usvg/resvg) パイプラインを完全に排除
  → PNG ラスタライズ + 背景合成 (renderer/rasterize.rs)
      - ドロップシャドウ: ShadowCache からサイズ別にキャッシュ取得（ヒット時は即座に返却）
        - キャッシュミス時: tiny_skia で矩形描画 → 1/4ダウンスケール+ぼかし → キャッシュに格納
        - 合成時: draw_pixmap で 8x アップスケール合成
      - 背景あり: ShadowCache → キャッシュ取得（即座）、背景ぼかし+コード描画を並列実行（2スレッド）
      - 背景なし: ShadowCache → キャッシュ取得 → コード Pixmap を合成 → PNG bytes
      - 2x スケールで高解像度レンダリング（Discord の高DPI表示に対応）
  → Discord に通常メッセージとしてリプライ (画像添付、全員に表示)
  → メトリクス記録 (レンダリング回数、処理時間)
```

---

## マイクロサービスアーキテクチャ（Gateway/Worker 分離）

モノリス（`src/main.rs`）に加え、Gateway/Worker に分離したマイクロサービス構成をサポートする。モノリスは後方互換性のために維持される。

### デプロイモード

| モード | バイナリ | 説明 |
|--------|---------|------|
| モノリス | `blaze-bot`（デフォルト） | 従来の単一プロセス。`REDIS_URL` 未設定時はこのモードで動作 |
| マイクロサービス | `blaze-gateway` + `blaze-worker` | Discord I/O とレンダリングを分離。水平スケーリング対応 |

### マイクロサービス データフロー

```
ユーザー右クリック
  → [Discord Gateway]
  → blaze-gateway (src/bin/gateway.rs)
      - 起動時に Redis MultiplexedConnection を確立し Data に保持（リクエスト毎の再接続を排除）
      - poise コマンドハンドラ
      - レート制限チェック (governor)
      - 入力バリデーション・サニタイズ
      - DB クエリ（ユーザーテーマ取得）
      - RenderJob を JSON シリアライズ
      - Data.redis から Clone した接続で LPUSH → Redis リスト `blaze:jobs`
      - BRPOP ← Redis リスト `blaze:results:{job_id}` で結果を待機
      - PNG を Discord にリプライ送信

  → Redis キュー (`blaze:jobs`)

  → blaze-worker (src/bin/worker.rs)
      - BRPOP ← Redis リスト `blaze:jobs` でジョブを取得
      - spawn_blocking で CPU バウンドなレンダリングを実行
      - 1プロセス1ジョブの同期処理（並行処理は Worker の複数起動で実現）
      - RenderResult を JSON シリアライズ
      - LPUSH → Redis リスト `blaze:results:{job_id}`（TTL 60秒）
```

### プロトコル型 (src/protocol.rs)

```rust
/// Gateway → Worker: レンダリングジョブ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJob {
    pub job_id: String,          // UUID v4
    pub code: String,
    pub language: Option<String>,
    pub theme_name: String,      // syntect テーマ名
    pub options: RenderJobOptions,
}

/// レンダリングオプション（RenderOptions の Serialize 対応版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJobOptions {
    pub title_bar_style: String,
    pub opacity: f64,
    pub blur_radius: f64,
    pub show_line_numbers: bool,
    pub max_line_length: Option<usize>,
    pub background_image: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,  // None でデフォルト (Fira Code)
}

/// Worker → Gateway: レンダリング結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderResult {
    /// レンダリング成功。PNG バイト列を含む
    Success { png_bytes: Vec<u8> },
    /// レンダリング失敗。エラーメッセージを含む
    Error { message: String },
}
```

### スケーリング

- 各 Worker は1プロセスあたり1ジョブずつ同期的に処理する（セマフォや内部並行処理は不要）
- 複数の Worker プロセスを起動することで水平スケーリングが可能
- Redis リストの BRPOP により、ジョブは自動的に空いている Worker に分配される
- Gateway は Discord I/O に専念し、CPUバウンドなレンダリング処理を Worker に委譲する

---

## 主要な型定義

### Bot データ (src/lib.rs)

```rust
pub struct Data {
    pub db: sqlx::PgPool,
    pub renderer: Arc<renderer::Renderer>,
    pub rate_limiter: Arc<governor::DefaultKeyedRateLimiter<u64>>,
    pub render_semaphore: Arc<tokio::sync::Semaphore>,  // spawn_blocking 同時実行数制御
    pub settings: Arc<Settings>,
    /// Gateway モードで Worker に委譲する際の Redis 接続（Monolith では None）
    /// MultiplexedConnection は Clone 可能で内部で多重化されるため Mutex 不要
    pub redis: Option<redis::aio::MultiplexedConnection>,
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
    pub max_line_length: usize,     // デフォルト: 120 (超過行はトリミング + "…")
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
use std::sync::LazyLock;

/// コードブロック抽出用の正規表現（LazyLock でコンパイル結果をキャッシュ）
static CODE_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"```(\w*)\n([\s\S]*?)```").expect("正規表現のコンパイルに失敗")
});

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
/// LazyLock でキャッシュ済みの正規表現を使用（毎回のコンパイルを排除）
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
    pub color_scheme: String,   // syntect テーマ名 (e.g. "base16-eighties.dark")
    pub background_id: String,  // 背景画像識別子 (e.g. "default", "gradient", "denim", "repeated-square-dark")
    pub blur_radius: f32,       // ガウスぼかし強度 (0.0 - 30.0)
    pub opacity: f32,           // ウィンドウ不透明度 (0.3 - 1.0)
    pub font_family: String,    // フォント名 (e.g. "Fira Code")
    pub title_bar_style: String,    // "macos" | "linux"
    pub show_line_numbers: bool,    // 行番号表示 (Phase 8 で実装、スキーマは先行準備)
}
```

### レンダラー (src/renderer/mod.rs)

```rust
pub struct Renderer {
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub font_db: Arc<usvg::fontdb::Database>,  // SVGスナップショットテスト用に保持
    pub font_set: canvas::FontSet,              // デフォルトの fontdue フォントセット（Fira Code）
    font_sets: HashMap<String, canvas::FontSet>, // フォントファミリー名 → FontSet（各フォント個別のグリフキャッシュを保持）
    pub shadow_cache: rasterize::ShadowCache,   // シャドウ Pixmap サイズ別キャッシュ
    pub background_cache: BackgroundCache,      // WebP デコード済みタイルをキャッシュ
}

impl Renderer {
    pub fn new() -> Self {
        // build.rs が生成した非圧縮 packdump からロード（起動時の解凍処理を省略）
        let syntax_set: SyntaxSet = syntect::dumps::from_uncompressed_data(
            include_bytes!(concat!(env!("OUT_DIR"), "/syntax_set.packdump"))
        ).expect("SyntaxSet packdump の読み込みに失敗");
        let theme_set: ThemeSet = syntect::dumps::from_uncompressed_data(
            include_bytes!(concat!(env!("OUT_DIR"), "/theme_set.packdump"))
        ).expect("ThemeSet packdump の読み込みに失敗");
        let mut font_db = usvg::fontdb::Database::new();
        load_fonts(&mut font_db);
        let font_set = canvas::FontSet::new();
        // 全フォントファミリー分の FontSet をプリロード
        let mut font_sets = HashMap::new();
        font_sets.insert("Fira Code".into(), canvas::FontSet::with_family(canvas::FontFamily::FiraCode));
        font_sets.insert("PlemolJP".into(), canvas::FontSet::with_family(canvas::FontFamily::PlemolJP));
        font_sets.insert("HackGen Console NF".into(), canvas::FontSet::with_family(canvas::FontFamily::HackGenNF));
        let shadow_cache = rasterize::ShadowCache::new();
        let background_cache = BackgroundCache::new();

        Self { syntax_set, theme_set, font_db: Arc::new(font_db), font_set, font_sets, shadow_cache, background_cache }
    }

    /// 直接描画パイプライン: highlight → canvas.rs (fontdue + tiny_skia) → PNG
    /// SVG パイプライン (usvg/resvg) を完全に排除
    pub fn render_with_options(&self, code, language, theme_name, options) -> Result<Vec<u8>> {
        let lines = highlight::highlight(code, language, &self.syntax_set, theme);
        let font_set = self.resolve_font_set(options.font_family.as_deref());
        let canvas_options = canvas::CanvasOptions { /* テーマ設定から構築 */ };
        if let Some(bg_id) = &options.background_image {
            let bg_pixmap = background::load_background_pixmap(&self.background_cache, bg_id, w, h)?;
            rasterize::rasterize_direct_with_background(&lines, font_set, &canvas_options, &self.shadow_cache, bg_pixmap, blur, margin)
        } else {
            rasterize::rasterize_direct(&lines, font_set, &canvas_options, &self.shadow_cache)
        }
    }

    /// font_family 名から FontSet を解決する。不明な名前はデフォルト (Fira Code) にフォールバック
    fn resolve_font_set(&self, font_family: Option<&str>) -> &canvas::FontSet;
}

/// BackgroundCache: WebP 背景画像を起動時にデコードしてキャッシュ
/// リクエストごとの WebP デコードを排除し、Pixmap タイリングのみで背景を生成
pub struct BackgroundCache {
    denim: image::RgbaImage,
    repeated_square_dark: image::RgbaImage,
}

/// ShadowCache: シャドウ Pixmap を (svg_width, svg_height) でキャッシュ
/// シャドウはコード内容やテーマに依存せず、サイズのみで決まるため高いヒット率を実現
/// 幅は常に 864px、高さは行数+タイトルバースタイルで決まるため、パターン数は高々 ~50
/// RwLock で読み取りは共有ロック、Arc で Pixmap clone を pointer clone に置換
pub struct ShadowCache {
    cache: RwLock<HashMap<(u32, u32), Arc<tiny_skia::Pixmap>>>,
}

impl ShadowCache {
    pub fn new() -> Self;
    /// キャッシュヒット時は Arc clone（pointer clone）を返し、ミス時は生成してキャッシュに格納
    pub fn get_or_create(&self, svg_width: f32, svg_height: f32) -> Result<Arc<tiny_skia::Pixmap>, BlazeError>;
}

fn load_fonts(font_db: &mut usvg::fontdb::Database) {
    font_db.load_font_data(include_bytes!("../assets/fonts/FiraCode-Regular.ttf").to_vec());
    font_db.load_font_data(include_bytes!("../assets/fonts/PlemolJP-Regular.ttf").to_vec());
    font_db.load_font_data(include_bytes!("../assets/fonts/HackGenConsoleNF-Regular.ttf").to_vec());
}
```

### 直接描画モジュール (src/renderer/canvas.rs)

```rust
/// ユーザーが選択可能なフォントファミリー
pub enum FontFamily { FiraCode, PlemolJP, HackGenNF }

/// フォントセット（fontdue によるグリフラスタライズ）
/// FontFamily に応じてプライマリフォントを切り替え、もう一方をフォールバックとする
/// 各 FontSet は個別の RwLock グリフキャッシュを保持し、フォント間でキャッシュが汚染されない
pub struct FontSet {
    primary: fontdue::Font,   // ユーザー選択のプライマリフォント
    fallback: fontdue::Font,  // フォールバックフォント
    glyph_cache: RwLock<HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>>,
}

impl FontSet {
    /// デフォルト (Fira Code) で構築
    pub fn new() -> Self { Self::with_family(FontFamily::FiraCode) }

    /// 指定フォントファミリーをプライマリとした FontSet を構築
    /// FiraCode → fallback: PlemolJP, PlemolJP → fallback: FiraCode, HackGenNF → fallback: PlemolJP
    pub fn with_family(family: FontFamily) -> Self;

    /// 文字に対応するフォントを選択してラスタライズ
    /// primary の lookup_glyph_index が 0 なら fallback にフォールバック
    fn rasterize_char(&self, ch: char, px: f32) -> (fontdue::Metrics, Vec<u8>);

    /// キャッシュ付きラスタライズ: 同一 (char, px) は再利用する
    pub fn rasterize_cached(&self, ch: char, px: f32) -> (fontdue::Metrics, Vec<u8>);
}

/// キャンバス描画オプション
pub struct CanvasOptions<'a> {
    pub bg_color: [u8; 3],
    pub opacity: f32,
    pub title_bar_style: &'a str,
    pub language: Option<&'a str>,
    pub max_line_length: Option<usize>,
    pub show_line_numbers: bool,
}

/// ハイライト済みコード行を直接 tiny_skia::Pixmap に描画する
/// 描画プリミティブ（角丸矩形、円、線、テキスト）はすべて tiny_skia PathBuilder で構築
pub fn render_code_pixmap(
    lines: &[HighlightedLine],
    font_set: &FontSet,
    options: &CanvasOptions,
    scale: f32,
) -> Result<tiny_skia::Pixmap, BlazeError>;
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

### グローバルエラーハンドラ (src/handlers.rs)

`BlazeError` をユーザー向けのエフェメラルメッセージに変換する共通ハンドラ。`main.rs`（モノリス）と `gateway.rs`（マイクロサービス）の両方から参照される。内部エラーの詳細はログに記録し、ユーザーには汎用メッセージのみ返す。

```rust
// src/handlers.rs
pub async fn on_error(error: poise::FrameworkError<'_, Data, BlazeError>) {
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

// Framework構築時に設定（main.rs / gateway.rs 共通）:
// poise::FrameworkOptions {
//     on_error: |err| Box::pin(blaze_bot::handlers::on_error(err)),
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

## SVGテンプレート構造 (svg_builder.rs) — スナップショットテスト専用

svg_builder.rs はメインのレンダリングパスでは使用されない。SVG スナップショットテスト（`insta`）でレンダリング出力の回帰を検知するためにのみ使用する。

生成するSVGの論理構造（コードのみ。背景画像は含まない）:

```svg
<svg width="..." height="...">
  <!-- ウィンドウ本体 (角丸 + 半透明) -->
  <!-- ドロップシャドウは SVG に含めず、rasterize 側で tiny_skia により直接描画 -->
  <g>
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
    color_scheme    TEXT NOT NULL DEFAULT 'base16-eighties.dark',
    background_id   TEXT NOT NULL DEFAULT 'gradient',
    blur_radius     DOUBLE PRECISION NOT NULL DEFAULT 8.0,
    opacity         DOUBLE PRECISION NOT NULL DEFAULT 0.75,
    font_family     TEXT NOT NULL DEFAULT 'Fira Code',
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

# 構文解析（ランタイム: packdump からロードするため default-syntaxes/default-themes 不要）
syntect = { version = "5", default-features = false, features = ["parsing", "dump-load", "regex-onig"] }

# 画像生成（直接描画パイプライン）
fontdue = "0.9"          # グリフラスタライズ（SVGパイプライン排除のコア）
tiny-skia = "0.11"       # 2Dレンダリング（PathBuilder, Pixmap）
image = { version = "0.25", default-features = false, features = ["webp", "png"] }  # WebP背景画像デコード・タイリング
resvg = "0.45"           # SVGスナップショットテスト用（メインパスでは未使用）

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

# マイクロサービス間通信
redis = { version = "0.27", features = ["tokio-comp"] }  # Redis キュー (Gateway/Worker 分離時)
serde_json = "1"                                          # プロトコル JSON シリアライズ
uuid = { version = "1", features = ["v4"] }               # ジョブ ID 生成

# ユーティリティ
dotenvy = "0.15"
regex = "1"

[build-dependencies]
# ビルド時に SyntaxSet/ThemeSet の packdump を生成
syntect = { version = "5", features = ["default-syntaxes", "default-themes", "dump-create", "dump-load", "regex-onig"] }
# デフォルトに含まれないモダン言語の構文定義を追加（TypeScript, Kotlin, TOML 等 80+言語）
two-face = { version = "0.5", default-features = false, features = ["syntect-default-onig"] }

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
| **Phase 9** | syntect バイナリダンプ化 + Gateway/Worker 分離 | 高度なアーキテクチャ最適化 |
| **Phase 10** | ぼかし直接ピクセル操作 + PNG 高速エンコード | レンダリング高速化 |
| **Phase 11** | シャドウ1/4ダウンスケール + resvg直接描画 + 並列実行 + font-family集約 | パイプライン高速化（823ms→143ms, 83%削減） |
| **Phase 12** | SVGパイプライン排除 + fontdue/tiny_skia直接描画 (canvas.rs) | SVGパイプライン完全排除（143ms→88ms, 累積89%削減） |
| **Phase 14** | シャドウ Pixmap サイズ別キャッシュ (ShadowCache) | キャッシュヒット時 ~28ms 短縮（101ms→73ms, 50行背景なし） |
| **Phase 15** | グリフキャッシュ + PNG最速化 + Pixmap二重確保排除 + draw_glyphクリッピング + Cow + ShadowCache RwLock+Arc | レンダリング高速化（73ms→39ms, 46%削減, 累積95%削減） |

※ 各フェーズはRGBCサイクル（Red→Green→Blue→Commit）で進行する

---

## 設計上のポイント

- **syntect Binary Dump (`build.rs`)**: `build.rs` がビルド時に `two_face::syntax::extra_newlines()` / `ThemeSet::load_defaults()` を実行し、非圧縮 packdump ファイルを `OUT_DIR` に生成する。`two-face` クレートにより、syntect のデフォルトに含まれない TypeScript, Kotlin, Swift, Dart, Elixir, TOML, Zig, Dockerfile, Terraform, Vue, Svelte, Nix 等 80 以上の言語をサポートする。ランタイムでは `syntect::dumps::from_uncompressed_data()` で読み込むことで、起動時の解凍処理を省略する
- **Gateway/Worker 分離**: Discord I/O（Gateway）と CPUバウンドなレンダリング（Worker）を別プロセスに分離するマイクロサービス構成をサポートする。Redis リストベースのキューで通信し、Worker の水平スケーリングが可能。モノリスモードも後方互換として維持する
- **`Renderer` は `Arc` で共有**: `SyntaxSet` / `ThemeSet` / `fontdb` は起動時に一度だけ packdump からロードし、全リクエストで再利用。読み取り専用のためロック不要
- **SVGパイプライン排除（Phase 12）**: メインのレンダリングパスから usvg/resvg を完全に排除。fontdue でグリフを個別にラスタライズし、tiny_skia の PathBuilder で描画プリミティブ（角丸矩形、円、線）を構築して直接 Pixmap に描画する。usvg の SVG パース（~50ms）を完全に排除し、38%の高速化を実現
- **canvas.rs 直接描画モジュール**: `FontSet` が `FontFamily` enum に応じたプライマリ＋フォールバックの fontdue::Font と `RwLock<HashMap>` ベースのグリフキャッシュを保持。`Renderer` は全3フォント（Fira Code / PlemolJP / HackGen Console NF）の `FontSet` を `HashMap` でプリロードし、`RenderOptions.font_family` に応じて選択する。`render_code_onto_pixmap()` がハイライト済みコード行を受け取り、既存の Pixmap に直接描画する（Pixmap 二重確保を回避）。各文字は `lookup_glyph_index` でフォントを選択し、`rasterize_cached()` でキャッシュ付きラスタライズを行い、事前クリッピング計算で per-pixel bounds check を排除した `draw_glyph` で α ブレンド
- **svg_builder.rs はスナップショットテスト専用**: SVG 文字列生成はメインパスでは使用されない。`insta` によるスナップショットテストでレンダリング出力の視覚的回帰を検知するためにのみ保持
- **背景画像は Pixmap で直接合成**: SVG に Base64 埋め込みせず、rasterize 側で背景 Pixmap（ぼかし済み）とコード Pixmap を合成する。WebP デコード結果は `BackgroundCache` で起動時にキャッシュし、リクエストごとの再デコードを排除
- **ドロップシャドウの直接描画**: SVG の `feDropShadow` フィルタを除去し、tiny_skia で矩形を描画 → `image::imageops::blur` でぼかし → 2xスケールで合成。resvg 内部のフィルタ処理（パイプライン全体の30〜50%を占めていた）を回避
- **シャドウの1/4ダウンスケール**: `create_shadow_pixmap` は1/4サイズで描画+ぼかしを行い、ダウンスケールされた Pixmap を返す。合成時に `draw_pixmap` が `SHADOW_DRAW_SCALE`（= SCALE × 4.0 = 8.0）でアップスケールするため、upscale 処理を `draw_pixmap` に委ね、中間 Pixmap のメモリ確保を削減
- **ぼかし処理のダウンスケール最適化**: 背景ぼかしは1/2にダウンスケール → blur_radius も1/2に。`blur_pixmap` はダウンスケールされた Pixmap とスケール倍率のタプルを返し、元サイズへの復元は `draw_pixmap` のスケール変換に委ねる。ぼかし計算量を約1/4に削減（ぼかし後は細部が消えるため品質劣化なし）
- **resvg 直接描画（SVGパス）**: SVG ベースのレガシーパス（`rasterize()` / `rasterize_with_background()`）では、中間の `code_pixmap` を確保せず `resvg::render()` で直接 `final_pixmap` に描画する。ただしメインパスでは使用されない
- **rasterize_direct / rasterize_direct_with_background**: SVG を経由しない新しいエントリポイント。`rasterize_direct` は `render_code_onto_pixmap()` で `final_pixmap` に直接描画し、中間 Pixmap の確保を排除。`&ShadowCache` を引数に取り、シャドウ Pixmap を `Arc` でキャッシュから取得する
- **ShadowCache によるシャドウ Pixmap キャッシュ（Phase 14→15）**: `ShadowCache`（`RwLock<HashMap<(u32, u32), Arc<tiny_skia::Pixmap>>>`）がシャドウ Pixmap を `(svg_width, svg_height)` でキャッシュする。RwLock で読み取りは共有ロック、Arc で Pixmap clone（数十KB memcpy）を pointer clone に置換。シャドウはコード内容やテーマに依存せず、サイズのみで決まるため高いヒット率を実現。幅は常に 864px、高さは行数+タイトルバースタイルで決まるため、パターン数は高々 ~50
- **背景ぼかし・コード描画の並列実行**: `rasterize_direct_with_background()` ではシャドウを `ShadowCache` から即座に取得し、`std::thread::scope` で `blur_pixmap` と `render_code_pixmap` の2処理を並列実行する（キャッシュ導入前は3スレッドだったが、シャドウ生成が不要になり2スレッドに削減）
- **ぼかし処理の直接ピクセル操作**: 背景へのガウスぼかしは `image::imageops::blur` で直接適用する。従来の SVG 経由（Pixmap→PNG encode→Base64→SVG→resvg）の6段パイプラインを1段に簡素化
- **PNG 高速エンコード**: Discord は画像アップロード時に再圧縮するため、Bot 側では `png` crate を直接利用し `Compression::Fast` + `FilterType::Sub`（Sub フィルタは隣接ピクセルの差分をとることで圧縮効率を改善） で高速にエンコードする。`image` crate 経由より直接的で、`Vec::with_capacity` による事前確保でアロケーションも最適化
- **レンダリングは `spawn_blocking`**: resvgのラスタライズはCPUバウンドなので `tokio::task::spawn_blocking` で実行し、非同期ランタイムをブロックしない
- **コードブロック抽出は正規表現**: `` ```(\w*)\n([\s\S]*?)``` `` で言語タグとコード本体を分離。`LazyLock` でコンパイル結果をキャッシュし、毎メッセージの再コンパイルを排除
- **Graceful Shutdown**: `tokio::signal` で SIGINT / SIGTERM を受け取り、処理中の `spawn_blocking` タスク（レンダリング）が完了してからBotプロセスを終了する
- **Discord API 429 リトライ**: serenity/poise はDiscord APIからの `429 Too Many Requests` レスポンスに対して、`Retry-After` ヘッダに基づく自動リトライを内蔵している。独自のリトライロジックは実装しない。ただし、Bot側のレート制限（`governor`）でリクエスト頻度を事前に抑制し、429を極力発生させない設計とする
- **レンダリング同時実行数制御**: `tokio::sync::Semaphore` で `spawn_blocking` の同時実行数を `Settings.max_concurrent_renders`（デフォルト: 4）に制限し、CPU飽和を防止する
- **fontdue によるフォントフォールバック**: `FontSet::rasterize_char()` は `primary.lookup_glyph_index(ch)` でグリフの存在を確認し、0（未定義）なら `fallback` にフォールバックする。usvg/resvg の fontdb によるフォント解決を経由しないため、フォント選択のオーバーヘッドが極小。`FontFamily` enum により Fira Code / PlemolJP / HackGen Console NF の切替が可能で、各 FontSet は独立したグリフキャッシュを保持する

---

## テスト戦略

### ユニットテスト（各ファイル内の `#[cfg(test)] mod tests`）

| 対象 | テストケース |
|------|------------|
| `extract_code_block` | 空文字、言語タグあり/なし、最大文字数境界、複数ブロック、ネストしたバッククォート |
| `sanitize_code` | 制御文字除去、タブ/改行保持、Unicode正規化（NFC）、ゼロ幅文字 |
| `escape_for_svg` | `&`, `<`, `>`, `"` のエスケープ |
| `highlight` | Rust/Python/Go 各言語のトークン化、未知言語のフォールバック |
| `svg_builder` | 行数に応じたSVG高さ計算、テーマ設定の反映、フォント指定（スナップショットテスト用） |
| `canvas` | FontSet初期化、寸法計算、Pixmap描画（タイトルバー各スタイル、行番号）、グリフαブレンド |
| `rasterize` (ShadowCache) | キャッシュヒット/ミスの一貫性、サイズ別独立性 |
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
- `svg_builder` で超過行をトリミングし、末尾に `…` を表示する
- 画像幅は `max_line_length` に基づいて固定し、Discordの表示領域を超過しない

### 14. 背景画像のパフォーマンス最適化

背景画像のレンダリングを高速化し、メモリ消費を抑制する。

- **BackgroundCache**: WebP 画像を起動時に1回だけデコードし、`image::RgbaImage` としてキャッシュ
- **Pixmap 直接合成**: PNG エンコード/デコードの往復を排除。`tiny_skia::Pixmap` ベースで背景タイリング → ぼかし → コード合成
- **SVG の軽量化**: 背景画像を SVG に Base64 埋め込みしない。コードのみの軽量 SVG を resvg でラスタライズし、背景は rasterize 側で合成
- **テクスチャ背景のタイリング**: `image::imageops::overlay` で小さなタイル画像を対象サイズまで敷き詰め

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
