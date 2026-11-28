use crate::{
    config::StorageConfig,
    model::{CachedPage, CrawlSummary, ExtractedRecord, PageRecord},
};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
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
            let has_change: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('pages') WHERE name = 'change_type')",
                [], |row| row.get(0),
            )?;
            if !has_change {
                connection.execute("ALTER TABLE pages ADD COLUMN change_type TEXT", [])?;
            }
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
                    response_headers_json, body_sha256, error, change_type
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                          ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
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
                    page.change.map(|change| change.as_str()),
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
        let connection = Arc::clone(&self.connection);
        let summary_json = serde_json::to_string(summary)?;
        let crawl_id = summary.crawl_id.clone();
        let finished_at = summary.finished_at.to_rfc3339();
        tokio::task::spawn_blocking(move || -> Result<()> {
            connection.lock().execute(
                "INSERT OR REPLACE INTO crawl_runs (crawl_id, finished_at, summary_json) VALUES (?1, ?2, ?3)",
                params![crawl_id, finished_at, summary_json],
            )?;
            Ok(())
        }).await??;
        let bytes = serde_json::to_vec_pretty(summary)?;
        fs::write(&self.cfg.summary_path, bytes)
            .await
            .with_context(|| format!("cannot write {}", self.cfg.summary_path))?;
        Ok(())
    }

    pub async fn cached_page(&self, url: &str, context: &str) -> Result<Option<CachedPage>> {
        let connection = Arc::clone(&self.connection);
        let (url, context) = (url.to_owned(), context.to_owned());
        tokio::task::spawn_blocking(move || -> Result<Option<CachedPage>> {
            let raw: Option<String> = connection.lock().query_row(
                "SELECT state_json FROM url_cache WHERE requested_url = ?1 AND context_sha256 = ?2",
                params![url, context], |row| row.get(0),
            ).optional()?;
            raw.map(|raw| serde_json::from_str(&raw).context("invalid cached page"))
                .transpose()
        })
        .await?
    }

    pub async fn save_cached_page(
        &self,
        url: &str,
        context: &str,
        page: &CachedPage,
    ) -> Result<()> {
        let connection = Arc::clone(&self.connection);
        let (url, context, state) = (
            url.to_owned(),
            context.to_owned(),
            serde_json::to_string(page)?,
        );
        tokio::task::spawn_blocking(move || -> Result<()> {
            connection.lock().execute(
                "INSERT OR REPLACE INTO url_cache (requested_url, context_sha256, state_json) VALUES (?1, ?2, ?3)",
                params![url, context, state],
            )?;
            Ok(())
        }).await??;
        Ok(())
    }

    pub async fn history(path: String, limit: usize) -> Result<Vec<CrawlSummary>> {
        match fs::metadata(&path).await {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        tokio::task::spawn_blocking(move || -> Result<Vec<CrawlSummary>> {
            let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='crawl_runs')", [], |row| row.get(0),
            )?;
            if !exists { return Ok(Vec::new()); }
            let mut statement = connection.prepare("SELECT summary_json FROM crawl_runs ORDER BY finished_at DESC, rowid DESC LIMIT ?1")?;
            let rows = statement.query_map([i64::try_from(limit.clamp(1, 100))?], |row| row.get::<_, String>(0))?;
            rows.map(|raw| serde_json::from_str(&raw?).context("invalid crawl history summary")).collect()
        }).await?
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
CREATE TABLE IF NOT EXISTS url_cache (
    requested_url TEXT NOT NULL,
    context_sha256 TEXT NOT NULL,
    state_json TEXT NOT NULL,
    PRIMARY KEY (requested_url, context_sha256)
);
CREATE TABLE IF NOT EXISTS crawl_runs (
    crawl_id TEXT PRIMARY KEY,
    finished_at TEXT NOT NULL,
    summary_json TEXT NOT NULL
);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn legacy_database_is_migrated_without_removing_results() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join("legacy.sqlite3");
        {
            let connection = Connection::open(&database)?;
            connection.execute_batch(include_str!("../tests/fixtures/storage_v0_3.sql"))?;
            connection.execute("INSERT INTO pages (crawl_id,requested_url,final_url,depth,fetched_at,status,http_version,body_bytes,elapsed_ms,redirect_chain_json,request_profile,request_header_fingerprint,response_fingerprint,response_headers_json,body_sha256) VALUES ('old','https://example.com/','https://example.com/',0,'2026-01-01',200,'HTTP/1.1',3,10,'[]','minimal_bot','request','response','{}','legacy-hash')",[])?;
            connection.execute("INSERT INTO records (crawl_id,source_url,rule_name,ordinal,extracted_at,data_json) VALUES ('old','https://example.com','test',0,'2026-01-01','{}')",[])?;
        }
        let cfg = StorageConfig {
            sqlite_path: database.display().to_string(),
            jsonl_path: directory.path().join("records.jsonl").display().to_string(),
            summary_path: directory.path().join("summary.json").display().to_string(),
            ..Default::default()
        };
        assert!(
            Storage::history(cfg.sqlite_path.clone(), 20)
                .await?
                .is_empty()
        );
        let storage = Storage::open(cfg).await?;
        let count: i64 =
            storage
                .connection
                .lock()
                .query_row("SELECT count(*) FROM records", [], |row| row.get(0))?;
        assert_eq!(count, 1);
        let (hash, change): (String, Option<String>) = storage.connection.lock().query_row(
            "SELECT body_sha256, change_type FROM pages WHERE crawl_id='old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(hash, "legacy-hash");
        assert_eq!(change, None);
        let has_change: bool = storage.connection.lock().query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('pages') WHERE name='change_type')",
            [],
            |row| row.get(0),
        )?;
        assert!(has_change);
        assert!(
            storage
                .cached_page("https://example.com/", "new-context")
                .await?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn older_summaries_default_to_zero_change_counts() -> Result<()> {
        let summary: CrawlSummary = serde_json::from_str(
            r#"{"crawl_id":"old","started_at":"2026-01-01T00:00:00Z","finished_at":"2026-01-01T00:00:01Z","pages_scheduled":1,"pages_fetched":1,"pages_failed":0,"pages_blocked":0,"records_extracted":1,"bytes_downloaded":100}"#,
        )?;
        assert_eq!(summary.changes.pages_new, 0);
        assert_eq!(summary.changes.pages_unchanged, 0);
        Ok(())
    }
}
