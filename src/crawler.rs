use crate::{
    config::AppConfig,
    extract, fingerprint,
    model::{CrawlProgress, CrawlSummary, CrawlTarget, PageRecord},
    network::NetworkEngine,
    policy::UrlPolicy,
    robots::{RobotsCache, RobotsRules},
    storage::Storage,
    user_agent::RequestProfile,
};
use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use dashmap::DashMap;
use rama::{
    Service,
    error::BoxError,
    http::{Request, Response},
};
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
    time::sleep,
};
use url::Url;
use uuid::Uuid;

pub async fn run<S>(config: AppConfig, client: S) -> Result<CrawlSummary>
where
    S: Service<Request, Output = Response> + Clone + Send + Sync + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    run_with_progress(config, client, |_| {}).await
}

pub async fn run_with_progress<S, F>(
    config: AppConfig,
    client: S,
    report: F,
) -> Result<CrawlSummary>
where
    S: Service<Request, Output = Response> + Clone + Send + Sync + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
    F: Fn(CrawlProgress) + Send + Sync + 'static,
{
    let started_at = Utc::now();
    let crawl_id = Uuid::new_v4().to_string();
    let crawler_cfg = Arc::new(config.crawler.clone());
    let extraction_cfg = Arc::new(config.extraction.clone());
    let policy = Arc::new(UrlPolicy::from_config(&crawler_cfg)?);
    let profile = RequestProfile::from_config(&crawler_cfg.user_agent_profile);
    let profile_name = profile.name.clone();
    let network = Arc::new(NetworkEngine::new(
        client,
        Arc::clone(&crawler_cfg),
        Arc::clone(&policy),
        profile,
    ));
    let storage = Storage::open(config.storage).await?;
    let robots = Arc::new(RobotsCache::default());
    let limiter = Arc::new(HostLimiter::new(crawler_cfg.per_host_concurrency));

    let mut queue = VecDeque::new();
    for seed in &crawler_cfg.seeds {
        queue.push_back(CrawlTarget {
            url: normalize_url(seed.clone()),
            depth: 0,
            parent: None,
        });
    }

    let mut visited = HashSet::<String>::new();
    let mut join_set = JoinSet::new();
    let mut pages_scheduled = 0usize;
    let mut pages_fetched = 0usize;
    let mut pages_failed = 0usize;
    let mut pages_blocked = 0usize;
    let mut records_extracted = 0usize;
    let mut bytes_downloaded = 0u64;

    report(progress_snapshot(
        &crawl_id,
        pages_scheduled,
        pages_fetched,
        pages_failed,
        pages_blocked,
        records_extracted,
        bytes_downloaded,
        queue.len(),
        join_set.len(),
        crawler_cfg.max_pages,
    ));

    loop {
        while join_set.len() < crawler_cfg.concurrency
            && pages_scheduled < crawler_cfg.max_pages
            && !queue.is_empty()
        {
            let Some(target) = queue.pop_front() else {
                break;
            };
            let key = target.url.as_str().to_owned();
            if !visited.insert(key) {
                continue;
            }
            pages_scheduled += 1;

            let crawl_id = crawl_id.clone();
            let crawler_cfg = Arc::clone(&crawler_cfg);
            let extraction_cfg = Arc::clone(&extraction_cfg);
            let policy = Arc::clone(&policy);
            let network = Arc::clone(&network);
            let robots = Arc::clone(&robots);
            let limiter = Arc::clone(&limiter);
            let storage = storage.clone();
            let profile_name = profile_name.clone();
            join_set.spawn(async move {
                let result = process_target(
                    &crawl_id,
                    target.clone(),
                    &crawler_cfg,
                    &extraction_cfg.rules,
                    &policy,
                    &network,
                    &robots,
                    &limiter,
                    &storage,
                    &profile_name,
                )
                .await;
                (target, result)
            });
        }

        if join_set.is_empty() {
            break;
        }

        let Some(joined) = join_set.join_next().await else {
            break;
        };
        let (target, result) = joined.context("crawler task panicked")?;
        match result {
            Ok(ProcessResult::Fetched {
                links,
                body_bytes,
                record_count,
            }) => {
                pages_fetched += 1;
                bytes_downloaded = bytes_downloaded.saturating_add(body_bytes as u64);
                records_extracted = records_extracted.saturating_add(record_count);
                for link in links {
                    if pages_scheduled + queue.len() >= crawler_cfg.max_pages {
                        break;
                    }
                    queue.push_back(CrawlTarget {
                        url: link,
                        depth: target.depth + 1,
                        parent: Some(target.url.clone()),
                    });
                }
            }
            Ok(ProcessResult::Blocked(reason)) => {
                pages_blocked += 1;
                tracing::info!(url = %target.url, %reason, "URL blocked by crawl policy");
            }
            Err(error) => {
                pages_failed += 1;
                tracing::error!(url = %target.url, %error, "page fetch failed");
                let page = error_page(&crawl_id, &target, &profile_name, &error);
                if let Err(storage_error) = storage.save_page(page, None).await {
                    tracing::error!(%storage_error, "failed to persist page error");
                }
            }
        }

        report(progress_snapshot(
            &crawl_id,
            pages_scheduled,
            pages_fetched,
            pages_failed,
            pages_blocked,
            records_extracted,
            bytes_downloaded,
            queue.len(),
            join_set.len(),
            crawler_cfg.max_pages,
        ));
    }

    let summary = CrawlSummary {
        crawl_id,
        started_at,
        finished_at: Utc::now(),
        pages_scheduled,
        pages_fetched,
        pages_failed,
        pages_blocked,
        records_extracted,
        bytes_downloaded,
    };
    storage.save_summary(&summary).await?;
    report(progress_snapshot(
        &summary.crawl_id,
        summary.pages_scheduled,
        summary.pages_fetched,
        summary.pages_failed,
        summary.pages_blocked,
        summary.records_extracted,
        summary.bytes_downloaded,
        0,
        0,
        crawler_cfg.max_pages,
    ));
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn progress_snapshot(
    crawl_id: &str,
    pages_scheduled: usize,
    pages_fetched: usize,
    pages_failed: usize,
    pages_blocked: usize,
    records_extracted: usize,
    bytes_downloaded: u64,
    queued: usize,
    active: usize,
    max_pages: usize,
) -> CrawlProgress {
    CrawlProgress {
        crawl_id: crawl_id.to_owned(),
        updated_at: Utc::now(),
        pages_scheduled,
        pages_fetched,
        pages_failed,
        pages_blocked,
        records_extracted,
        bytes_downloaded,
        queued,
        active,
        max_pages,
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_target<S>(
    crawl_id: &str,
    target: CrawlTarget,
    crawler_cfg: &crate::config::CrawlerConfig,
    extraction_rules: &[crate::config::ExtractionRule],
    policy: &UrlPolicy,
    network: &NetworkEngine<S>,
    robots_cache: &RobotsCache,
    limiter: &HostLimiter,
    storage: &Storage,
    profile_name: &str,
) -> Result<ProcessResult>
where
    S: Service<Request, Output = Response> + Clone + Send + Sync + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    if target.depth > crawler_cfg.max_depth {
        return Ok(ProcessResult::Blocked("maximum depth exceeded".to_owned()));
    }
    if let Err(error) = policy.validate_network_destination(&target.url).await {
        return Ok(ProcessResult::Blocked(error.to_string()));
    }

    let fetch = network
        .fetch_with_request_guard(&target.url, |url| async move {
            let mut delay_ms = crawler_cfg.min_delay_ms;
            if crawler_cfg.respect_robots_txt {
                let rules = load_robots(
                    &url,
                    policy,
                    network,
                    robots_cache,
                    limiter,
                    crawler_cfg.min_delay_ms,
                    crawler_cfg.robots_fail_closed,
                )
                .await;
                if !rules.allowed(&crawler_cfg.robots_user_agent, &url) {
                    return Ok(None);
                }
                delay_ms = delay_ms.max(
                    rules
                        .crawl_delay_ms(&crawler_cfg.robots_user_agent)
                        .unwrap_or_default(),
                );
            }
            limiter
                .acquire(&url, Duration::from_millis(delay_ms))
                .await
                .map(Some)
        })
        .await?;
    let Some(fetch) = fetch else {
        return Ok(ProcessResult::Blocked(
            "robots.txt disallows URL".to_owned(),
        ));
    };
    let body_sha256 = fingerprint::sha256(&fetch.body);
    let records = if (200..300).contains(&fetch.status) {
        extract::extract_records(
            crawl_id,
            &fetch.final_url,
            fetch.content_type.as_deref(),
            &fetch.body,
            extraction_rules,
        )
    } else {
        Vec::new()
    };

    let links = if crawler_cfg.discover_links
        && target.depth < crawler_cfg.max_depth
        && (200..300).contains(&fetch.status)
        && is_html(fetch.content_type.as_deref())
    {
        extract::discover_links(&fetch.final_url, &fetch.body, &crawler_cfg.link_selector)
            .unwrap_or_else(|error| {
                tracing::warn!(url = %fetch.final_url, %error, "link discovery failed");
                Vec::new()
            })
            .into_iter()
            .filter(|url| policy.validate_url(url).is_ok())
            .collect()
    } else {
        Vec::new()
    };

    let page = PageRecord {
        crawl_id: crawl_id.to_owned(),
        requested_url: fetch.requested_url.to_string(),
        final_url: fetch.final_url.to_string(),
        parent_url: target.parent.as_ref().map(ToString::to_string),
        depth: target.depth,
        fetched_at: Utc::now(),
        status: fetch.status,
        http_version: fetch.http_version,
        content_type: fetch.content_type,
        body_bytes: fetch.body.len(),
        elapsed_ms: fetch.elapsed_ms,
        redirect_chain: fetch
            .redirect_chain
            .iter()
            .map(ToString::to_string)
            .collect(),
        request_profile: profile_name.to_owned(),
        upstream_proxy: fetch.proxy,
        request_header_fingerprint: fetch.request_header_fingerprint,
        response_fingerprint: fetch.response_fingerprint,
        response_headers: fetch.headers,
        body_sha256,
        error: None,
    };
    storage.save_page(page, Some(fetch.body.clone())).await?;
    storage.save_records(&records).await?;

    Ok(ProcessResult::Fetched {
        links,
        body_bytes: fetch.body.len(),
        record_count: records.len(),
    })
}

async fn load_robots<S>(
    page_url: &Url,
    policy: &UrlPolicy,
    network: &NetworkEngine<S>,
    cache: &RobotsCache,
    limiter: &HostLimiter,
    min_delay_ms: u64,
    fail_closed: bool,
) -> RobotsRules
where
    S: Service<Request, Output = Response> + Clone + Send + Sync + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    let origin = origin_key(page_url);
    if let Some(rules) = cache.get(&origin).await {
        return rules;
    }

    let mut robots_url = page_url.clone();
    robots_url.set_path("/robots.txt");
    robots_url.set_query(None);
    robots_url.set_fragment(None);
    let rules = if policy.validate_url(&robots_url).is_err() {
        if fail_closed {
            RobotsRules::deny_all()
        } else {
            RobotsRules::default()
        }
    } else {
        match limiter
            .acquire(&robots_url, Duration::from_millis(min_delay_ms))
            .await
        {
            Ok(_permit) => match network.fetch(&robots_url).await {
                Ok(result) if (200..300).contains(&result.status) => {
                    RobotsRules::parse(&String::from_utf8_lossy(&result.body))
                }
                Ok(result) if result.status == 404 => RobotsRules::default(),
                Ok(result) => {
                    tracing::warn!(url = %robots_url, status = result.status, "robots.txt returned a non-success status");
                    if fail_closed {
                        RobotsRules::deny_all()
                    } else {
                        RobotsRules::default()
                    }
                }
                Err(error) => {
                    tracing::warn!(url = %robots_url, %error, fail_closed, "robots.txt fetch failed");
                    if fail_closed {
                        RobotsRules::deny_all()
                    } else {
                        RobotsRules::default()
                    }
                }
            },
            Err(error) => {
                tracing::warn!(url = %robots_url, %error, fail_closed, "robots.txt limiter failed");
                if fail_closed {
                    RobotsRules::deny_all()
                } else {
                    RobotsRules::default()
                }
            }
        }
    };
    cache.insert(origin, rules.clone()).await;
    rules
}

fn error_page(
    crawl_id: &str,
    target: &CrawlTarget,
    profile: &str,
    error: &anyhow::Error,
) -> PageRecord {
    PageRecord {
        crawl_id: crawl_id.to_owned(),
        requested_url: target.url.to_string(),
        final_url: target.url.to_string(),
        parent_url: target.parent.as_ref().map(ToString::to_string),
        depth: target.depth,
        fetched_at: Utc::now(),
        status: 0,
        http_version: String::new(),
        content_type: None,
        body_bytes: 0,
        elapsed_ms: 0,
        redirect_chain: Vec::new(),
        request_profile: profile.to_owned(),
        upstream_proxy: None,
        request_header_fingerprint: String::new(),
        response_fingerprint: String::new(),
        response_headers: Default::default(),
        body_sha256: fingerprint::sha256(&[]),
        error: Some(error.to_string()),
    }
}

#[derive(Debug)]
enum ProcessResult {
    Fetched {
        links: Vec<Url>,
        body_bytes: usize,
        record_count: usize,
    },
    Blocked(String),
}

#[derive(Debug)]
struct HostLimiter {
    hosts: DashMap<String, Arc<HostState>>,
    per_host_concurrency: usize,
}

impl HostLimiter {
    fn new(per_host_concurrency: usize) -> Self {
        Self {
            hosts: DashMap::new(),
            per_host_concurrency,
        }
    }

    async fn acquire(&self, url: &Url, delay: Duration) -> Result<OwnedSemaphorePermit> {
        let host = host_key(url)?;
        let state = self
            .hosts
            .entry(host)
            .or_insert_with(|| Arc::new(HostState::new(self.per_host_concurrency)))
            .clone();
        let permit = Arc::clone(&state.semaphore)
            .acquire_owned()
            .await
            .map_err(|_| anyhow!("host semaphore closed"))?;

        let mut next_allowed = state.next_allowed.lock().await;
        let now = Instant::now();
        if *next_allowed > now {
            sleep(*next_allowed - now).await;
        }
        *next_allowed = Instant::now() + delay;
        drop(next_allowed);
        Ok(permit)
    }
}

#[derive(Debug)]
struct HostState {
    semaphore: Arc<Semaphore>,
    next_allowed: Mutex<Instant>,
}

impl HostState {
    fn new(concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            next_allowed: Mutex::new(Instant::now()),
        }
    }
}

fn host_key(url: &Url) -> Result<String> {
    let host = url.host_str().context("URL has no host")?;
    let port = url.port_or_known_default().context("URL has no port")?;
    Ok(format!("{}://{host}:{port}", url.scheme()))
}

fn origin_key(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    let port = url.port_or_known_default().unwrap_or_default();
    format!("{}://{host}:{port}", url.scheme())
}

fn is_html(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("text/html") || value.contains("application/xhtml+xml")
    })
}

fn normalize_url(mut url: Url) -> Url {
    url.set_fragment(None);
    if (url.scheme() == "http" && url.port() == Some(80))
        || (url.scheme() == "https" && url.port() == Some(443))
    {
        let _ = url.set_port(None);
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;
    use rama::{http::Body, service::service_fn};
    use serde_json::json;

    #[tokio::test]
    async fn redirects_obey_destination_robots() -> Result<()> {
        for (cross_origin, robots_status, denied, respect_robots) in [
            (false, 200, true, true),
            (true, 200, true, true),
            (true, 503, false, true),
            (true, 200, false, true),
            (true, 200, true, false),
        ] {
            let directory = tempfile::tempdir()?;
            let source = "http://127.0.0.1:8001";
            let destination = if cross_origin {
                "http://127.0.0.1:8002"
            } else {
                source
            };
            let config: AppConfig = serde_json::from_value(json!({
                "crawler": {
                    "seeds": [format!("{source}/start")],
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_ports": [8001, 8002],
                    "allow_private_networks": true,
                    "respect_robots_txt": respect_robots,
                    "robots_fail_closed": true,
                    "min_delay_ms": 0,
                    "per_host_concurrency": 1,
                    "max_pages": 1,
                    "discover_links": false,
                    "retries": 2
                },
                "storage": {
                    "sqlite_path": directory.path().join("pages.sqlite3").display().to_string(),
                    "jsonl_path": directory.path().join("records.jsonl").display().to_string(),
                    "summary_path": directory.path().join("summary.json").display().to_string()
                }
            }))?;
            config.validate()?;
            let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let seen = Arc::clone(&requests);
            let client = service_fn(move |request: Request| {
                seen.lock().push(request.uri().to_string());
                async move {
                    let url = Url::parse(&request.uri().to_string())?;
                    let is_destination = url.port() == Some(8002) || !cross_origin;
                    let mut response = Response::builder();
                    let body = match url.path() {
                        "/robots.txt" if is_destination => {
                            response = response.status(robots_status);
                            if denied {
                                "User-agent: *\nDisallow: /private\n"
                            } else {
                                "User-agent: *\nAllow: /\n"
                            }
                        }
                        "/robots.txt" => "User-agent: *\nAllow: /\n",
                        "/start" => {
                            response = response.status(302).header("location", "/step");
                            ""
                        }
                        "/step" => {
                            response = response
                                .status(302)
                                .header("location", format!("{destination}/private"));
                            ""
                        }
                        "/private" => "private content",
                        other => return Err(anyhow!("unexpected fixture path: {other}").into()),
                    };
                    Ok::<_, BoxError>(response.body(Body::from(body))?)
                }
            });
            let database_path = config.storage.sqlite_path.clone();
            let summary =
                tokio::time::timeout(Duration::from_secs(5), run(config, client)).await??;
            let blocked = respect_robots && (denied || robots_status != 200);
            assert_eq!(summary.pages_blocked, usize::from(blocked));
            assert_eq!(summary.pages_fetched, usize::from(!blocked));
            assert_eq!(summary.pages_failed, 0);

            let mut expected = Vec::new();
            if respect_robots {
                expected.push(format!("{source}/robots.txt"));
            }
            expected.extend([format!("{source}/start"), format!("{source}/step")]);
            if respect_robots && cross_origin {
                let attempts = if robots_status == 503 { 3 } else { 1 };
                expected.extend(std::iter::repeat_n(
                    format!("{destination}/robots.txt"),
                    attempts,
                ));
            }
            if !blocked {
                expected.push(format!("{destination}/private"));
                let connection = rusqlite::Connection::open(database_path)?;
                let (requested, final_url, redirects): (String, String, String) = connection
                    .query_row(
                        "SELECT requested_url, final_url, redirect_chain_json FROM pages",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )?;
                assert_eq!(requested, format!("{source}/start"));
                assert_eq!(final_url, format!("{destination}/private"));
                assert_eq!(
                    serde_json::from_str::<Vec<String>>(&redirects)?,
                    vec![format!("{source}/step"), final_url]
                );
            }
            assert_eq!(*requests.lock(), expected);
        }
        Ok(())
    }
}
