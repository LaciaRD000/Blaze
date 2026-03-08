# タスク管理

## 凡例

- [ ] 未着手
- [~] 進行中
- [x] 完了

---

## Phase 1: Bot起動 + コンテキストメニュー + コードブロック抽出

- [x] Cargo.toml に依存クレートを追加
- [x] config/default.toml 作成、Settings 構造体実装
- [x] BlazeError 独自エラー型実装
- [x] main.rs: poise Framework 起動 + Graceful Shutdown
- [x] extract_code_block() 実装 + テスト
- [x] sanitize_code() / escape_for_svg() 実装 + テスト
- [x] render_message コンテキストメニューコマンド（画像生成はスタブ、コードブロック抽出まで）

## Phase 2: シンタックスハイライト + SVG生成 + PNG変換

- [x] renderer/highlight.rs: syntect によるトークン化
- [x] renderer/svg_builder.rs: 最小限のSVG文字列生成（背景なし、装飾なし）
- [x] renderer/rasterize.rs: resvg/tiny-skia で SVG→PNG
- [x] renderer/mod.rs: Renderer 構造体 + パイプライン統括
- [x] render_message をスタブから実画像生成に接続
- [x] スナップショットテスト（insta）導入

## Phase 3: ビジュアル完成

- [x] ガウスぼかし背景（feGaussianBlur）
- [x] 半透明ウィンドウ矩形（fill-opacity）
- [x] タイトルバー（macOS風 / Linux風）
- [x] 角丸 + ドロップシャドウ
- [x] フォント埋め込み（Fira Code + PlemolJP）

## Phase 4: テーマ管理 + DB

- [x] ~~SQLite~~ Supabase (PostgreSQL) セットアップ + マイグレーション（001_create_user_themes）
- [x] db/models.rs: UserTheme 構造体 + CRUD
- [x] db/mod.rs: ThemeRepository トレイト + PgThemeRepository
- [x] /theme set コマンド
- [x] /theme preview コマンド
- [x] /theme reset コマンド
- [x] DB障害時のデフォルトテーマフォールバック

## Phase 5: 本番品質

- [x] 背景画像バリエーション + 事前リサイズ最適化
- [x] max_line_length による横方向トリミング
- [x] spawn_blocking + Semaphore による同時実行数制御
- [x] Settings::validate() による起動時バリデーション

## Phase 6: 堅牢性・セキュリティ

- [x] governor によるレート制限
- [x] 入力サニタイズ強化（制御文字除去、Unicode NFC正規化）
- [x] SVGインジェクション防止の検証
- [x] 設定管理（config/default.toml + BLAZE_* 環境変数オーバーライド）

## Phase 7: ロギング・監視

- [x] tracing / tracing-subscriber 導入
- [x] ログレベル運用（ERROR / WARN / INFO / DEBUG）
- [x] レンダリング処理時間の計測ログ

## Phase 8: UX向上

- [x] 行番号表示オプション（show_line_numbers）
- [ ] 複数コードブロック対応（将来）

## Phase 9: 高度なアーキテクチャ最適化

- [x] syntect バイナリダンプ化（build.rs + from_uncompressed_data）
- [x] Gateway / Worker 間プロトコル定義（protocol.rs）
- [x] Render Worker バイナリ（src/bin/worker.rs）
- [x] Gateway バイナリ（src/bin/gateway.rs）
- [x] ドキュメント更新（DESIGN.md, SPEC.md, IMPLEMENTATION.md）

## Phase 10: レンダリングパフォーマンス最適化

- [x] ぼかし処理の直接ピクセル操作化（SVG 経由の6段パイプラインを排除）
- [x] PNG エンコードの高速圧縮化（CompressionType::Fast）
- [x] ドキュメント更新（DESIGN.md, SPEC.md, IMPLEMENTATION.md）
