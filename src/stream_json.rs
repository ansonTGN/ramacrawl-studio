use crate::{config::CrawlerConfig, policy::UrlPolicy, user_agent::RequestProfile};
use anyhow::{Context, Result, anyhow, bail};
use rama::{
    Service,
    error::BoxError,
    extensions::ExtensionsRef,
    http::{Body, BodyExtractExt, HeaderName, HeaderValue, Method, Request, Response, Version},
    json::path::JsonPath,
    net::address::ProxyAddress,
};
use serde_json::{Value, json};
use std::{str::FromStr, time::Duration};
use tokio::time::timeout;
use url::Url;

pub async fn capture<S>(
    client: &S,
    cfg: &CrawlerConfig,
    url: &Url,
    paths: &[String],
    max_capture_bytes: usize,
) -> Result<Vec<Value>>
where
    S: Service<Request, Output = Response> + Clone + Send + Sync + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    if paths.is_empty() {
        bail!("at least one JSONPath selector is required");
    }

    let policy = UrlPolicy::from_config(cfg)?;
    policy.validate_network_destination(url).await?;
    let profile = RequestProfile::from_config(&cfg.user_agent_profile);
    let mut request = Request::builder()
        .method(Method::GET)
        .uri(url.as_str())
        .version(Version::HTTP_11)
        .body(Body::empty())?;

    for (name, value) in profile.headers.iter().chain(cfg.custom_headers.iter()) {
        request.headers_mut().insert(
            HeaderName::try_from(name.as_str())?,
            HeaderValue::try_from(value.as_str())?,
        );
    }
    request.headers_mut().insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("application/json, application/*+json;q=0.9, */*;q=0.1"),
    );
    request.headers_mut().insert(
        HeaderName::from_static("accept-encoding"),
        HeaderValue::from_static("gzip, br, deflate"),
    );

    if let Some(proxy) = cfg.proxies.first() {
        let address = ProxyAddress::try_from(proxy.as_str())
            .map_err(|error| anyhow!("invalid upstream proxy configuration: {error}"))?;
        request.extensions().insert(address);
    }

    let response = timeout(Duration::from_secs(cfg.timeout_secs), client.serve(request))
        .await
        .context("JSON streaming request timed out")?
        .map_err(|error| {
            let error: BoxError = error.into();
            anyhow!(error.to_string())
        })?;
    if !response.status().is_success() {
        bail!(
            "JSON streaming endpoint returned HTTP {}",
            response.status()
        );
    }

    let selectors = paths
        .iter()
        .map(|path| JsonPath::from_str(path).with_context(|| format!("invalid JSONPath: {path}")))
        .collect::<Result<Vec<_>>>()?;
    let captures = timeout(
        Duration::from_secs(cfg.timeout_secs),
        response.try_capture_json(selectors, max_capture_bytes),
    )
    .await
    .context("JSON streaming capture timed out")?
    .map_err(|error| anyhow!(error.to_string()))?;

    captures
        .into_iter()
        .map(|capture| {
            let path = capture.path().to_string();
            let value: Value = capture
                .deserialize()
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(json!({"path": path, "value": value}))
        })
        .collect()
}
