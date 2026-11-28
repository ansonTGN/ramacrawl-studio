mod config;
mod crawler;
mod dashboard;
mod extract;
mod fingerprint;
mod json_path;
mod model;
mod network;
mod policy;
mod robots;
mod storage;
mod stream_json;
mod user_agent;
mod webapp;
mod websocket;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use config::AppConfig;
use std::{net::SocketAddr, path::PathBuf};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;

#[derive(Debug, Parser)]
#[command(name = "rama-controlled-extractor", version, about)]
struct Cli {
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    log_filter: String,
    #[arg(long, default_value_t = false)]
    json_logs: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate {
        #[arg(short, long)]
        config: PathBuf,
    },
    Crawl {
        #[arg(short, long)]
        config: PathBuf,
    },
    Watch {
        #[arg(short, long, default_value = "config/example.yaml")]
        config: PathBuf,
        #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=31_536_000))]
        interval_secs: u64,
    },
    JsonStream {
        #[arg(short, long)]
        config: PathBuf,
        #[arg(long)]
        url: Url,
        #[arg(long = "path", required = true)]
        paths: Vec<String>,
        #[arg(long, default_value_t = 1_048_576)]
        max_capture_bytes: usize,
        #[arg(long, default_value = "output/json-stream.json")]
        output: PathBuf,
    },
    Websocket {
        #[arg(short, long)]
        config: PathBuf,
    },
    Dashboard {
        #[arg(short, long)]
        config: PathBuf,
        #[arg(short, long, default_value = "output/dashboard.html")]
        output: PathBuf,
    },
    Serve {
        #[arg(short, long, default_value = "config/example.yaml")]
        config: PathBuf,
        #[arg(long, default_value = "127.0.0.1:8090")]
        bind: SocketAddr,
        #[arg(long, default_value_t = false)]
        allow_remote: bool,
        #[arg(long, default_value_t = false)]
        no_open: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_filter, cli.json_logs)?;

    match cli.command {
        Command::Validate { config } => {
            let cfg = AppConfig::load(config).await?;
            println!(
                "Configuración válida: {} semillas, {} dominios permitidos",
                cfg.crawler.seeds.len(),
                cfg.crawler.allowed_domains.len()
            );
        }
        Command::Crawl { config } => {
            let cfg = AppConfig::load(config).await?;
            let client = network::build_client(&cfg.crawler)?;
            let summary = crawler::run(cfg, client).await?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::Watch {
            config,
            interval_secs,
        } => {
            let cfg = AppConfig::load(config).await?;
            if !cfg.crawler.incremental {
                bail!(
                    "watch requires crawler.incremental: true to preserve the monitoring baseline"
                );
            }
            let client = network::build_client(&cfg.crawler)?;
            let limiter =
                std::sync::Arc::new(crawler::HostLimiter::new(cfg.crawler.per_host_concurrency));
            let shutdown = tokio::signal::ctrl_c();
            tokio::pin!(shutdown);
            loop {
                tokio::select! {
                    result = &mut shutdown => { result.context("cannot receive shutdown signal")?; break; }
                    result = crawler::run_with_limiter(cfg.clone(), client.clone(), |_| {}, std::sync::Arc::clone(&limiter)) => {
                        let summary = result?;
                        println!("{}", serde_json::to_string(&summary)?);
                    }
                }
                tokio::select! {
                    result = &mut shutdown => { result.context("cannot receive shutdown signal")?; break; }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(interval_secs)) => {}
                }
            }
        }
        Command::JsonStream {
            config,
            url,
            paths,
            max_capture_bytes,
            output,
        } => {
            if max_capture_bytes == 0 {
                bail!("max_capture_bytes must be greater than zero");
            }
            let cfg = AppConfig::load(config).await?;
            let client = network::build_client(&cfg.crawler)?;
            let captures =
                stream_json::capture(&client, &cfg.crawler, &url, &paths, max_capture_bytes)
                    .await?;
            if let Some(parent) = output.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&output, serde_json::to_vec_pretty(&captures)?).await?;
            println!("Capturas JSON: {} -> {}", captures.len(), output.display());
        }
        Command::Dashboard { config, output } => {
            let cfg = AppConfig::load(config).await?;
            let build = dashboard::generate(&cfg, &output).await?;
            println!("{}", serde_json::to_string_pretty(&build)?);
            println!("Abrir en el navegador: xdg-open {}", output.display());
        }
        Command::Serve {
            config,
            bind,
            allow_remote,
            no_open,
        } => {
            webapp::serve(config, bind, allow_remote, !no_open).await?;
        }
        Command::Websocket { config } => {
            let cfg = AppConfig::load(config).await?;
            let ws_cfg = cfg
                .websocket
                .as_ref()
                .context("the configuration has no websocket section")?;
            if ws_cfg.max_messages == 0 {
                bail!("websocket.max_messages must be greater than zero");
            }
            let client = network::build_client(&cfg.crawler)?;
            let count = websocket::run(&client, &cfg.crawler, ws_cfg).await?;
            println!("Mensajes WebSocket guardados: {count}");
        }
    }
    Ok(())
}

fn init_tracing(filter: &str, json_logs: bool) -> Result<()> {
    let env_filter = EnvFilter::try_new(filter).context("invalid tracing filter")?;
    if json_logs {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .try_init()?;
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer())
            .try_init()?;
    }
    Ok(())
}
