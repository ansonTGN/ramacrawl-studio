use crate::{
    config::CrawlerConfig, fingerprint, model::FetchResult, policy::UrlPolicy,
    user_agent::RequestProfile,
};
use anyhow::{Context, Result, anyhow, bail};
use rama::{
    Layer, Service,
    error::BoxError,
    extensions::ExtensionsRef,
    http::{
        Body, BodyExtractExt, HeaderName, HeaderValue, Method, Request, Response, StatusCode,
        Version,
        body::CollectOptions,
        client::{EasyHttpWebClient, HttpPooledConnectorConfig},
        layer::{decompression::DecompressionLayer, map_response_body::MapResponseBodyLayer},
    },
    net::address::ProxyAddress,
    rt::Executor,
    tls::client::TlsClientConfig,
};
use rand::Rng;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::time::{sleep, timeout};
use url::Url;

#[cfg(all(feature = "portable-tls", feature = "boring-emulation"))]
compile_error!("enable exactly one TLS feature: portable-tls or boring-emulation");

#[cfg(not(any(feature = "portable-tls", feature = "boring-emulation")))]
compile_error!("enable one TLS feature: portable-tls or boring-emulation");

#[cfg(feature = "portable-tls")]
pub fn build_client(
    cfg: &CrawlerConfig,
) -> Result<impl Service<Request, Output = Response, Error = BoxError> + Clone + 'static> {
    let client = EasyHttpWebClient::connector_builder()
        .with_default_transport_connector()
        .with_default_dns_connector()
        .with_tls_proxy_support_using_rustls()
        .with_proxy_support()
        .with_tls_support_using_rustls_and_default_http_version(
            TlsClientConfig::default_http(),
            Version::HTTP_11,
        )
        .with_default_http_connector(Executor::default())
        .try_with_connection_pool(HttpPooledConnectorConfig {
            max_concurrent_streams: cfg.concurrency.max(1),
            ..Default::default()
        })
        .map_err(|error| anyhow!("failed to configure Rama connection pool: {error}"))?
        .build_client();

    Ok((
        MapResponseBodyLayer::new(Body::new),
        DecompressionLayer::new(),
    )
        .into_layer(client))
}

#[cfg(feature = "boring-emulation")]
pub fn build_client(
    cfg: &CrawlerConfig,
) -> Result<impl Service<Request, Output = Response, Error = BoxError> + Clone + 'static> {
    use rama::{
        tls::boring::client::EmulateTlsProfileLayer,
        ua::{
            layer::emulate::{
                UserAgentEmulateHttpConnectModifierLayer, UserAgentEmulateHttpRequestModifierLayer,
                UserAgentEmulateLayer,
            },
            profile::UserAgentDatabase,
        },
    };

    let database = Arc::new(
        UserAgentDatabase::try_embedded()
            .map_err(|error| anyhow!("failed to load embedded User-Agent database: {error}"))?,
    );
    let connector = EasyHttpWebClient::connector_builder()
        .with_default_transport_connector()
        .with_default_dns_connector()
        .with_tls_proxy_support_using_boringssl()
        .with_proxy_support()
        .with_tls_support_using_boringssl(TlsClientConfig::default_http())
        .with_custom_connector(UserAgentEmulateHttpConnectModifierLayer::default())
        .with_default_http_connector(Executor::default())
        .try_with_connection_pool(HttpPooledConnectorConfig {
            max_concurrent_streams: cfg.concurrency.max(1),
            ..Default::default()
        })
        .map_err(|error| anyhow!("failed to configure Rama connection pool: {error}"))?
        .build_client()
        .with_jit_layer(UserAgentEmulateHttpRequestModifierLayer::default());

    Ok((
        MapResponseBodyLayer::new(Body::new),
        DecompressionLayer::new(),
        UserAgentEmulateLayer::new(database)
            .with_try_auto_detect_user_agent(true)
            .with_is_optional(true),
        EmulateTlsProfileLayer::new(),
    )
        .into_layer(connector)
        .boxed())
}

#[derive(Debug)]
pub struct NetworkEngine<S> {
    client: S,
    cfg: Arc<CrawlerConfig>,
    policy: Arc<UrlPolicy>,
    profile: RequestProfile,
    proxy_cursor: AtomicUsize,
}

impl<S> NetworkEngine<S>
where
    S: Service<Request, Output = Response> + Clone + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    pub fn new(
        client: S,
        cfg: Arc<CrawlerConfig>,
        policy: Arc<UrlPolicy>,
        profile: RequestProfile,
    ) -> Self {
        Self {
            client,
            cfg,
            policy,
            profile,
            proxy_cursor: AtomicUsize::new(0),
        }
    }

    pub async fn fetch(&self, url: &Url) -> Result<FetchResult> {
        self.fetch_with_request_guard(url, |_| async { Ok(Some(())) })
            .await?
            .context("request blocked")
    }

    /// Checks every request, including redirects and retries. The returned guard
    /// is held through body collection; `None` blocks the fetch without retrying.
    pub async fn fetch_with_request_guard<F, Fut, G>(
        &self,
        url: &Url,
        before_request: F,
    ) -> Result<Option<FetchResult>>
    where
        F: Fn(Url) -> Fut,
        Fut: Future<Output = Result<Option<G>>>,
    {
        let proxy = self.next_proxy();
        let mut last_error = None;
        for attempt in 0..=self.cfg.retries {
            match self
                .fetch_attempt(url, proxy.as_deref(), &before_request)
                .await
            {
                Ok(None) => return Ok(None),
                Ok(Some(result)) if !is_retryable_status(result.status) => return Ok(Some(result)),
                Ok(Some(result)) if attempt == self.cfg.retries => return Ok(Some(result)),
                Ok(Some(result)) => {
                    tracing::warn!(
                        url = %url,
                        status = result.status,
                        attempt,
                        "retryable HTTP response"
                    );
                }
                Err(error) if attempt == self.cfg.retries => return Err(error),
                Err(error) => {
                    tracing::warn!(url = %url, attempt, %error, "request failed; retrying");
                    last_error = Some(error);
                }
            }
            sleep(retry_delay(attempt)).await;
        }
        Err(last_error.unwrap_or_else(|| anyhow!("request exhausted retry policy")))
    }

    async fn fetch_attempt<F, Fut, G>(
        &self,
        requested_url: &Url,
        proxy: Option<&str>,
        before_request: &F,
    ) -> Result<Option<FetchResult>>
    where
        F: Fn(Url) -> Fut,
        Fut: Future<Output = Result<Option<G>>>,
    {
        let started = Instant::now();
        let mut current = requested_url.clone();
        let mut redirects = Vec::new();

        for redirect_index in 0..=self.cfg.redirect_limit {
            self.policy.validate_network_destination(&current).await?;
            let Some(_guard) = before_request(current.clone()).await? else {
                return Ok(None);
            };
            let request = self.build_request(&current, proxy)?;
            let request_header_fingerprint = fingerprint::request_fingerprint(
                request.method(),
                request.version(),
                request.headers(),
            );

            let response = timeout(
                Duration::from_secs(self.cfg.timeout_secs),
                self.client.serve(request),
            )
            .await
            .with_context(|| format!("request timed out for {current}"))?
            .map_err(|error| {
                let error: BoxError = error.into();
                anyhow!(error.to_string())
            })
            .with_context(|| format!("Rama client failed for {current}"))?;

            let status = response.status();
            let version = response.version();
            let headers = response.headers().clone();
            let response_fingerprint =
                fingerprint::response_fingerprint(status.as_u16(), version, &headers);

            if status.is_redirection() {
                if redirect_index == self.cfg.redirect_limit {
                    bail!("redirect limit exceeded for {requested_url}");
                }
                let location = headers
                    .get("location")
                    .context("redirect response has no Location header")?
                    .to_str()
                    .context("redirect Location is not valid UTF-8")?;
                let next = current
                    .join(location)
                    .with_context(|| format!("invalid redirect target {location}"))?;
                self.policy.validate_network_destination(&next).await?;
                redirects.push(next.clone());
                current = next;
                continue;
            }

            let content_type = headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let options = CollectOptions::new()
                .with_max_size(self.cfg.max_body_bytes)
                .with_timeout(Duration::from_secs(self.cfg.timeout_secs));
            let body = response
                .try_into_string_with(options)
                .await
                .map_err(|error| anyhow!(error.to_string()))?
                .into_bytes();

            return Ok(Some(FetchResult {
                requested_url: requested_url.clone(),
                final_url: current,
                status: status.as_u16(),
                http_version: version_name(version),
                content_type,
                headers: fingerprint::headers_to_map(&headers),
                body,
                redirect_chain: redirects,
                elapsed_ms: started.elapsed().as_millis(),
                proxy: proxy.map(sanitized_proxy_label),
                request_header_fingerprint,
                response_fingerprint,
            }));
        }
        bail!("redirect state machine terminated unexpectedly")
    }

    fn build_request(&self, url: &Url, proxy: Option<&str>) -> Result<Request> {
        let mut request = Request::builder()
            .method(Method::GET)
            .uri(url.as_str())
            .version(Version::HTTP_11)
            .body(Body::empty())?;

        for (name, value) in self
            .profile
            .headers
            .iter()
            .chain(self.cfg.custom_headers.iter())
        {
            request.headers_mut().insert(
                HeaderName::try_from(name.as_str())?,
                HeaderValue::try_from(value.as_str())?,
            );
        }
        request.headers_mut().insert(
            HeaderName::from_static("accept-encoding"),
            HeaderValue::from_static("gzip, br, deflate"),
        );

        if let Some(proxy) = proxy {
            let address = ProxyAddress::try_from(proxy)
                .map_err(|error| anyhow!("invalid upstream proxy configuration: {error}"))?;
            request.extensions().insert(address);
        }
        Ok(request)
    }

    fn next_proxy(&self) -> Option<String> {
        if self.cfg.proxies.is_empty() {
            return None;
        }
        let index = self.proxy_cursor.fetch_add(1, Ordering::Relaxed);
        self.cfg
            .proxies
            .get(index % self.cfg.proxies.len())
            .cloned()
    }
}

fn retry_delay(attempt: usize) -> Duration {
    let exponent = u32::try_from(attempt.min(8)).unwrap_or(8);
    let base_ms = 250_u64.saturating_mul(2_u64.saturating_pow(exponent));
    let jitter = rand::rng().random_range(0..=250_u64);
    Duration::from_millis((base_ms + jitter).min(30_000))
}

fn is_retryable_status(status: u16) -> bool {
    matches!(
        StatusCode::from_u16(status),
        Ok(StatusCode::REQUEST_TIMEOUT)
            | Ok(StatusCode::TOO_MANY_REQUESTS)
            | Ok(StatusCode::INTERNAL_SERVER_ERROR)
            | Ok(StatusCode::BAD_GATEWAY)
            | Ok(StatusCode::SERVICE_UNAVAILABLE)
            | Ok(StatusCode::GATEWAY_TIMEOUT)
    )
}

fn version_name(version: Version) -> String {
    match version {
        Version::HTTP_09 => "HTTP/0.9",
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_11 => "HTTP/1.1",
        Version::HTTP_2 => "HTTP/2",
        Version::HTTP_3 => "HTTP/3",
    }
    .to_owned()
}

fn sanitized_proxy_label(proxy: &str) -> String {
    let Ok(mut url) = Url::parse(proxy) else {
        return "<configured-proxy>".to_owned();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    if url.path() != "/" {
        url.set_path("");
    }
    url.to_string()
}
