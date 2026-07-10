<div align="center">

# RamaCrawl Studio

### Controlled web crawling and structured data extraction in Rust

A policy-driven crawler with a local web control panel, built on [Rama](https://github.com/plabayo/rama) and designed for authorized HTML, JSON, NDJSON and WebSocket extraction.

[![Rust](https://img.shields.io/badge/Rust-1.96%2B-000000?logo=rust)](https://www.rust-lang.org/)
[![Rama](https://img.shields.io/badge/Rama-0.3.0-5b34da)](https://github.com/plabayo/rama)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Status](https://img.shields.io/badge/status-active%20development-orange)](#project-status)

</div>

---

## Overview

**RamaCrawl Studio** is a controlled data-extraction platform written in Rust. It combines a configurable crawler, a local HTML control panel and a traceable persistence layer for extracting public or explicitly authorized information.

The project prioritizes reproducibility, safety and operational control rather than maximum crawling volume. Every seed, discovered link and redirect is evaluated against explicit policies before a request is issued.

Typical use cases include:

- public catalog extraction;
- technical documentation indexing;
- authorized website inventories;
- structured monitoring of public content;
- JSON and NDJSON API collection;
- controlled WebSocket event capture;
- research and interoperability testing in authorized environments.

> [!IMPORTANT]
> Use this software only against systems you own or are explicitly authorized to access. Respect applicable law, website terms, `robots.txt`, rate limits and data-protection requirements.

## Key features

### Local web control panel

- Edit crawler parameters from a browser.
- Validate configuration before execution.
- Save the active YAML configuration atomically.
- Start and cancel scans.
- Follow progress and errors during execution.
- Open an HTML dashboard with the extracted results.
- No external JavaScript libraries or cloud services are required.
- Binds to `127.0.0.1` by default.

### Controlled crawling

- Mandatory domain and port allowlists.
- Optional subdomain authorization.
- Denied path prefixes.
- URL inclusion and exclusion regular expressions.
- Validation of seeds, discovered links and every redirect target.
- SSRF protection and private-network blocking by default.
- Embedded-credential rejection.
- Maximum page count, depth and body size.
- Global and per-host concurrency limits.
- Configurable delay between requests.
- Timeouts, retries and redirect limits.
- `robots.txt` support with fail-open or fail-closed behavior.

### Network stack powered by Rama

- HTTP/1.1 and HTTP/2 client support.
- Reusable connection pool.
- Rustls-based portable TLS by default.
- Optional BoringSSL HTTP/TLS profile emulation.
- HTTP, HTTPS, SOCKS5 and SOCKS5H upstream proxies.
- Proxy rotation.
- Response decompression.
- Exponential retry with jitter for transient failures.
- WebSocket client support.
- Streaming JSON capture.

### Structured extraction

- HTML extraction using CSS selectors.
- Attribute, text and multi-value extraction.
- JSON and NDJSON extraction with deterministic JSONPath rules.
- JSON streaming for large responses.
- Link discovery from configurable selectors.
- Request, response and body SHA-256 fingerprints.

### Storage and observability

- SQLite persistence.
- JSON Lines output.
- Optional response-body storage.
- Execution summary in JSON.
- Structured logs through `tracing`.
- Sensitive-header and proxy-credential redaction.
- Standalone HTML results dashboard.

## Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                    Local HTML control panel                 │
│ configure · validate · save · scan · cancel · inspect      │
└──────────────────────────────┬──────────────────────────────┘
                               │ local API
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    Configuration and policy                 │
│ domains · ports · paths · regex · robots · limits · SSRF   │
└──────────────────────────────┬──────────────────────────────┘
                               │ approved URL
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                      Rama network engine                    │
│ TLS · pooling · proxies · retries · redirects · WebSocket  │
└──────────────────────────────┬──────────────────────────────┘
                               │ response
                  ┌────────────┴────────────┐
                  ▼                         ▼
┌───────────────────────────┐  ┌─────────────────────────────┐
│ Extraction engine         │  │ Streaming and WebSocket     │
│ HTML · JSON · NDJSON      │  │ JSON capture · event data   │
└──────────────┬────────────┘  └──────────────┬──────────────┘
               └───────────────┬──────────────┘
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                   Traceable local storage                   │
│ SQLite · JSONL · bodies · hashes · summary · HTML dashboard│
└─────────────────────────────────────────────────────────────┘
```

## Technology stack

| Component | Technology |
|---|---|
| Language | Rust 2024 Edition |
| Network framework | Rama 0.3.0 |
| Web control API | Axum 0.8 |
| Async runtime | Tokio |
| HTML parsing | `scraper` / html5ever |
| Configuration | YAML with Serde |
| Local database | SQLite through Rusqlite |
| Logging | `tracing` |
| TLS | Rustls or optional BoringSSL |

## Requirements

- Rust 1.96 or newer.
- Cargo.
- Linux, macOS or Windows.
- Docker Engine and Docker Compose for containerized execution.
- Native BoringSSL build tools only when enabling `boring-emulation`.

The repository includes `rust-toolchain.toml`, so Rustup can select the expected toolchain automatically.

## Quick start

### 1. Clone the repository

```bash
git clone https://github.com/ansonTGN/ramacrawl-studio.git
cd ramacrawl-studio
```

### 2. Prepare a configuration

```bash
cp config/example.yaml config/local.yaml
```

Edit `config/local.yaml` and add only domains you are authorized to crawl.

### 3. Verify the project

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Or use:

```bash
make verify
```

### 4. Build the release binary

```bash
cargo build --release
```

### 5. Open the web control panel

```bash
./target/release/rama-controlled-extractor \
  serve \
  --config config/local.yaml
```

Open:

```text
http://127.0.0.1:8090/
```

To prevent automatic browser launch:

```bash
./target/release/rama-controlled-extractor \
  serve \
  --config config/local.yaml \
  --no-open
```

## Web control workflow

1. Enter one or more seed URLs.
2. Define the exact authorized domains and ports.
3. Review `robots.txt`, path and regex policies.
4. Configure limits, delays, concurrency, retries and timeouts.
5. Define HTML, JSON or NDJSON extraction rules.
6. Press **Validate**.
7. Press **Save** to update the YAML configuration.
8. Press **Start scan**.
9. Follow the live progress indicators.
10. Open **Results** when the scan completes.

The backend validates the complete configuration again before writing the file or starting the crawler.

## CLI usage

Show all commands:

```bash
./target/release/rama-controlled-extractor --help
```

### Validate configuration

```bash
./target/release/rama-controlled-extractor \
  validate \
  --config config/local.yaml
```

### Run a crawl

```bash
RUST_LOG=info \
./target/release/rama-controlled-extractor \
  crawl \
  --config config/local.yaml
```

### Generate a results dashboard

```bash
./target/release/rama-controlled-extractor \
  dashboard \
  --config config/local.yaml \
  --output output/dashboard.html
```

Open it with:

```bash
xdg-open output/dashboard.html
```

### Capture selected JSON values as a stream

```bash
./target/release/rama-controlled-extractor \
  json-stream \
  --config config/local.yaml \
  --url 'https://authorized.example/catalog.json' \
  --path '$.products[*]' \
  --path '$.metadata.version' \
  --output output/json-stream.json
```

### Capture an authorized WebSocket

Add a `websocket` section to the YAML configuration and run:

```bash
./target/release/rama-controlled-extractor \
  websocket \
  --config config/local.yaml
```

## Configuration example

```yaml
crawler:
  seeds:
    - "https://authorized.example/"

  allowed_domains:
    - "authorized.example"

  allowed_ports: [80, 443]
  denied_path_prefixes:
    - "/admin"
    - "/logout"

  allow_subdomains: false
  allow_private_networks: false

  respect_robots_txt: true
  robots_fail_closed: true
  robots_user_agent: "RamaCrawlStudio"

  max_pages: 40
  max_depth: 2
  concurrency: 4
  per_host_concurrency: 1
  min_delay_ms: 1500
  timeout_secs: 30
  retries: 2
  redirect_limit: 5
  max_body_bytes: 8388608

  user_agent_profile: minimal_bot
  custom_headers:
    dnt: "1"

  proxies: []
  discover_links: true
  link_selector: "a[href]"
  include_url_regex: null
  exclude_url_regex: "(?i)(/cart|/checkout|/login|/account)"

extraction:
  rules:
    - kind: html_records
      name: articles
      item_selector: "article"
      fields:
        title:
          selector: "h1, h2, h3"
          trim: true
        url:
          selector: "h1 a[href], h2 a[href], h3 a[href]"
          attribute: "href"
          trim: true
        summary:
          selector: "p"
          trim: true

storage:
  sqlite_path: "output/extractor.sqlite3"
  jsonl_path: "output/records.jsonl"
  summary_path: "output/summary.json"
  save_bodies: false
  bodies_dir: "output/bodies"
```

See [`config/example.yaml`](config/example.yaml) for the full configuration model.

## Output files

A normal scan can produce:

```text
output/
├── extractor.sqlite3
├── records.jsonl
├── summary.json
├── dashboard.html
└── bodies/
```

### JSONL record format

```json
{
  "crawl_id": "f637fba3-4994-442e-83d9-eb308a2e0d0a",
  "source_url": "https://authorized.example/news/",
  "rule": "articles",
  "ordinal": 0,
  "extracted_at": "2026-07-10T10:27:25Z",
  "data": {
    "title": "Example article",
    "url": "https://authorized.example/news/example",
    "summary": "Structured content extracted by a configured rule."
  }
}
```

## Docker

Build and run the local control panel:

```bash
export LOCAL_UID=$(id -u)
export LOCAL_GID=$(id -g)
docker compose up --build
```

Open:

```text
http://127.0.0.1:8090/
```

Run individual CLI commands:

```bash
docker compose run --rm extractor \
  validate --config /data/config/example.yaml

docker compose run --rm extractor \
  crawl --config /data/config/example.yaml
```

The Compose port is published only on `127.0.0.1` by default.

## TLS modes

### Portable mode — Rustls

This is the default:

```bash
cargo build --release
```

Equivalent explicit command:

```bash
cargo build \
  --release \
  --no-default-features \
  --features portable-tls
```

### Optional HTTP/TLS profile emulation — BoringSSL

```bash
cargo build \
  --release \
  --no-default-features \
  --features boring-emulation
```

The two TLS modes must not be enabled simultaneously.

## Security model

RamaCrawl Studio is designed with restrictive defaults:

- explicit domain and port allowlists;
- private and special-purpose IP ranges blocked by default;
- DNS results checked before access;
- each redirect target revalidated;
- embedded URL credentials rejected;
- `robots.txt` policy support;
- bounded response bodies;
- bounded configuration API payloads;
- atomic configuration writes;
- local-only web binding by default;
- remote binding requires `--allow-remote`;
- output deletion restricted to paths below `output/`;
- sensitive HTTP headers and proxy credentials redacted;
- Rust `unsafe` code forbidden by project lints.

Read [`SECURITY.md`](SECURITY.md) before exposing the application to any network.

## Project structure

```text
ramacrawl-studio/
├── config/                 # YAML configurations
├── output/                 # Generated local results
├── scripts/                # Verification and launch scripts
├── src/
│   ├── config.rs           # Configuration loading and validation
│   ├── crawler.rs          # Crawl orchestration
│   ├── dashboard.rs        # Standalone HTML results dashboard
│   ├── extract.rs          # HTML/JSON extraction rules
│   ├── fingerprint.rs      # SHA-256 fingerprints
│   ├── json_path.rs        # Deterministic JSONPath subset
│   ├── network.rs          # Rama HTTP/TLS/proxy client stack
│   ├── policy.rs           # URL, port, DNS and SSRF policy
│   ├── robots.rs           # robots.txt evaluation
│   ├── storage.rs          # SQLite, JSONL and body storage
│   ├── stream_json.rs      # Streaming JSON capture
│   ├── webapp.rs           # Local Axum control API
│   └── websocket.rs        # Authorized WebSocket capture
├── web/                    # Browser interface
├── Dockerfile
├── docker-compose.yml
├── Makefile
└── Cargo.toml
```

## Quality checks

Run the complete local verification pipeline:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

The project lints:

- forbid `unsafe` code;
- deny `unwrap()`;
- deny `expect()`;
- treat Clippy warnings as errors in the verification workflow.

## Responsible use

This repository does not provide authorization to access any website or service.

Before running a scan:

1. Confirm ownership or explicit authorization.
2. Review the target's terms and technical policies.
3. Use a descriptive User-Agent when appropriate.
4. Configure conservative concurrency and delays.
5. Avoid collecting unnecessary personal data.
6. Protect extracted data and configuration files.
7. Stop immediately if the target shows instability or requests cessation.

Do not use this project to bypass authentication, anti-bot controls, access restrictions, paywalls or security mechanisms.

## Roadmap

- [ ] Authentication for optional remote deployments.
- [ ] Server-Sent Events for push-based scan progress.
- [ ] Visual extraction-rule builder.
- [ ] Scheduled scans.
- [ ] Incremental crawl and content-change detection.
- [ ] CSV and Parquet exporters.
- [ ] OpenTelemetry and Prometheus integration.
- [ ] Per-domain policy profiles.
- [ ] Pluggable storage backends.
- [ ] Internationalized web interface.

## Project status

The project is under active development. Version `0.3.0` provides a functional local control panel, the crawler engine, structured extractors and local result storage. Review configuration and perform tests before using it in operational environments.

## Contributing

Contributions are welcome when they preserve the controlled and authorization-first design of the project.

Recommended workflow:

1. Create a feature branch.
2. Add or update tests.
3. Run `make verify`.
4. Document changes affecting configuration or security.
5. Open a pull request with a clear technical explanation.

## License

Licensed under either of the following, at your option:

- Apache License, Version 2.0 — [`LICENSE`](LICENSE)
- MIT License — [`LICENSE-MIT`](LICENSE-MIT)

## Acknowledgements

- [Rama](https://github.com/plabayo/rama), the modular Rust networking framework used by the crawler client stack.
- [Axum](https://github.com/tokio-rs/axum), used by the local control API.
- The Rust ecosystem projects that provide parsing, persistence, asynchronous execution and observability.

## Author

**Angel A. Urbina**

- GitHub: [@ansonTGN](https://github.com/ansonTGN)
- LinkedIn: [angelurbina](https://www.linkedin.com/in/angelurbina/)

---

<div align="center">

**RamaCrawl Studio — controlled extraction, explicit policies, traceable results.**

</div>
