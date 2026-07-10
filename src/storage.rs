use crate::{
    config::StorageConfig,
    model::{CrawlSummary, ExtractedRecord, PageRecord},
};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
};

#[derive(Clone)]
pub struct Storage {
    cfg: Arc<StorageConfig>,
    connection: Arc<Mutex<Connection>>,
    jsonl_lock: Arc<tokio::sync::Mutex<()>>,
}

impl Storage {
    pub async fn open(cfg: StorageConfig) -> Result<Self> {
        ensure_parent(&cfg.sqlite_path).await?;
        ensure_parent(&cfg.jsonl_path).await?;
        ensure_parent(&cfg.summary_path).await?;
        if cfg.save_bodies {
            fs::create_dir_all(&cfg.bodies_dir)
                .await
                .with_context(|| format!("cannot create {}", cfg.bodies_dir))?;
        }

        let path = cfg.sqlite_path.clone();
        let connection = tokio::task::spawn_blocking(move || -> Result<Connection> {
            let connection = Connection::open(&path)
                .with_context(|| format!("cannot open SQLite database {path}"))?;
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "foreign_keys", "ON")?;
            connection.execute_batch(SCHEMA)?;
            Ok(connection)
        })
        .await??;

        Ok(Self {
            cfg: Arc::new(cfg),
            connection: Arc::new(Mutex::new(connection)),
            jsonl_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub async fn save_page(&self, page: PageRecord, body: Option<Vec<u8>>) -> Result<()> {
        if self.cfg.save_bodies
            && let Some(body) = body
        {
            let body_path =
                Path::new(&self.cfg.bodies_dir).join(format!("{}.body", page.body_sha256));
            if fs::metadata(&body_path).await.is_err() {
                fs::write(&body_path, body)
                    .await
                    .with_context(|| format!("cannot write {}", body_path.display()))?;
            }
        }

        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let headers = serde_json::to_string(&page.response_headers)?;
            let redirects = serde_json::to_string(&page.redirect_chain)?;
            connection.lock().execute(
                r#"
                INSERT OR REPLACE INTO pages (
                    crawl_id, requested_url, final_url, parent_url, depth, fetched_at,
                    status, http_version, content_type, body_bytes, elapsed_ms,
                    redirect_chain_json, request_profile, upstream_proxy,
                    request_header_fingerprint, response_fingerprint,
                    response_headers_json, body_sha256, error
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                          ?13, ?14, ?15, ?16, ?17, ?18, ?19)
                "#,
                params![
                    page.crawl_id,
                    page.requested_url,
                    page.final_url,
                    page.parent_url,
                    i64::try_from(page.depth)?,
                    page.fetched_at.to_rfc3339(),
                    i64::from(page.status),
                    page.http_version,
                    page.content_type,
                    i64::try_from(page.body_bytes)?,
                    i64::try_from(page.elapsed_ms)?,
                    redirects,
                    page.request_profile,
                    page.upstream_proxy,
                    page.request_header_fingerprint,
                    page.response_fingerprint,
                    headers,
                    page.body_sha256,
                    page.error,
                ],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    pub async fn save_records(&self, records: &[ExtractedRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        {
            let _guard = self.jsonl_lock.lock().await;
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.cfg.jsonl_path)
                .await
                .with_context(|| format!("cannot open {}", self.cfg.jsonl_path))?;
            for record in records {
                let mut line = serde_json::to_vec(record)?;
                line.push(b'\n');
                file.write_all(&line).await?;
            }
            file.flush().await?;
        }

        let owned = records.to_vec();
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut connection = connection.lock();
            let transaction = connection.transaction()?;
            {
                let mut statement = transaction.prepare(
                    r#"
                    INSERT INTO records (
                        crawl_id, source_url, rule_name, ordinal, extracted_at, data_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    "#,
                )?;
                for record in owned {
                    statement.execute(params![
                        record.crawl_id,
                        record.source_url,
                        record.rule,
                        i64::try_from(record.ordinal)?,
                        record.extracted_at.to_rfc3339(),
                        serde_json::to_string(&record.data)?,
                    ])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    pub async fn save_summary(&self, summary: &CrawlSummary) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(summary)?;
        fs::write(&self.cfg.summary_path, bytes)
            .await
            .with_context(|| format!("cannot write {}", self.cfg.summary_path))?;
        Ok(())
    }
}

async fn ensure_parent(path: &str) -> Result<()> {
    let parent = PathBuf::from(path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent)
        .await
        .with_context(|| format!("cannot create {}", parent.display()))
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS pages (
    crawl_id TEXT NOT NULL,
    requested_url TEXT NOT NULL,
    final_url TEXT NOT NULL,
    parent_url TEXT,
    depth INTEGER NOT NULL,
    fetched_at TEXT NOT NULL,
    status INTEGER NOT NULL,
    http_version TEXT NOT NULL,
    content_type TEXT,
    body_bytes INTEGER NOT NULL,
    elapsed_ms INTEGER NOT NULL,
    redirect_chain_json TEXT NOT NULL,
    request_profile TEXT NOT NULL,
    upstream_proxy TEXT,
    request_header_fingerprint TEXT NOT NULL,
    response_fingerprint TEXT NOT NULL,
    response_headers_json TEXT NOT NULL,
    body_sha256 TEXT NOT NULL,
    error TEXT,
    PRIMARY KEY (crawl_id, requested_url)
);

CREATE INDEX IF NOT EXISTS pages_final_url_idx ON pages(final_url);
CREATE INDEX IF NOT EXISTS pages_status_idx ON pages(status);
CREATE INDEX IF NOT EXISTS pages_body_sha256_idx ON pages(body_sha256);

CREATE TABLE IF NOT EXISTS records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crawl_id TEXT NOT NULL,
    source_url TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    extracted_at TEXT NOT NULL,
    data_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS records_crawl_id_idx ON records(crawl_id);
CREATE INDEX IF NOT EXISTS records_source_url_idx ON records(source_url);
CREATE INDEX IF NOT EXISTS records_rule_idx ON records(rule_name);
"#;
