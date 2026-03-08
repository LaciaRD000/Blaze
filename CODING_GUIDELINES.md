# コーディング規約

## 命名規約
- 型名・列挙型: `PascalCase`（例: `AppState`, `ApiError`, `RateLimitIpMode`）
- 関数・変数・フィールド: `snake_case`（例: `create_token`, `user_id`）
- 定数: `SCREAMING_SNAKE_CASE`（例: `EPOCH`, `SEQUENCE_BITS`）
- ファイル名: `snake_case`（例: `rate_limit.rs`, `snowflake.rs`）

## コメント
- 日本語で書く
- 自明なコードにはコメントを書かない。ロジックの意図が分かりにくい箇所にのみ記載する

## エラーメッセージ
- APIレスポンスのエラーメッセージは英語（例: `"username is empty"`）

## unwrap() の使用
- 原則 `expect("理由")` を使い、パニックの理由を明記する
- Mutex の `lock()` など、失敗しないことが自明な場合のみ `unwrap()` を許可する

## unsafe
- このプロジェクトでは使用禁止

## `pub` の公開範囲
- 外部モジュールから使うもののみ `pub` にする
- ファイル内のヘルパー関数や定数は非公開にする（例: `fn hash_refresh_token`, `const USERS_EMAIL_UNIQUE_CONSTRAINT`）
- 迷ったら非公開から始め、必要になった時点で `pub` に昇格する

## use 文の整理
- 外部クレート → `crate::` 内部モジュールの順に並べる
- グループ間に空行を入れる
- 同一クレートからの複数インポートはネストしてまとめる

## フォーマット
- `rustfmt` に従う（`rustfmt.toml` の unstable オプションのため nightly toolchain 前提）
- 関数の引数が多い場合は引数ごとに改行する

## 型アノテーション
- コンパイラが推論できる場合は省略する

## `derive` マクロの順序
- 以下の順で記述する: std トレイト → serde → sqlx → その他
- 例: `#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]`

## 文字列の扱い
- 構造体のフィールドや戻り値は `String`（所有型）を使う
- 関数の引数は `&str`（借用）を優先する
- 例: `fn validate_username(username: &str) -> Result<(), String>`

## 関数の引数の数
- 引数が多くなりすぎないように注意する
- 引数が多い場合は構造体にまとめることを検討する（例: `Config` 構造体）

## トレイト実装の配置
- 構造体と同じファイルに置く（例: `ApiError` の `IntoResponse` は `errors.rs` に）

## マジックナンバーの禁止
- 意味のある数値は定数に切り出すか、コメントで意図を明記する
- 設定値（タイムアウト秒数、ボディサイズ上限等）は `Config` や定数で管理する

## SQL
- `sqlx::query` / `query_as` に生 SQL を直接書く
- `SELECT *` は使わず、必要なカラムを明示的に指定する

## エラーハンドリング
- ハンドラーの戻り値は `Result<T, ApiError>` で統一する
- ライブラリエラーは `.map_err(|err| ApiError::Internal(err.to_string()))` で変換する
- バリデーションエラーは `.map_err(|err| ApiError::BadRequest(err))` で変換する

## 非同期/ブロッキング処理
- CPU バウンド処理（パスワードハッシュ化・照合等）は `tokio::task::spawn_blocking` で実行する
- Tokio のワーカースレッドをブロックしない。長時間かかる同期処理はブロッキング専用スレッドプールに逃がす

## 機密情報の取り扱い
- パスワード（平文・ハッシュ）、トークン（JWT・リフレッシュトークン）、シークレット（`JWT_SECRET`・`REFRESH_TOKEN_PEPPER`）をログやレスポンスに含めない
- `ApiError::Internal` はクライアントに詳細を返さず、固定メッセージ `"An internal error occurred"` を返す

## `clippy` の扱い
- `cargo clippy` の警告はゼロを維持する
- 抑制（`#[allow(...)]`）する場合はコメントで理由を記載する
- 例: `// GovernorError is defined in an external crate and cannot be boxed`

## ログレベルの使い分け
- `tracing::error!` — 500系の内部エラー
- `tracing::warn!` — 認証失敗・不正リクエスト等
- `tracing::info!` — サーバー起動・DB接続成功等
- `tracing::debug!` — 開発時のデバッグ情報

## コミットメッセージ
- Conventional Commits の prefix + 日本語の説明
- 例: `feat: ログインハンドラー実装`、`fix: バリデーションの修正`

## テスト
- 単体テストは同じファイル内の `#[cfg(test)] mod tests` に書く
- 結合テストは `tests/` ディレクトリに置く
- テスト関数名は `{対象}_{条件}_{期待結果}` の形式にする（例: `password_7_chars_is_rejected`, `username_empty_is_rejected`）
