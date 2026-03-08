# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Blaze Bot is a Discord bot that converts code blocks into beautiful terminal-window-style PNG images (SwayFX/Wezterm aesthetic). All rendering is done natively in Rust — no external image APIs. See `DESIGN.md` for full architecture and data flow.

## Build & Development Commands

```bash
cargo build                    # Build
cargo run                      # Run (requires DISCORD_TOKEN in .env)
cargo test                     # Run all tests
cargo test test_name           # Run a single test
cargo clippy                   # Lint (warnings must be zero)
cargo fmt                      # Format (nightly toolchain required for rustfmt.toml unstable options)
```

## Architecture

**Rendering pipeline**: Discord message → extract code block (regex) → tokenize with `syntect` → build SVG string (`format!`/`write!`) → rasterize to PNG via `resvg`/`tiny-skia` → reply with image attachment.

Key crates: `poise` (Discord framework), `syntect` (syntax highlighting), `resvg`/`tiny-skia` (SVG→PNG), `sqlx` (Supabase PostgreSQL for user theme persistence).

The `Renderer` struct (holding `SyntaxSet`, `ThemeSet`, `fontdb::Database`) is wrapped in `Arc` and shared across all requests — read-only, no locks needed. CPU-bound rasterization runs in `tokio::task::spawn_blocking`.

## Coding Conventions (from CODING_GUIDELINES.md)

- **Language**: Comments in Japanese. Error messages in API responses in English.
- **Naming**: Types `PascalCase`, functions/variables `snake_case`, constants `SCREAMING_SNAKE_CASE`, files `snake_case`
- **No `unsafe`**. No `unwrap()` without justification — prefer `expect("reason")`.
- **Visibility**: Start private, promote to `pub` only when needed.
- **`use` ordering**: External crates → `crate::` internals, blank line between groups, nest imports from same crate.
- **`derive` order**: std traits → serde → sqlx → others (e.g., `#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]`)
- **Strings**: Owned `String` for struct fields/return values, `&str` for function parameters.
- **SQL**: Raw SQL via `sqlx::query`/`query_as`. No `SELECT *` — list columns explicitly.
- **Errors**: Return `Result<T, Error>`. Convert library errors with `.map_err()`.
- **Logging**: `error!` for 500s, `warn!` for auth failures, `info!` for startup, `debug!` for dev.
- **Clippy**: Zero warnings. If suppressed, comment the reason.
- **Tests**: Unit tests in `#[cfg(test)] mod tests` in the same file. Integration tests in `tests/`. Name format: `{subject}_{condition}_{expected}`.
- **Commits**: Conventional Commits prefix + Japanese description (e.g., `feat: ログインハンドラー実装`).

## Development Workflow

- **RGBC サイクルを厳守する**:
  1. **Red** — 失敗するテストを書く
  2. **Green** — テストを通す最小限のコードを書く
  3. **Blue** — リファクタリングする
  4. **Commit** — コミットする
- 一つの機能の実装が完了したら、コミットする
- ユーザーは Rust 初心者のため、実装内容を丁寧に日本語で解説する
- エディタ: nvim + rust-analyzer

## Conventions

- 日本語でコミュニケーションする
- 設計は `DESIGN.md`、仕様は `SPEC.md` を参照する
- 実装は `IMPLEMENTATION.md` の手順に従い、ステップ順に進める。スキップしない
- タスクと進捗は `TASKS.md` で管理する。タスク完了時にチェックを更新すること

## 禁止事項

- `.env*` ファイルをユーザーの許可なく読み取らないこと（`DISCORD_TOKEN` 等のシークレットを含むため）
