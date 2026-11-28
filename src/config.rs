use anyhow::{Context, Result, bail};
use rama::http::{HeaderName, HeaderValue};
use scraper::Selector;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use tokio::fs;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub crawler: CrawlerConfig,
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub websocket: Option<WebSocketConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    #[serde(default)]
    pub incremental: bool,
    pub seeds: Vec<Url>,
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub denied_path_prefixes: Vec<String>,
    #[serde(default = "default_allowed_ports")]
    pub allowed_ports: Vec<u16>,
    #[serde(default = "default_true")]
    pub allow_subdomains: bool,
    #[serde(default)]
    pub allow_private_networks: bool,
    #[serde(default = "default_true")]
    pub respect_robots_txt: bool,
    #[serde(default = "default_true")]
    pub robots_fail_closed: bool,
    #[serde(default = "default_user_agent_token")]
    pub robots_user_agent: String,
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_per_host_concurrency")]
    pub per_host_concurrency: usize,
    #[serde(default = "default_delay_ms")]
    pub min_delay_ms: u64,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retries")]
    pub retries: usize,
    #[serde(default = "default_redirect_limit")]
    pub redirect_limit: usize,
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    #[serde(default = "default_profile")]
    pub user_agent_profile: UserAgentProfile,
    #[serde(default)]
    pub custom_headers: BTreeMap<String, String>,
    #[serde(default)]
    pub proxies: Vec<String>,
    #[serde(default = "default_true")]
    pub discover_links: bool,
    #[serde(default = "default_link_selector")]
    pub link_selector: String,
    #[serde(default)]
    pub include_url_regex: Option<String>,
    #[serde(default)]
    pub exclude_url_regex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAgentProfile {
    ChromeLinux,
    FirefoxLinux,
    SafariMacos,
    MinimalBot,
    Custom {
        user_agent: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractionConfig {
    #[serde(default)]
    pub rules: Vec<ExtractionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtractionRule {
    HtmlRecords {
        name: String,
        item_selector: String,
        fields: BTreeMap<String, HtmlField>,
    },
    JsonPath {
        name: String,
        path: String,
        #[serde(default)]
        fields: BTreeMap<String, String>,
    },
    NdjsonPath {
        name: String,
        path: String,
        #[serde(default)]
        fields: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlField {
    pub selector: String,
    #[serde(default)]
    pub attribute: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub trim: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_sqlite_path")]
    pub sqlite_path: String,
    #[serde(default = "default_jsonl_path")]
    pub jsonl_path: String,
    #[serde(default = "default_summary_path")]
    pub summary_path: String,
    #[serde(default)]
    pub save_bodies: bool,
    #[serde(default = "default_bodies_dir")]
    pub bodies_dir: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            sqlite_path: default_sqlite_path(),
            jsonl_path: default_jsonl_path(),
            summary_path: default_summary_path(),
            save_bodies: false,
            bodies_dir: default_bodies_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    pub url: Url,
    #[serde(default)]
    pub send_text: Vec<String>,
    #[serde(default = "default_ws_messages")]
    pub max_messages: usize,
    #[serde(default = "default_ws_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub json_paths: Vec<String>,
    #[serde(default = "default_ws_output_path")]
    pub output_path: String,
}

impl AppConfig {
    pub async fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()?;
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("cannot create config directory {}", parent.display()))?;
        }
        let serialized =
            serde_yaml::to_string(self).context("cannot serialize configuration as YAML")?;
        let temporary = path.with_extension("yaml.tmp");
        fs::write(&temporary, serialized)
            .await
            .with_context(|| format!("cannot write temporary config {}", temporary.display()))?;
        fs::rename(&temporary, path)
            .await
            .with_context(|| format!("cannot replace config {}", path.display()))?;
        Ok(())
    }

    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .await
            .with_context(|| format!("cannot read config {}", path.display()))?;
        let cfg: Self = serde_yaml::from_str(&raw)
            .with_context(|| format!("invalid YAML in {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.crawler.seeds.is_empty() {
            bail!("crawler.seeds cannot be empty");
        }
        if self.crawler.allowed_domains.is_empty() {
            bail!("crawler.allowed_domains cannot be empty");
        }
        for domain in &self.crawler.allowed_domains {
            let domain = domain.trim();
            if domain.is_empty() || domain.contains("://") || domain.contains('/') {
                bail!("allowed_domains entries must be host names only: {domain:?}");
            }
        }
        if self.crawler.allowed_ports.is_empty() {
            bail!("crawler.allowed_ports cannot be empty");
        }
        if self.crawler.max_pages == 0 || self.crawler.concurrency == 0 {
            bail!("max_pages and concurrency must be greater than zero");
        }
        if self.crawler.per_host_concurrency == 0 {
            bail!("per_host_concurrency must be greater than zero");
        }
        if self.crawler.timeout_secs == 0 || self.crawler.max_body_bytes == 0 {
            bail!("timeout_secs and max_body_bytes must be greater than zero");
        }
        if self.crawler.redirect_limit > 32 {
            bail!("redirect_limit cannot exceed 32");
        }
        if self.crawler.retries > 20 {
            bail!("retries cannot exceed 20");
        }
        let policy = crate::policy::UrlPolicy::from_config(&self.crawler)?;
        for (name, value) in &self.crawler.custom_headers {
            let normalized = name.trim().to_ascii_lowercase();
            if matches!(
                normalized.as_str(),
                "host"
                    | "content-length"
                    | "transfer-encoding"
                    | "connection"
                    | "proxy-connection"
                    | "upgrade"
            ) {
                bail!("custom header is managed by the HTTP stack and cannot be set: {name}");
            }
            HeaderName::try_from(name.as_str())
                .with_context(|| format!("invalid custom header name: {name}"))?;
            HeaderValue::try_from(value.as_str())
                .with_context(|| format!("invalid value for custom header: {name}"))?;
        }
        for (index, proxy) in self.crawler.proxies.iter().enumerate() {
            let parsed = Url::parse(proxy)
                .with_context(|| format!("invalid upstream proxy at index {index}"))?;
            match parsed.scheme() {
                "http" | "https" | "socks5" | "socks5h" => {}
                other => bail!("unsupported upstream proxy scheme at index {index}: {other}"),
            }
            if parsed.host_str().is_none() {
                bail!("upstream proxy at index {index} has no host");
            }
        }
        for seed in &self.crawler.seeds {
            match seed.scheme() {
                "http" | "https" => {}
                other => bail!("unsupported seed scheme: {other}"),
            }
            policy
                .validate_url(seed)
                .with_context(|| format!("seed is outside crawler policy: {seed}"))?;
        }
        validate_selector(&self.crawler.link_selector).context("invalid crawler.link_selector")?;
        for rule in &self.extraction.rules {
            match rule {
                ExtractionRule::HtmlRecords {
                    name,
                    item_selector,
                    fields,
                } => {
                    validate_selector(item_selector).with_context(|| {
                        format!("invalid item_selector in extraction rule {name:?}")
                    })?;
                    for (field_name, field) in fields {
                        validate_selector(&field.selector).with_context(|| {
                            format!("invalid selector for field {field_name:?} in extraction rule {name:?}")
                        })?;
                    }
                }
                ExtractionRule::JsonPath { name, path, fields }
                | ExtractionRule::NdjsonPath { name, path, fields } => {
                    crate::json_path::validate(path)
                        .with_context(|| format!("invalid JSONPath in extraction rule {name:?}"))?;
                    for (field_name, field_path) in fields {
                        crate::json_path::validate(field_path).with_context(|| {
                            format!("invalid JSONPath for field {field_name:?} in extraction rule {name:?}")
                        })?;
                    }
                }
            }
        }
        if let Some(websocket) = &self.websocket {
            let mut validation_url = websocket.url.clone();
            match websocket.url.scheme() {
                "ws" => validation_url
                    .set_scheme("http")
                    .map_err(|_| anyhow::anyhow!("invalid WebSocket URL"))?,
                "wss" => validation_url
                    .set_scheme("https")
                    .map_err(|_| anyhow::anyhow!("invalid WebSocket URL"))?,
                other => bail!("unsupported WebSocket scheme: {other}"),
            }
            policy
                .validate_url(&validation_url)
                .context("websocket URL is outside crawler policy")?;
            if websocket.max_messages == 0 || websocket.timeout_secs == 0 {
                bail!(
                    "websocket.max_messages and websocket.timeout_secs must be greater than zero"
                );
            }
            if websocket.output_path.trim().is_empty() {
                bail!("websocket.output_path cannot be empty");
            }
            for path in &websocket.json_paths {
                crate::json_path::validate(path)
                    .context("invalid websocket.json_paths JSONPath")?;
            }
        }
        Ok(())
    }
}

fn validate_selector(selector: &str) -> Result<()> {
    Selector::parse(selector)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn default_allowed_ports() -> Vec<u16> {
    vec![80, 443]
}
const fn default_true() -> bool {
    true
}
const fn default_max_pages() -> usize {
    500
}
const fn default_max_depth() -> usize {
    3
}
const fn default_concurrency() -> usize {
    12
}
const fn default_per_host_concurrency() -> usize {
    2
}
const fn default_delay_ms() -> u64 {
    750
}
const fn default_timeout_secs() -> u64 {
    30
}
const fn default_retries() -> usize {
    3
}
const fn default_redirect_limit() -> usize {
    8
}
const fn default_max_body_bytes() -> usize {
    8 * 1024 * 1024
}
const fn default_ws_messages() -> usize {
    100
}
const fn default_ws_timeout() -> u64 {
    30
}
fn default_profile() -> UserAgentProfile {
    UserAgentProfile::MinimalBot
}
fn default_user_agent_token() -> String {
    "RamaControlledExtractor".to_owned()
}
fn default_link_selector() -> String {
    "a[href]".to_owned()
}
fn default_sqlite_path() -> String {
    "output/extractor.sqlite3".to_owned()
}
fn default_jsonl_path() -> String {
    "output/records.jsonl".to_owned()
}
fn default_summary_path() -> String {
    "output/summary.json".to_owned()
}
fn default_bodies_dir() -> String {
    "output/bodies".to_owned()
}
fn default_ws_output_path() -> String {
    "output/websocket.jsonl".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn valid_config() -> Value {
        json!({
            "crawler": {
                "seeds": ["https://example.com/"],
                "allowed_domains": ["example.com"],
                "link_selector": "a[href]"
            },
            "extraction": {"rules": [
                {"kind": "html_records", "name": "html", "item_selector": "article",
                 "fields": {"title": {"selector": "h1"}}},
                {"kind": "json_path", "name": "json", "path": "$.items[*]",
                 "fields": {"title": "$.name"}},
                {"kind": "ndjson_path", "name": "ndjson", "path": "$",
                 "fields": {"title": "$['display-name']"}}
            ]},
            "websocket": {"url": "wss://example.com/events", "json_paths": ["$.items[0]"]}
        })
    }

    #[test]
    fn accepts_valid_selectors_and_bundled_example() -> Result<()> {
        let config: AppConfig = serde_json::from_value(valid_config())?;
        config.validate()?;
        let example: AppConfig = serde_yaml::from_str(include_str!("../config/example.yaml"))?;
        example.validate()
    }

    #[test]
    fn rejects_invalid_css_selectors() -> Result<()> {
        for (pointer, context) in [
            ("/crawler/link_selector", "crawler.link_selector"),
            ("/extraction/rules/0/item_selector", "item_selector"),
            (
                "/extraction/rules/0/fields/title/selector",
                "field \"title\"",
            ),
        ] {
            let mut value = valid_config();
            *value
                .pointer_mut(pointer)
                .context("missing fixture selector")? = json!("[");
            let config: AppConfig = serde_json::from_value(value)?;
            let error = config
                .validate()
                .err()
                .context("invalid CSS was accepted")?;
            assert!(error.to_string().contains(context), "{error:#}");
        }
        Ok(())
    }

    #[test]
    fn rejects_invalid_json_paths() -> Result<()> {
        for pointer in [
            "/extraction/rules/1/path",
            "/extraction/rules/1/fields/title",
            "/extraction/rules/2/path",
            "/extraction/rules/2/fields/title",
            "/websocket/json_paths/0",
        ] {
            let mut value = valid_config();
            *value
                .pointer_mut(pointer)
                .context("missing fixture JSONPath")? = json!("$.items[");
            let config: AppConfig = serde_json::from_value(value)?;
            let error = config
                .validate()
                .err()
                .context("invalid JSONPath was accepted")?;
            assert!(error.to_string().contains("JSONPath"), "{error:#}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn invalid_selectors_do_not_replace_saved_config() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.yaml");
        let config: AppConfig = serde_json::from_value(valid_config())?;
        config.save(&path).await?;
        let original = fs::read(&path).await?;
        let mut invalid = config;
        invalid.crawler.link_selector = "[".to_owned();
        assert!(invalid.save(&path).await.is_err());
        assert_eq!(fs::read(&path).await?, original);
        Ok(())
    }
}
