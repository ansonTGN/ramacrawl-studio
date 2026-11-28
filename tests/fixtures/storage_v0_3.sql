
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
