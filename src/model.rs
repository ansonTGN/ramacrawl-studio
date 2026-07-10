use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use url::Url;

#[derive(Debug, Clone)]
pub struct CrawlTarget {
    pub url: Url,
    pub depth: usize,
    pub parent: Option<Url>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRecord {
    pub crawl_id: String,
    pub requested_url: String,
    pub final_url: String,
    pub parent_url: Option<String>,
    pub depth: usize,
    pub fetched_at: DateTime<Utc>,
    pub status: u16,
    pub http_version: String,
    pub content_type: Option<String>,
    pub body_bytes: usize,
    pub elapsed_ms: u128,
    pub redirect_chain: Vec<String>,
    pub request_profile: String,
    pub upstream_proxy: Option<String>,
    pub request_header_fingerprint: String,
    pub response_fingerprint: String,
    pub response_headers: BTreeMap<String, String>,
    pub body_sha256: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRecord {
    pub crawl_id: String,
    pub source_url: String,
    pub rule: String,
    pub ordinal: usize,
    pub extracted_at: DateTime<Utc>,
    pub data: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct FetchResult {
    pub requested_url: Url,
    pub final_url: Url,
    pub status: u16,
    pub http_version: String,
    pub content_type: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub redirect_chain: Vec<Url>,
    pub elapsed_ms: u128,
    pub proxy: Option<String>,
    pub request_header_fingerprint: String,
    pub response_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlProgress {
    pub crawl_id: String,
    pub updated_at: DateTime<Utc>,
    pub pages_scheduled: usize,
    pub pages_fetched: usize,
    pub pages_failed: usize,
    pub pages_blocked: usize,
    pub records_extracted: usize,
    pub bytes_downloaded: u64,
    pub queued: usize,
    pub active: usize,
    pub max_pages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlSummary {
    pub crawl_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub pages_scheduled: usize,
    pub pages_fetched: usize,
    pub pages_failed: usize,
    pub pages_blocked: usize,
    pub records_extracted: usize,
    pub bytes_downloaded: u64,
}
