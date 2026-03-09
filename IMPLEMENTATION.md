# 実装手順書

DESIGN.md の設計に基づき、RGBCサイクル（Red→Green→Blue→Commit）を厳守して段階的に実装する。
各ステップは独立したコミット単位であり、`git revert` で個別に巻き戻し可能。

## ルール

1. **1ステップ = 1コミット** — ステップを跨いだ変更はしない
2. **RGBC サイクル** — 各ステップ内で Red→Green→Blue→Commit の順に進行する
   - **Red**: 失敗するテストを書く（テスト不要なステップは明記）
   - **Green**: テストを通す最小限のコードを書く
   - **Blue**: リファクタリング（clippy 警告ゼロ、`cargo fmt`）
   - **Commit**: Conventional Commits + 日本語で記録
3. **各ステップ完了時に `cargo test` + `cargo clippy` が通ること**
4. **巻き戻し**: 問題が起きたら `git revert <commit>` で該当ステップだけ取り消す

---

## Phase 1: Bot起動 + コンテキストメニュー + コードブロック抽出

### Step 1.1: プロジェクト基盤セットアップ

- Cargo.toml に全依存クレートを追加（DESIGN.md「依存クレート」節の通り）
- `cargo check` が通ることを確認
- テスト: なし（設定ファイルのみ）
- コミット: `chore: 依存クレートを追加`

### Step 1.2: BlazeError 独自エラー型

- `src/error.rs` を作成
- `BlazeError` enum を定義（全バリアント）
- `From<syntect::Error>` 実装
- `BlazeError::rendering()` コンビニエンスコンストラクタ
- Red: `BlazeError` の各バリアントの `Display` 出力テスト、`From` 変換テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: BlazeError 独自エラー型を実装`

### Step 1.3: Settings 構造体 + config/default.toml

- `src/config.rs` を作成
- `Settings` 構造体（Deserialize）を定義
- `Settings::validate()` を実装
- `config/default.toml` を作成
- Red: validate() のテスト — 正常値、各フィールドの境界値（0, 上限超過）
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: Settings 構造体とバリデーションを実装`

### Step 1.4: 入力サニタイズ

- `src/sanitize.rs` を作成
- `sanitize_code()`: 制御文字除去、タブ展開、Unicode NFC正規化
- `escape_for_svg()`: `&`, `<`, `>`, `"` のエスケープ
- Red: 制御文字除去、タブ展開、NFC正規化、SVGエスケープのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 入力サニタイズ関数を実装`

### Step 1.5: CodeBlock 構造体 + extract_code_block

- `src/commands/mod.rs` + `src/commands/render.rs` を作成
- `CodeBlock` 構造体、`sanitized()` メソッド
- `extract_code_block()` 関数（正規表現）
- Red: 空文字、言語タグあり/なし、複数ブロック（最初のみ）、ネストしたバッククォート
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: コードブロック抽出を実装`

### Step 1.6: main.rs — poise Framework 起動

- `src/main.rs` を実装
- Data 構造体（この時点では db / renderer / rate_limiter はスタブまたは省略可）
- poise::Framework の構築 + `render_message` コマンド登録
- on_error ハンドラ
- Graceful Shutdown（tokio::signal）
- dotenvy による .env 読み込み
- テスト: なし（Discord接続が必要なため手動確認）
- コミット: `feat: poise Framework 起動とコンテキストメニュー登録`

### Step 1.7: render_message コマンド（スタブ）

- `render_message` のロジック実装（画像生成はスタブ）
  - コードブロック抽出 → バリデーション → サニタイズ
  - 画像の代わりにコードブロックの内容をテキストで返す（仮実装）
- テスト: なし（Discord接続が必要なため手動確認）
- コミット: `feat: render_message コマンドのスタブ実装`

**Phase 1 完了確認**: Bot が Discord に接続し、コンテキストメニューが表示され、コードブロックの抽出結果がテキストで返ること。

---

## Phase 2: シンタックスハイライト + SVG生成 + PNG変換

### Step 2.1: highlight.rs — syntect トークン化

- `src/renderer/mod.rs` + `src/renderer/highlight.rs` を作成
- `HighlightedLine`, `StyledToken` 構造体
- `highlight()` 関数: コード文字列 → `Vec<HighlightedLine>`
- Red: Rust/Python コードのトークン化テスト、未知言語のフォールバック
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: syntect によるシンタックスハイライトを実装`

### Step 2.2: svg_builder.rs — 最小限SVG生成

- `src/renderer/svg_builder.rs` を作成
- 装飾なしの最小SVG: 白背景 + 色付きテキストのみ
- `build_svg()` 関数: `Vec<HighlightedLine>` + テーマ設定 → SVG文字列
- `xml:space="preserve"` 付与
- Red: 行数に応じた高さ計算テスト、`<tspan>` の色属性テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 最小限のSVG文字列生成を実装`

### Step 2.3: rasterize.rs — resvg/tiny-skia PNG変換

- `src/renderer/rasterize.rs` を作成
- `rasterize()` 関数: SVG文字列 → PNG バイト列（`Vec<u8>`）
- フォント埋め込み（Fira Code のみ。この時点では最小限）
- Red: 有効なSVG → PNG バイト列が空でないことのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: resvg/tiny-skia によるSVG→PNG変換を実装`

### Step 2.4: Renderer 統括 + パイプライン結合

- `src/renderer/mod.rs` に `Renderer` 構造体を実装
- `Renderer::new()`: SyntaxSet, ThemeSet, fontdb 初期化
- `Renderer::render()`: highlight → svg_builder → rasterize の統括
- Red: コード入力 → PNG バイト列出力の統合テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: レンダリングパイプラインを統括する Renderer を実装`

### Step 2.5: render_message を実画像生成に接続

- Step 1.7 のスタブを置き換え
- Data 構造体に `renderer: Arc<Renderer>` を追加
- `spawn_blocking` でレンダリングを実行
- テスト: なし（手動確認）
- コミット: `feat: render_message を実画像生成に接続`

### Step 2.6: スナップショットテスト導入

- `insta` によるSVGスナップショットテスト
- 特定のコード + デフォルトテーマの組み合わせでSVGをスナップショット保存
- Red: スナップショット作成（初回は必ず失敗）
- Green: `cargo insta review` で承認
- Blue: clippy + fmt
- コミット: `test: insta によるSVGスナップショットテストを導入`

**Phase 2 完了確認**: コンテキストメニューから実行すると、シンタックスハイライト付きのコード画像（装飾なし）がチャンネルに投稿されること。

---

## Phase 3: ビジュアル完成

### Step 3.1: フォント埋め込み（Fira Code + PlemolJP）

- `assets/fonts/` にフォントファイルを配置
- `load_fonts()` 関数: `include_bytes!` で両フォントをロード
- `Renderer::new()` から呼び出し
- Red: font_db にフォントが登録されていることのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: Fira Code + PlemolJP フォント埋め込み`

### Step 3.2: タイトルバー（macOS風）

- svg_builder に macOS 風タイトルバーを追加
- 赤・黄・緑の円ボタン + 言語名テキスト
- Red: SVGに `<circle>` と言語名が含まれることのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: macOS風タイトルバーを実装`

### Step 3.3: 角丸 + ドロップシャドウ

- SVGに `<clipPath>` (角丸) + `<filter id="shadow">` (ドロップシャドウ) を追加
- Red: SVGに角丸クリップとシャドウフィルタが含まれることのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 角丸とドロップシャドウを実装`

### Step 3.4: 半透明ウィンドウ + ガウスぼかし背景

- SVGに背景画像レイヤー（Base64埋め込み）+ `feGaussianBlur` フィルタを追加
- ウィンドウ矩形に `fill-opacity` を適用
- `assets/backgrounds/` にデフォルト背景画像を配置
- Red: SVGに `feGaussianBlur` と `fill-opacity` が含まれることのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 半透明ウィンドウとガウスぼかし背景を実装`

### Step 3.5: Linux風タイトルバー

- svg_builder に Linux WM 風タイトルバーの描画を追加
- テーマ設定（`title_bar_style`）で macOS / Linux を切り替え
- Red: Linux スタイル選択時のSVG出力テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: Linux風タイトルバーを実装`

### Step 3.6: スナップショット更新

- ビジュアル変更を反映してスナップショットを更新
- `cargo insta test` → `cargo insta review` で承認
- コミット: `test: ビジュアル変更に伴うスナップショット更新`

**Phase 3 完了確認**: SwayFX/Wezterm 風の美しいターミナル画像が生成されること。角丸、影、タイトルバー、ぼかし背景がすべて揃っていること。

---

## Phase 4: テーマ管理 + DB

### Step 4.1: SQLite セットアップ + マイグレーション

- `migrations/001_create_user_themes.up.sql` と `.down.sql` を作成
- main.rs で SQLite 接続 + WALモード + マイグレーション自動実行
- Data 構造体に `db: SqlitePool` を追加
- Red: インメモリDBでマイグレーション実行テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: SQLite セットアップとマイグレーションを実装`

### Step 4.2: UserTheme + ThemeRepository トレイト

- `src/db/mod.rs` + `src/db/models.rs` を作成
- `UserTheme` 構造体（`Default` 実装含む）
- `ThemeRepository` トレイト（get / upsert / delete）
- `SqliteThemeRepository` 実装
- Red: CRUD テスト（insert → get → update → delete、インメモリDB使用）
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: UserTheme と ThemeRepository を実装`

### Step 4.3: /theme set コマンド

- `src/commands/theme.rs` を作成
- `theme` 親コマンド + `set` サブコマンド
- パラメータのバリデーション（blur 範囲、opacity 範囲、既知のカラースキーム等）
- DB 更新
- テスト: なし（Discord コマンドのため手動確認）
- コミット: `feat: /theme set コマンドを実装`

### Step 4.4: /theme preview コマンド

- サンプルコードを現在のテーマで画像化して返信
- テスト: なし（手動確認）
- コミット: `feat: /theme preview コマンドを実装`

### Step 4.5: /theme reset コマンド

- ユーザーのテーマをDBから削除
- テスト: なし（手動確認）
- コミット: `feat: /theme reset コマンドを実装`

### Step 4.6: render_message にテーマ適用

- render_message 内でユーザーテーマを取得してレンダリングに反映
- DB障害時はデフォルトテーマにフォールバック
- テスト: なし（手動確認）
- コミット: `feat: render_message にユーザーテーマ適用を統合`

**Phase 4 完了確認**: `/theme set` でテーマを変更し、コード画像にテーマが反映されること。reset でデフォルトに戻ること。

---

## Phase 5: 本番品質

### Step 5.1: 背景画像の事前リサイズ最適化

- 背景画像をウィンドウサイズに合わせて事前リサイズしてからBase64埋め込み
- Red: リサイズ後の画像サイズが期待値であることのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 背景画像の事前リサイズ最適化`

### Step 5.2: max_line_length による横方向トリミング

- svg_builder で超過行をトリミング + `...` 表示
- 画像幅を `max_line_length` に基づいて固定
- Red: 120文字超の行がトリミングされることのテスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 横方向の長い行のトリミングを実装`

### Step 5.3: Semaphore による同時実行数制御

- Data 構造体に `render_semaphore: Arc<Semaphore>` を追加
- render_message で `semaphore.acquire()` → `spawn_blocking` → `drop(permit)`
- テスト: なし（手動確認）
- コミット: `feat: Semaphore によるレンダリング同時実行数制御`

**Phase 5 完了確認**: 大量リクエスト時にCPUが飽和せず、長い行が適切にトリミングされること。

---

## Phase 6: 堅牢性・セキュリティ

### Step 6.1: governor レート制限

- Data 構造体に `rate_limiter: Arc<DefaultKeyedRateLimiter<u64>>` を追加
- render_message の冒頭でレート制限チェック
- Red: 制限超過時のエラー通知テスト（ユニットテストでは governor のモックまたは直接呼び出し）
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: governor によるユーザーごとレート制限を実装`

### Step 6.2: 設定管理強化（TOML + 環境変数オーバーライド）

- config/default.toml からの読み込み + `BLAZE_*` 環境変数オーバーライド
- main.rs で `Settings::validate()` を起動時に呼び出し
- Red: 環境変数オーバーライドのテスト、不正値でのバリデーション失敗テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: TOML + 環境変数による設定管理を実装`

**Phase 6 完了確認**: レート制限が機能し、設定ファイルのオーバーライドが動作すること。

---

## Phase 7: ロギング・監視

### Step 7.1: tracing 基盤導入

- main.rs に `tracing_subscriber` を初期化
- `Settings.log_level` に基づくフィルタ設定
- テスト: なし
- コミット: `feat: tracing/tracing-subscriber によるロギング基盤を導入`

### Step 7.2: 各コマンドにログ出力を追加

- render_message: レンダリング成功/失敗、処理時間
- theme コマンド: テーマ変更/リセット
- on_error: 内部エラーの詳細
- テスト: なし
- コミット: `feat: コマンドハンドラにログ出力を追加`

**Phase 7 完了確認**: Bot 起動時・レンダリング時・エラー時に適切なログが出力されること。

---

## Phase 8: UX向上

### Step 8.1: 行番号表示オプション

- `/theme set` に `show_line_numbers` パラメータを追加
- svg_builder で行番号列を左側に描画（条件付き）
- Red: 行番号ON/OFF時のSVG出力テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: 行番号表示オプションを実装`

**Phase 8 完了確認**: 行番号表示の切り替えが正常に動作すること。

---

## Phase 9: 高度なアーキテクチャ最適化

### Step 9.1: syntect バイナリダンプ化

- `build.rs` を作成し、ビルド時に SyntaxSet / ThemeSet を uncompressed packdump に出力
- ランタイムの syntect フィーチャーから `default-syntaxes` / `default-themes` を除去
- `Renderer::new()` を `from_uncompressed_data()` によるダンプロードに変更
- `highlight.rs` テスト内のヘルパーもダンプロードに変更
- Red: 既存テスト（106 unit + 7 integration）がダンプロードで合格すること
- Green: build.rs + Renderer 変更
- Blue: clippy + fmt
- コミット: `perf: syntect バイナリダンプ化で起動を高速化`

### Step 9.2: Gateway / Worker 分離 — プロトコル定義

- `src/protocol.rs` を作成: RenderJob, RenderJobOptions, RenderResult
- Redis 定数（キュー名、結果キー接頭辞、TTL）を定義
- Cargo.toml に redis, serde_json, uuid を追加
- Settings に `redis_url: Option<String>` を追加
- Red: プロトコルのシリアライズ/デシリアライズ roundtrip テスト
- Green: 実装
- Blue: clippy + fmt
- コミット: `feat: Gateway/Worker 間プロトコルを定義`

### Step 9.3: Worker バイナリ

- `src/bin/worker.rs` を作成
- Redis BRPOP でジョブを待機 → Renderer でレンダリング → LPUSH で結果を返す
- セマフォで同時実行数を制御、spawn_blocking で CPU バウンド処理を分離
- テスト: なし（Redis 接続が必要なため手動確認）
- コミット: `feat: Render Worker バイナリを実装`

### Step 9.4: Gateway バイナリ

- `src/bin/gateway.rs` を作成
- Discord I/O + 入力バリデーション + Redis キュー投入 + 結果待機
- レンダリングは Worker に委譲（Gateway 自身では spawn_blocking しない）
- テスト: なし（Discord + Redis 接続が必要なため手動確認）
- コミット: `feat: Gateway バイナリを実装`

### Step 9.5: ドキュメント更新

- DESIGN.md にマイクロサービスアーキテクチャを追加
- SPEC.md にデプロイモードと REDIS_URL を追加
- コミット: `docs: マイクロサービスアーキテクチャを文書化`

**Phase 9 完了確認**: `cargo build --bin blaze-gateway` と `cargo build --bin blaze-worker` が成功し、全テストが合格すること。

---

## Phase 10: レンダリングパフォーマンス最適化

### Step 10.1: ぼかし処理の直接ピクセル操作化

- `blur_pixmap` を SVG 経由（Pixmap→PNG→Base64→SVG→resvg の6段パイプライン）から `image::imageops::blur` による直接ピクセル操作に変更
- `background::rgba_to_pixmap` を `pub(crate)` に昇格（rasterize.rs から参照）
- `pixmap_to_rgba` ヘルパーを追加（premultiplied → straight alpha 変換）
- `base64` クレートが不要になったため Cargo.toml から削除
- Red: `blur_pixmap_modifies_image`, `blur_pixmap_zero_radius_returns_unchanged`, `blur_pixmap_preserves_dimensions`
- Green: 実装
- Blue: clippy + fmt + 不要依存削除
- コミット: `perf: ぼかし処理を SVG 経由から直接ピクセル操作に変更`

### Step 10.2: PNG エンコードの高速圧縮化

- `tiny_skia::Pixmap::encode_png()` を `image::codecs::png::PngEncoder` に置換
- `CompressionType::Fast` + `FilterType::Sub` で高速エンコード
- Discord が画像アップロード時に再圧縮するため Bot 側の高圧縮は不要
- Red: `encode_png_fast_produces_valid_png`
- Green: 実装
- Blue: clippy + fmt
- コミット: `perf: PNG エンコードを高速圧縮に変更`

### Step 10.3: ドキュメント更新

- DESIGN.md / SPEC.md にパフォーマンス最適化の内容を反映
- コミット: `docs: Phase 10 パフォーマンス最適化をドキュメントに反映`

**Phase 10 完了確認**: 全テストが合格し、clippy 警告ゼロであること。

---

## Phase 11: レンダリングパイプライン高速化

### Step 11.1: feDropShadow の SVG 除去 + tiny_skia 直接描画

- **Red**: SVG にフィルタがないことを検証するテスト、シャドウ Pixmap 生成テストを追加
- **Green**: svg_builder から `<filter>` と `filter="url(#shadow)"` を除去。rasterize.rs に `create_shadow_pixmap()` を追加し、`rasterize()` と `rasterize_with_background()` でシャドウを合成
- **Blue**: スナップショット更新、リファクタリング
- コミット: `perf: ドロップシャドウを SVG フィルタから tiny_skia 直接描画に変更`

### Step 11.2: 背景ぼかしのダウンスケール最適化

- **Red**: 既存の `blur_pixmap_preserves_dimensions` テストが Green の役割
- **Green**: `blur_pixmap()` でダウンスケール → ぼかし → アップスケールに変更（計算量1/4）
- **Blue**: リファクタリング
- コミット: `perf: 背景ぼかしにダウンスケール最適化を適用`

### Step 11.3: ドキュメント更新 + ベンチマーク

- DESIGN.md / SPEC.md / IMPLEMENTATION.md / TASKS.md にパイプライン高速化の内容を反映
- ベンチマーク結果: パイプライン全体で約60%高速化（50行背景あり: 823ms → 315ms）
- コミット: `docs: Phase 11 パイプライン高速化をドキュメントに反映`

**Phase 11 完了確認**: 全テストが合格し、clippy 警告ゼロであること。
