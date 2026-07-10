use crate::{
    config::{CrawlerConfig, WebSocketConfig},
    json_path,
    policy::UrlPolicy,
};
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use rama::{
    Service,
    error::BoxError,
    extensions::Extensions,
    futures::{SinkExt, StreamExt},
    http::{
        Request, Response,
        ws::{Message, handshake::client::HttpClientWebSocketExt},
    },
    net::address::ProxyAddress,
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::{fs, io::AsyncWriteExt, time::timeout};
use url::Url;

pub async fn run<S>(
    client: &S,
    crawler_cfg: &CrawlerConfig,
    ws_cfg: &WebSocketConfig,
) -> Result<usize>
where
    S: Service<Request, Output = Response> + Clone + Send + Sync + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
{
    validate_ws_destination(crawler_cfg, &ws_cfg.url).await?;

    let extensions = Extensions::default();
    if let Some(proxy) = crawler_cfg.proxies.first() {
        let address = ProxyAddress::try_from(proxy.as_str())
            .map_err(|error| anyhow!("invalid upstream proxy configuration: {error}"))?;
        extensions.insert(address);
    }

    let mut socket = timeout(
        Duration::from_secs(ws_cfg.timeout_secs),
        client.websocket(ws_cfg.url.as_str()).handshake(extensions),
    )
    .await
    .context("WebSocket handshake timed out")?
    .map_err(|error| anyhow!(error.to_string()))?;

    for text in &ws_cfg.send_text {
        socket.send(Message::text(text.clone())).await?;
    }

    if let Some(parent) = Path::new(&ws_cfg.output_path).parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut output = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ws_cfg.output_path)
        .await
        .with_context(|| format!("cannot open {}", ws_cfg.output_path))?;

    let mut count = 0usize;
    while count < ws_cfg.max_messages {
        let next = timeout(Duration::from_secs(ws_cfg.timeout_secs), socket.next()).await;
        let Some(message) = next.context("WebSocket receive timeout")? else {
            break;
        };
        let message = message?;
        let record = message_record(&message, &ws_cfg.json_paths)?;
        let mut line = serde_json::to_vec(&record)?;
        line.push(b'\n');
        output.write_all(&line).await?;
        count += 1;
    }
    output.flush().await?;
    socket.close(None).await?;
    Ok(count)
}

fn message_record(message: &Message, paths: &[String]) -> Result<Value> {
    if message.is_text() {
        let text = message.to_text()?;
        let json_value = serde_json::from_str::<Value>(text).ok();
        let selected = if let Some(value) = json_value.as_ref() {
            let mut values = Vec::new();
            for path in paths {
                for selected in json_path::select(value, path)? {
                    values.push(json!({"path": path, "value": selected}));
                }
            }
            values
        } else {
            Vec::new()
        };
        return Ok(json!({
            "received_at": Utc::now(),
            "kind": "text",
            "text": text,
            "json": json_value,
            "selected": selected
        }));
    }
    if message.is_binary() {
        let data = message.clone().into_data();
        return Ok(json!({
            "received_at": Utc::now(),
            "kind": "binary",
            "hex": hex::encode(&data),
            "bytes": data.len()
        }));
    }
    Ok(json!({
        "received_at": Utc::now(),
        "kind": "control",
        "debug": format!("{message:?}")
    }))
}

async fn validate_ws_destination(cfg: &CrawlerConfig, url: &Url) -> Result<()> {
    let mut http_url = url.clone();
    match url.scheme() {
        "ws" => http_url
            .set_scheme("http")
            .map_err(|_| anyhow!("invalid ws URL"))?,
        "wss" => http_url
            .set_scheme("https")
            .map_err(|_| anyhow!("invalid wss URL"))?,
        other => bail!("unsupported WebSocket scheme: {other}"),
    }
    UrlPolicy::from_config(cfg)?
        .validate_network_destination(&http_url)
        .await
}
