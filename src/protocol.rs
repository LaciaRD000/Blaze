//! Gateway ↔ Worker 間のジョブプロトコル定義
//!
//! Gateway は RenderJob を Redis キューに投入し、
//! Worker がキューから取り出してレンダリング後、結果を返す。

use serde::{Deserialize, Serialize};

/// Redis キュー名（ジョブ投入先）
pub const JOBS_QUEUE: &str = "blaze:jobs";

/// ジョブ結果のキー接頭辞。結果は `blaze:results:{job_id}` に格納される
pub const RESULTS_PREFIX: &str = "blaze:results:";

/// 結果の TTL（秒）。Gateway が取得し損ねた場合の自動クリーンアップ
pub const RESULT_TTL_SECS: u64 = 60;

/// Gateway → Worker: レンダリングジョブ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJob {
    /// ジョブ固有ID（結果の受け取りに使用）
    pub job_id: String,
    /// レンダリング対象のコード
    pub code: String,
    /// 言語タグ（None でプレーンテキスト）
    pub language: Option<String>,
    /// syntect テーマ名
    pub theme_name: String,
    /// レンダリングオプション
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
    pub scale: f32,
}

/// Worker → Gateway: レンダリング結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderResult {
    /// レンダリング成功。PNG バイト列を含む
    Success { png_bytes: Vec<u8> },
    /// レンダリング失敗。エラーメッセージを含む
    Error { message: String },
}

impl RenderJob {
    /// 新しいジョブを UUID v4 で生成する
    pub fn new(
        code: String,
        language: Option<String>,
        theme_name: String,
        options: RenderJobOptions,
    ) -> Self {
        Self {
            job_id: uuid::Uuid::new_v4().to_string(),
            code,
            language,
            theme_name,
            options,
        }
    }

    /// 結果格納用の Redis キーを返す
    pub fn result_key(&self) -> String {
        format!("{RESULTS_PREFIX}{}", self.job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_job_new_generates_unique_ids() {
        let job1 = RenderJob::new(
            "test".into(),
            None,
            "base16-ocean.dark".into(),
            RenderJobOptions {
                title_bar_style: "macos".into(),
                opacity: 0.75,
                blur_radius: 8.0,
                show_line_numbers: false,
                max_line_length: Some(120),
                background_image: Some("gradient".into()),
                scale: 2.0,
            },
        );
        let job2 = RenderJob::new(
            "test".into(),
            None,
            "base16-ocean.dark".into(),
            RenderJobOptions {
                title_bar_style: "macos".into(),
                opacity: 0.75,
                blur_radius: 8.0,
                show_line_numbers: false,
                max_line_length: Some(120),
                background_image: Some("gradient".into()),
                scale: 2.0,
            },
        );
        assert_ne!(job1.job_id, job2.job_id);
    }

    #[test]
    fn render_job_result_key_format() {
        let job = RenderJob {
            job_id: "abc-123".into(),
            code: "test".into(),
            language: None,
            theme_name: "theme".into(),
            options: RenderJobOptions {
                title_bar_style: "macos".into(),
                opacity: 0.75,
                blur_radius: 8.0,
                show_line_numbers: false,
                max_line_length: None,
                background_image: None,
                scale: 2.0,
            },
        };
        assert_eq!(job.result_key(), "blaze:results:abc-123");
    }

    #[test]
    fn render_job_serialization_roundtrip() {
        let job = RenderJob::new(
            "fn main() {}".into(),
            Some("rust".into()),
            "base16-ocean.dark".into(),
            RenderJobOptions {
                title_bar_style: "linux".into(),
                opacity: 0.5,
                blur_radius: 4.0,
                show_line_numbers: true,
                max_line_length: Some(80),
                background_image: Some("denim".into()),
                scale: 2.0,
            },
        );
        let json = serde_json::to_string(&job).expect("シリアライズ成功");
        let decoded: RenderJob =
            serde_json::from_str(&json).expect("デシリアライズ成功");
        assert_eq!(decoded.job_id, job.job_id);
        assert_eq!(decoded.code, job.code);
        assert_eq!(decoded.language, job.language);
        assert_eq!(decoded.options.opacity, job.options.opacity);
    }

    #[test]
    fn render_result_success_serialization() {
        let result = RenderResult::Success {
            png_bytes: vec![0x89, 0x50, 0x4E, 0x47],
        };
        let json = serde_json::to_string(&result).expect("シリアライズ成功");
        let decoded: RenderResult =
            serde_json::from_str(&json).expect("デシリアライズ成功");
        match decoded {
            RenderResult::Success { png_bytes } => {
                assert_eq!(png_bytes, vec![0x89, 0x50, 0x4E, 0x47]);
            }
            RenderResult::Error { .. } => panic!("Success であるべき"),
        }
    }

    #[test]
    fn render_result_error_serialization() {
        let result = RenderResult::Error {
            message: "テストエラー".into(),
        };
        let json = serde_json::to_string(&result).expect("シリアライズ成功");
        let decoded: RenderResult =
            serde_json::from_str(&json).expect("デシリアライズ成功");
        match decoded {
            RenderResult::Error { message } => {
                assert_eq!(message, "テストエラー");
            }
            RenderResult::Success { .. } => panic!("Error であるべき"),
        }
    }
}
