use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteSettings {
    pub id: String,
    pub name: String,
    pub api_base_url: String,
    pub download_base_url: Option<String>,
    pub start_path: String,
    pub token: Option<String>,
    /// User account base path (e.g. "/abc") - prepended to file paths when constructing download URLs
    #[serde(default)]
    pub user_base_path: Option<String>,
    pub dir_password: Option<String>,
    pub proxy_url: Option<String>,
    pub user_agent: Option<String>,
    pub cookie: Option<String>,
    pub timeout_ms: Option<u64>,
    pub follow_redirects: Option<bool>,
}

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: "New Site".to_string(),
            api_base_url: "".to_string(),
            download_base_url: None,
            start_path: "/".to_string(),
            token: None,
            user_base_path: None,
            dir_password: None,
            proxy_url: None,
            user_agent: Some("olist-cdn-preheat/0.1 (Mozilla/5.0 compatible)".to_string()),
            cookie: None,
            timeout_ms: Some(60000),
            follow_redirects: Some(true),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSettings {
    pub min_conn: Option<u32>,
    pub max_conn: u32,
    pub rate_limit_bytes_per_sec: Option<u64>,
    pub chunk_bytes: Option<usize>,
    pub max_attempts: Option<u32>,
    pub backoff_base_ms: Option<u64>,
    pub backoff_max_ms: Option<u64>,
    pub partial_mode: Option<PartialModeConfig>,
    pub warn_on_redirect: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialModeConfig {
    pub enabled: bool,
    pub max_bytes: Option<u64>,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            min_conn: Some(5),
            max_conn: 32,
            rate_limit_bytes_per_sec: Some(10 * 1024 * 1024), // 10MB/s
            chunk_bytes: Some(65536),
            max_attempts: Some(3),
            backoff_base_ms: Some(500),
            backoff_max_ms: Some(10000),
            partial_mode: Some(PartialModeConfig { enabled: false, max_bytes: None }),
            warn_on_redirect: Some(true),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub id: String,
    /// Which site/group this file belongs to (SiteSettings.id). Empty for legacy single-site payloads.
    #[serde(default)]
    pub site_id: String,
    pub path: String, // Logical path
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<DateTime<Utc>>,
    #[serde(default)]
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub run_id: String,
    pub site_id: String,
    pub state: RunState,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total_files: u64,
    pub total_known: bool,
    pub queued: u64,
    pub active: u64,
    pub desired: u64,
    pub done: u64,
    pub failed: u64,
    pub warnings: u64,
    pub bytes_read_total: u64,
    pub speed_bps: u64,
    pub eta_sec: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Idle,
    Running,
    Paused,
    Canceling,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUpdate {
    pub run_id: String,
    /// Which site/group this update belongs to (SiteSettings.id). Empty for legacy single-site payloads.
    #[serde(default)]
    pub site_id: String,
    pub path: String,
    pub status: FileStatus,
    pub attempt: Option<u32>,
    pub url: Option<String>,
    pub final_url: Option<String>,
    pub content_length: Option<u64>,
    pub response_headers: Option<Vec<HeaderKV>>,
    pub bytes_read: u64,
    pub http_status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub warning: Option<WarningItem>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderKV {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Queued,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningItem {
    pub code: String,
    pub message: String,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub api: ApiTestResult,
    pub download: DownloadTestResult,
    pub sample_file: Option<SampleFile>,
    pub warnings: Vec<WarningItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiTestResult {
    pub ok: bool,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTestResult {
    pub ok: bool,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub bytes_read: Option<u64>,
    pub redirected: Option<bool>,
    pub final_url: Option<String>,
    pub final_host: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleFile {
    pub path: String,
    pub url: String,
}
