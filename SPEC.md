# Blaze Bot - 仕様書

## 1. 概要

Blaze Bot は、Discord上に投稿されたコードブロックを、SwayFX/Wezterm風の美しいターミナルウィンドウ画像（PNG）に変換するDiscord Botである。

外部の画像生成APIに依存せず、Bot内部でネイティブにレンダリングを行うため、高速かつセキュアに動作する。

---

## 2. 対象ユーザー

- プログラミング系Discordコミュニティの参加者
- コードを視覚的に美しく共有したいユーザー
- 対象言語: 日本語（将来的に多言語対応の余地あり）

---

## 3. 機能仕様

### 3.1 コードブロックの画像化（コア機能）

#### 3.1.1 トリガー

メッセージを**右クリック → アプリ → 「ターミナル画像化」**を選択する（コンテキストメニューコマンド）。

コマンド入力は不要。任意のユーザーが送信済みのメッセージに対して実行できる。

#### 3.1.2 入力

対象メッセージに含まれる Markdown コードブロック（``` で囲まれた部分）。

```
\`\`\`rust
fn main() {
    println!("Hello, world!");
}
\`\`\`
```

- 言語タグ（上例の `rust`）は任意。省略時はプレーンテキストとして扱う
- 1メッセージに複数のコードブロックがある場合、最初のブロックのみ処理する

#### 3.1.3 出力

ターミナルウィンドウ風にデザインされたコード画像（PNG形式）を、対象メッセージへのリプライとして送信する。チャンネルの全員が閲覧できる。

#### 3.1.4 画像のデザイン要素

| 要素 | 説明 |
|------|------|
| 背景 | テクスチャ/グラデーション背景にガウスぼかし（Gaussian Blur）を適用。Pixmap 直接合成で高速処理 |
| ウィンドウ枠 | 角丸（border-radius: 12px）、ドロップシャドウ付き |
| タイトルバー | macOS風（赤・黄・緑の3つのボタン）またはLinux風。言語名を表示 |
| コード本体 | シンタックスハイライト済みのテキスト。プログラミング用フォントで描画 |
| フォント | Fira Code（英字）、PlemolJP（日本語フォールバック） |

#### 3.1.5 シンタックスハイライト

Sublime Text 互換の構文定義（syntect + two-face）を使用し、80以上の言語を自動認識する。主要な対応言語:

Rust, Python, Go, JavaScript, TypeScript (ts/tsx), JSX, C, C++, C#, Java, Kotlin, Swift, Dart, Ruby, PHP, Elixir, Shell, SQL, HTML, CSS, JSON, YAML, TOML, Markdown, Zig, Dockerfile, Terraform (HCL), Vue, Svelte, Nix, Scala, Lua, Haskell, その他 two-face がバンドルする全言語

言語タグが省略された場合、または認識できない場合は、ハイライトなし（プレーンテキスト）で描画する。

---

### 3.2 テーマ管理（パーソナライズ機能）

ユーザーごとに「お気に入りのターミナルテーマ」をデータベースに保存し、画像化のたびに自動適用する。

#### 3.2.1 `/theme set` — テーマ設定

| パラメータ | 型 | 説明 | デフォルト値 |
|-----------|-----|------|------------|
| `color_scheme` | ドロップダウン選択（任意） | カラースキーム名 | `base16-eighties.dark` |
| `background` | ドロップダウン選択（任意） | 背景画像（none / gradient / denim / repeated-square-dark） | `gradient` |
| `blur` | 小数（任意） | ガウスぼかしの強度 | `8.0` |
| `opacity` | 小数（任意） | ウィンドウの不透明度 | `0.75` |
| `title_bar` | ドロップダウン選択（任意） | タイトルバースタイル（macOS / Linux） | `macos` |
| `font` | ドロップダウン選択（任意） | フォント名（Fira Code / PlemolJP） | `Fira Code` |
| `show_line_numbers` | 真偽値（任意） | 行番号を表示するか | `false` |

- すべてのパラメータは任意。指定したものだけが更新される
- 未設定のユーザーにはデフォルト値が適用される
- 文字列パラメータは `poise::ChoiceParameter` による Discord ドロップダウンで選択する（自由入力不可）

#### 3.2.2 `/theme preview` — テーマプレビュー

現在保存されているテーマ設定で、サンプルコードを画像化して表示する。設定変更前の確認に使用する。

#### 3.2.3 `/theme reset` — テーマリセット

保存済みのテーマ設定を削除し、すべてデフォルト値に戻す。

---

### 3.3 テーマ設定のパラメータ詳細

#### カラースキーム（`color_scheme`）

syntect が内蔵するテーマから選択する。

| テーマ名 | 説明 |
|---------|------|
| `base16-ocean.dark` | 落ち着いたダークテーマ |
| `base16-eighties.dark` | 80年代風ダークテーマ（デフォルト） |
| `base16-mocha.dark` | Mocha ダークテーマ |
| `base16-ocean.light` | Ocean ライトテーマ |
| `InspiredGitHub` | GitHub風ライトテーマ |
| `Solarized (dark)` | Solarized ダーク |
| `Solarized (light)` | Solarized ライト |

#### 背景画像（`background`）

| 識別子 | 説明 |
|-------|------|
| `none` | 背景画像なし（ウィンドウのみ） |
| `gradient` | 暗い紫〜青のグラデーション背景 + ガウスぼかし |
| `denim` | デニム風テクスチャ背景（WebP埋め込み） |
| `repeated-square-dark` | ダークスクエアパターン背景（WebP埋め込み） |

#### ぼかし強度（`blur`）

- 範囲: `0.0` 〜 `30.0`
- `0.0` でぼかしなし、値が大きいほど背景が強くぼける

#### 不透明度（`opacity`）

- 範囲: `0.3` 〜 `1.0`
- `1.0` で完全に不透明、`0.3` で背景が透けて見える

#### フォント（`font`）

Bot にバンドルされたフォントから選択する。

| フォント名 | 説明 |
|-----------|------|
| `Fira Code` | リガチャ対応のプログラミングフォント（デフォルト） |
| `PlemolJP` | 日本語対応プログラミングフォント |
| `HackGen NF` | Nerd Fonts 対応の日本語プログラミングフォント |

#### タイトルバースタイル（`title_bar`）

| スタイル | 説明 |
|---------|------|
| `macos` | 赤・黄・緑の円ボタン（デフォルト） |
| `linux` | Linux WM 風（GNOME/Adwaita）のタイトルバー |
| `plain` | ボタンなし（言語名のみ表示） |
| `none` | タイトルバーなし（コードのみ表示） |

---

## 4. 入力制限

不特定多数のユーザーが利用するパブリックBotとして、以下の入力制限を設ける。

| 制限項目 | デフォルト値 | 設定キー |
|---------|------------|---------|
| 最大行数 | 100行 | `max_code_lines` |
| 最大文字数 | 4,000文字 | `max_code_chars` |
| 最大行長 | 120文字（超過分は `...` で省略） | `max_line_length` |

制限超過時は、実行者のみに見えるエフェメラルメッセージで通知し、画像生成は行わない。

---

## 5. レート制限

| 制限項目 | デフォルト値 | 設定キー |
|---------|------------|---------|
| ユーザーあたりのリクエスト数 | 10回/分 | `rate_limit_per_minute` |

制限超過時は「レート制限に達しました。しばらくお待ちください。」とエフェメラルメッセージで通知する。

---

## 6. エラー時の挙動

すべてのエラーメッセージは、実行者のみに見えるエフェメラルメッセージとして返される。

| 状況 | ユーザーへの表示 |
|------|----------------|
| コードブロックが見つからない | 「メッセージ内に ``` で囲まれたコードブロックが見つかりませんでした」 |
| コードが長すぎる | 「コードが長すぎます（上限: 100行 / 4000文字）」 |
| レート制限超過 | 「レート制限に達しました。しばらくお待ちください。」 |
| 無効なテーマ設定 | 具体的な設定エラー内容を表示 |
| 内部エラー（DB障害、レンダリング失敗等） | 「内部エラーが発生しました。しばらくしてからお試しください。」 |

内部エラーの詳細はサーバーログにのみ記録し、ユーザーには開示しない。

---

## 7. データ永続化

### 7.1 保存データ

ユーザーごとのテーマ設定のみをPostgreSQLデータベース（Supabase）に保存する。

| カラム | 型 | 説明 |
|-------|-----|------|
| `user_id` | BIGINT (PK) | Discord ユーザー ID |
| `color_scheme` | TEXT | カラースキーム名 |
| `background_id` | TEXT | 背景画像識別子 |
| `blur_radius` | DOUBLE PRECISION | ぼかし強度 |
| `opacity` | DOUBLE PRECISION | 不透明度 |
| `font_family` | TEXT | フォント名 |
| `font_size` | DOUBLE PRECISION | フォントサイズ (pt) |
| `title_bar_style` | TEXT | タイトルバースタイル |
| `show_line_numbers` | BOOLEAN | 行番号表示 |
| `updated_at` | TIMESTAMPTZ | 最終更新日時 |

### 7.2 データ喪失時の挙動

テーマデータが失われた場合（DB障害等）、デフォルトテーマにフォールバックしてサービスを継続する。ユーザーへのエラー通知は行わない。

---

## 8. Discord コマンド一覧

| 種類 | コマンド | 説明 |
|------|---------|------|
| コンテキストメニュー | 「ターミナル画像化」 | メッセージ内のコードブロックを画像化 |
| スラッシュコマンド | `/theme set` | テーマ設定を変更 |
| スラッシュコマンド | `/theme preview` | 現在のテーマでサンプルをプレビュー |
| スラッシュコマンド | `/theme reset` | テーマ設定をデフォルトにリセット |

---

## 9. 必要な Discord Bot 権限

| 権限 | 用途 |
|------|------|
| `Send Messages` | 画像付きメッセージの送信 |
| `Attach Files` | PNG画像の添付 |
| `Read Message History` | コンテキストメニューで対象メッセージの内容を読み取り |
| `Use Slash Commands` | スラッシュコマンドの登録・実行 |

**Gateway Intents**:
- `non_privileged` + `MESSAGE_CONTENT`（特権Intent。コンテキストメニューで対象メッセージのコードブロックを読み取るために必要）

---

## 10. 環境変数

| 変数名 | 必須 | 説明 |
|--------|------|------|
| `DISCORD_TOKEN` | 必須 | Discord Bot トークン |
| `DATABASE_URL` | 必須 | PostgreSQL 接続文字列（例: `postgresql://user:pass@host:port/db`） |
| `REDIS_URL` | 任意 | Redis 接続文字列（例: `redis://127.0.0.1:6379`）。マイクロサービスモード時に必要 |

シークレット情報は `.env` ファイルで管理し、設定ファイル（`config/default.toml`）には含めない。

---

## 11. 設定ファイル（config/default.toml）

```toml
max_code_lines = 100
max_code_chars = 4000
max_line_length = 120
rate_limit_per_minute = 10
max_concurrent_renders = 4
log_level = "info"
# redis_url = "redis://127.0.0.1:6379"  # 任意。マイクロサービスモード時に設定
```

環境変数 `BLAZE_*` で個別にオーバーライド可能（例: `BLAZE_MAX_CODE_LINES=150`）。`redis_url` は環境変数 `REDIS_URL` でも設定可能。

---

## 12. 非機能要件

### 12.1 パフォーマンス

- **syntect packdump**: `build.rs` がビルド時に `two-face` 経由で 80+ 言語の SyntaxSet と ThemeSet を非圧縮 packdump としてダンプし、ランタイムでは `from_uncompressed_data()` で即座にロードする。起動時の解凍処理を省略し、コールドスタートを高速化する
- レンダリング処理はCPUバウンドのため、`tokio::task::spawn_blocking` で非同期ランタイムをブロックしない
- レンダリングの同時実行数を `max_concurrent_renders`（デフォルト: 4）で制限し、CPU飽和を防止する
- **マイクロサービスによる水平スケーリング**: Gateway/Worker 分離構成では、複数の Worker プロセスを起動してレンダリング処理を分散できる。Redis リストベースのキューにより、ジョブは自動的に空いている Worker に分配される
- WebP 背景画像は起動時に1回だけデコードし `BackgroundCache` にキャッシュ。リクエストごとの再デコードを排除
- 背景画像は Pixmap として直接合成。メモリ消費の削減を実現
- テクスチャ背景（denim, repeated-square-dark）は `image::imageops::overlay` でタイリング
- 2x スケールを適用し、Discord の高DPI表示でもシャープに表示される高解像度画像を生成する
- **SVGパイプライン完全排除（Phase 12）**: メインのレンダリングパスから usvg/resvg を完全に排除。fontdue でグリフを個別にラスタライズし、tiny_skia の PathBuilder で描画プリミティブを構築して直接 Pixmap に描画する。usvg の SVG パース（~50ms）を完全に排除
- **canvas.rs 直接描画**: `FontSet`（fontdue::Font + RwLock グリフキャッシュ）がフォントフォールバック付きで `rasterize_cached()` でキャッシュ済みグリフを返す。`render_code_onto_pixmap()` が角丸矩形、タイトルバー、コード行を既存の Pixmap に直接描画（Pixmap 二重確保を排除）。`draw_glyph` は事前クリッピング計算で per-pixel bounds check を排除。トークン列は `Cow` で借用し、`max_line_length == None` 時の clone を回避
- **rasterize_direct / rasterize_direct_with_background**: SVG を経由しないラスタライズ関数。`rasterize_direct` は `render_code_onto_pixmap` で final_pixmap に直接描画し、中間 Pixmap 確保を排除
- 背景ぼかしは `image::imageops::blur` による直接ピクセル操作で処理。ダウンスケール最適化（1/2 に縮小 → ぼかし → 復元）で計算量を約1/4に削減
- ドロップシャドウは tiny_skia で矩形を直接描画し、1/4ダウンスケール+ぼかしを行う。`create_shadow_pixmap` はダウンスケールされた Pixmap を返し、合成時に `draw_pixmap` が `SHADOW_DRAW_SCALE`（8.0x）でアップスケールする
- **ShadowCache によるシャドウ Pixmap キャッシュ（Phase 14→15）**: `RwLock<HashMap<_, Arc<Pixmap>>>` でシャドウ Pixmap をキャッシュ。RwLock で読み取りは共有ロック、Arc で Pixmap clone を pointer clone に置換。シャドウはサイズのみに依存し、パターン数は高々 ~50
- 背景パスでは ShadowCache からシャドウを即座に取得し、背景ぼかし+コード描画を `std::thread::scope` で2スレッド並列実行（キャッシュ導入前の3スレッドから削減）
- PNG エンコードは `png` crate を直接利用し `Compression::Fast` + `FilterType::Sub` で高速に出力（Discord 側の再圧縮を考慮）
- 累積最適化効果: 50行コード（背景あり）で 823ms → 88ms（89%削減）。背景なし: 73ms → 39ms（Phase 15, 46%削減, 累積95%削減）

### 12.2 セキュリティ

- ユーザー入力のコードに対して制御文字除去・Unicode NFC正規化・SVGエスケープを適用し、SVGインジェクションを防止する
- Discord Bot トークンは環境変数で管理し、設定ファイルやログに出力しない
- レート制限により、単一ユーザーからのリソース枯渇攻撃を防止する

### 12.3 可用性

- DB障害時はデフォルトテーマにフォールバックし、画像化機能を継続する
- Graceful Shutdown: SIGINT / SIGTERM 受信時、処理中のレンダリングタスクが完了してから終了する
- Discord API の 429 レスポンスに対しては、poise/serenity 内蔵の自動リトライに委ねる

### 12.4 保守性

- PostgreSQL は読み取り/書き込みの並行性をネイティブに確保する（MVCC）
- マイグレーションは `sqlx` の `.up.sql` / `.down.sql` ペアでロールバック可能
- 日次バックアップを `pg_dump` または Supabase ダッシュボードのバックアップ機能で実施

---

## 13. デプロイモード

### 13.1 モノリスモード（デフォルト）

従来の単一プロセス構成。`src/main.rs` がエントリポイントとなり、Discord I/O とレンダリング処理を同一プロセス内で実行する。

- `REDIS_URL` が未設定の場合、自動的にこのモードで動作する
- 小〜中規模の利用に適する
- 起動コマンド: `cargo run` または `./blaze-bot`

### 13.2 マイクロサービスモード

Gateway（Discord I/O）と Worker（レンダリング）を別プロセスに分離する構成。Redis リストベースのキューで通信する。

- `REDIS_URL` の設定が必須
- Gateway: Discord コマンド処理、レート制限、入力バリデーション、DB クエリを担当
- Worker: CPUバウンドなレンダリング処理を担当。1プロセス1ジョブの同期処理で、並行処理は複数プロセス起動で実現
- 複数の Worker を起動して水平スケーリングが可能
- 起動コマンド:
  ```bash
  # Gateway（1プロセス）
  cargo run --bin blaze-gateway

  # Worker（必要に応じて複数起動）
  cargo run --bin blaze-worker
  ```

### 13.3 Redis キュー仕様

| 項目 | 値 |
|------|-----|
| ジョブキュー | Redis リスト `blaze:jobs` |
| 結果キュー | Redis リスト `blaze:results:{job_id}`（ジョブごとに個別） |
| 結果 TTL | 60秒（結果取得後またはタイムアウトで自動削除） |
| プロトコル | JSON（serde_json でシリアライズ/デシリアライズ） |
| ジョブ ID | UUID v4 |

---

## 14. 技術スタック

| カテゴリ | 技術 |
|---------|------|
| 言語 | Rust (Edition 2024, nightly toolchain) |
| Discord API | poise (serenity を re-export) |
| 構文解析 | syntect + two-face（ランタイムは packdump ロード、ビルド時に two-face 経由で 80+ 言語の dump 生成） |
| 画像生成 | fontdue（グリフラスタライズ）/ tiny-skia（2D描画・合成）、image（WebP デコード・タイリング）、resvg（SVGスナップショットテスト用） |
| データベース | PostgreSQL / Supabase (sqlx) |
| メッセージキュー | Redis (redis クレート、tokio-comp)（マイクロサービスモード時） |
| シリアライズ | serde / serde_json（プロトコル JSON） |
| ID 生成 | uuid v4（ジョブ ID） |
| エラーハンドリング | thiserror |
| レート制限 | governor |
| ロギング | tracing / tracing-subscriber |

---

## 15. 用語集

| 用語 | 定義 |
|------|------|
| コードブロック | Discord Markdown の ``` で囲まれたコード領域 |
| エフェメラルメッセージ | コマンド実行者のみに表示される一時的なメッセージ |
| コンテキストメニュー | メッセージを右クリック（またはロングタップ）して表示されるメニュー |
| シンタックスハイライト | プログラミング言語の構文に基づいてコードを色分けすること |
| カラースキーム | 構文ハイライトの配色セット |
| ガウスぼかし | 画像にかけるぼかしエフェクト。値が大きいほど強くぼける |
| タイトルバー | ターミナルウィンドウ上部の装飾部分（閉じる/最小化/最大化ボタン等） |
| レート制限 | 単位時間あたりのリクエスト回数を制限する仕組み |
| MVCC | PostgreSQL の Multi-Version Concurrency Control。読み書きの並行性をネイティブに確保する仕組み |
