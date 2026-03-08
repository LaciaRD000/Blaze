# Blaze Bot

Discord 上のコードブロックを、SwayFX/Wezterm 風のターミナルウィンドウ画像（PNG）に変換する Bot。

外部 API に依存せず、Rust 内部でネイティブにレンダリングを行うため、高速かつセキュアに動作します。

![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange)
![License](https://img.shields.io/badge/license-Apache%202.0-blue)

## 特徴

- **ネイティブレンダリング** — syntect でハイライト → SVG 生成 → resvg/tiny-skia で PNG 変換
- **高解像度出力** — 2x スケールで Discord の高 DPI 表示にも対応
- **テーマカスタマイズ** — カラースキーム、背景、フォント、タイトルバーなどを `/theme set` で設定
- **日本語対応** — PlemolJP / HackGen NF フォントをバンドル

## 使い方

### コードを画像化する

1. コードブロックを含むメッセージを右クリック
2. **アプリ → 「ターミナル画像化」** を選択
3. ターミナルウィンドウ風の画像がリプライとして送信されます

### テーマを設定する

| コマンド | 説明 |
|---------|------|
| `/theme set` | カラースキーム・背景・フォント等を変更 |
| `/theme preview` | 現在のテーマでサンプルコードをプレビュー |
| `/theme reset` | テーマをデフォルトに戻す |

#### `/theme set` のパラメータ

| パラメータ | 選択肢 | デフォルト |
|-----------|--------|-----------|
| `color_scheme` | base16-ocean.dark, base16-eighties.dark, base16-mocha.dark, base16-ocean.light, InspiredGitHub, Solarized (dark/light) | base16-eighties.dark |
| `background` | none / gradient / denim / repeated-square-dark | gradient |
| `blur` | 0.0 〜 30.0 | 8.0 |
| `opacity` | 0.3 〜 1.0 | 0.75 |
| `title_bar` | macOS / linux / plain / none | macOS |
| `font` | Fira Code / PlemolJP / HackGen NF | Fira Code |
| `show_line_numbers` | true / false | false |

## セットアップ

### 前提条件

- Rust nightly toolchain
- PostgreSQL（Supabase 推奨）
- Discord Bot トークン（MESSAGE_CONTENT Intent が必要）
- Redis（マイクロサービスモード時のみ）

### 環境変数

`.env` ファイルをプロジェクトルートに作成:

```env
DISCORD_TOKEN=your_discord_bot_token
DATABASE_URL=postgresql://user:pass@host:port/db
REDIS_URL=redis://127.0.0.1/        # マイクロサービスモード時のみ
```

### 起動方法

2つのデプロイモードから選択できます。

#### モノリスモード（シンプル構成）

1プロセスで Discord I/O + レンダリングを実行します。Redis 不要。

```bash
cargo run
```

#### マイクロサービスモード（スケーラブル構成）

Discord I/O（Gateway）と CPU バウンドなレンダリング（Worker）を別プロセスに分離します。
Worker のクラッシュが Discord 接続に影響せず、Worker を複数起動して水平スケール可能です。

```bash
# 1. Redis を起動
redis-server --daemonize yes

# 2. 起動スクリプト（Gateway 1台 + Worker 4台）
./scripts/start-microservices.sh

# Worker 数を指定する場合
./scripts/start-microservices.sh 8
```

個別に起動する場合:

```bash
# ターミナル 1 — Worker（複数起動可）
cargo run --bin blaze-worker

# ターミナル 2 — Gateway
cargo run --bin blaze-gateway
```

`Ctrl+C` で全プロセスを一括停止します。

### 設定ファイル

`config/default.toml` でデフォルト値を変更できます:

```toml
max_code_lines = 100
max_code_chars = 4000
max_line_length = 120
rate_limit_per_minute = 10
max_concurrent_renders = 4
log_level = "info"
```

環境変数 `BLAZE_*` でオーバーライド可能（例: `BLAZE_MAX_CODE_LINES=150`）。

## 開発

```bash
cargo test                     # テスト実行
cargo clippy                   # リント（警告ゼロを維持）
cargo fmt                      # フォーマット（nightly 必須）
```

## アーキテクチャ

### モノリスモード

```
Discord メッセージ
  → コードブロック抽出（正規表現）
  → 入力バリデーション & サニタイズ
  → syntect でトークン化 & 色付け
  → SVG 文字列生成（タイトルバー、背景、コードテキスト）
  → resvg/tiny-skia で PNG ラスタライズ（2x スケール）
  → Discord にリプライ送信
```

### マイクロサービスモード

```
ユーザー → Discord → Gateway (バリデーション・テーマ取得)
                        ↓ LPUSH
                      Redis (blaze:jobs)
                        ↓ BRPOP
                     Worker (レンダリング)
                        ↓ LPUSH
                      Redis (blaze:results:{job_id})
                        ↓ BRPOP
                     Gateway → Discord → ユーザー
```

詳細は [DESIGN.md](DESIGN.md)・[SPEC.md](SPEC.md) を参照してください。

## 技術スタック

| カテゴリ | 技術 |
|---------|------|
| 言語 | Rust (Edition 2024) |
| Discord | poise / serenity |
| 構文解析 | syntect (ビルド時 packdump) |
| 画像生成 | resvg / tiny-skia / image |
| DB | PostgreSQL / Supabase (sqlx) |
| メッセージキュー | Redis |
| エラー処理 | thiserror |
| レート制限 | governor |
| ロギング | tracing |

## ライセンス

[Apache License 2.0](LICENSE)
