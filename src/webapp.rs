use crate::{
    config::AppConfig,
    crawler, dashboard,
    model::{CrawlProgress, CrawlSummary},
    network,
};
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const STYLES_CSS: &str = include_str!("../web/styles.css");

#[derive(Clone)]
struct WebState {
    config_path: Arc<PathBuf>,
    runtime: Arc<RwLock<RuntimeState>>,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ScanPhase {
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
struct RuntimeState {
    phase: ScanPhase,
    message: String,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    progress: Option<CrawlProgress>,
    summary: Option<CrawlSummary>,
    dashboard_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
struct StatusResponse {
    phase: ScanPhase,
    message: String,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    progress: Option<CrawlProgress>,
    summary: Option<CrawlSummary>,
    dashboard_available: bool,
    dashboard_url: Option<&'static str>,
    config_path: String,
}

#[derive(Debug, Deserialize)]
struct ScanRequest {
    config: AppConfig,
    #[serde(default)]
    clean_output: bool,
}

#[derive(Debug, Serialize)]
struct ConfigActionResponse {
    ok: bool,
    message: String,
    config_path: String,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn conflict(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: error.to_string(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    fn not_found(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let payload = Json(json!({
            "ok": false,
            "error": self.message,
        }));
        (self.status, payload).into_response()
    }
}

pub async fn serve(
    config_path: PathBuf,
    bind: SocketAddr,
    allow_remote: bool,
    open_browser: bool,
) -> Result<()> {
    if !bind.ip().is_loopback() && !allow_remote {
        bail!(
            "refusing to expose the control panel on {bind}; use --allow-remote only in a trusted network"
        );
    }

    let config_display = config_path.display().to_string();
    let config = AppConfig::load(&config_path).await?;
    let initial_dashboard = dashboard_path_for(&config);
    let dashboard_exists = initial_dashboard.exists();
    let state = WebState {
        config_path: Arc::new(config_path),
        runtime: Arc::new(RwLock::new(RuntimeState {
            phase: ScanPhase::Idle,
            message: "Configuración cargada. El escáner está preparado.".to_owned(),
            started_at: None,
            finished_at: None,
            progress: None,
            summary: None,
            dashboard_path: dashboard_exists.then_some(initial_dashboard),
        })),
        task: Arc::new(Mutex::new(None)),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/styles.css", get(styles_css))
        .route("/api/config", get(get_config).put(save_config))
        .route("/api/validate", post(validate_config))
        .route("/api/scan", post(start_scan))
        .route("/api/scan/cancel", post(cancel_scan))
        .route("/api/status", get(get_status))
        .route("/dashboard", get(show_dashboard))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(state);

    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("cannot bind web control panel to {bind}"))?;
    let address = listener.local_addr().context("cannot read bound address")?;
    let url = format!("http://{address}/");
    println!("Panel de control: {url}");
    println!("Configuración: {config_display}");

    if open_browser {
        open_in_browser(&url);
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("web control panel stopped unexpectedly")?;
    Ok(())
}

async fn index() -> Response {
    static_asset("text/html; charset=utf-8", INDEX_HTML)
}

async fn app_js() -> Response {
    static_asset("text/javascript; charset=utf-8", APP_JS)
}

async fn styles_css() -> Response {
    static_asset("text/css; charset=utf-8", STYLES_CSS)
}

async fn get_config(State(state): State<WebState>) -> Result<Json<AppConfig>, ApiError> {
    AppConfig::load(state.config_path.as_ref())
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn validate_config(
    Json(config): Json<AppConfig>,
) -> Result<Json<ConfigActionResponse>, ApiError> {
    config.validate().map_err(ApiError::bad_request)?;
    Ok(Json(ConfigActionResponse {
        ok: true,
        message: "La configuración es válida.".to_owned(),
        config_path: String::new(),
    }))
}

async fn save_config(
    State(state): State<WebState>,
    Json(config): Json<AppConfig>,
) -> Result<Json<ConfigActionResponse>, ApiError> {
    let _task_guard = state.task.lock().await;
    ensure_not_running(&state)?;
    config
        .save(state.config_path.as_ref())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(ConfigActionResponse {
        ok: true,
        message: "Configuración validada y guardada.".to_owned(),
        config_path: state.config_path.display().to_string(),
    }))
}

async fn start_scan(
    State(state): State<WebState>,
    Json(request): Json<ScanRequest>,
) -> Result<(StatusCode, Json<StatusResponse>), ApiError> {
    let ScanRequest {
        config,
        clean_output,
    } = request;
    config.validate().map_err(ApiError::bad_request)?;
    let mut task_slot = state.task.lock().await;
    ensure_not_running(&state)?;
    config
        .save(state.config_path.as_ref())
        .await
        .map_err(ApiError::bad_request)?;
    if clean_output {
        clean_outputs(&config)
            .await
            .map_err(ApiError::bad_request)?;
    }

    let dashboard_path = dashboard_path_for(&config);
    {
        let mut runtime = state.runtime.write();
        runtime.phase = ScanPhase::Running;
        runtime.message = "Preparando cliente de red y políticas de rastreo…".to_owned();
        runtime.started_at = Some(Utc::now());
        runtime.finished_at = None;
        runtime.progress = None;
        runtime.summary = None;
        runtime.dashboard_path = None;
    }

    let state_for_task = state.clone();
    let handle = tokio::spawn(async move {
        let client = match network::build_client(&config.crawler) {
            Ok(client) => client,
            Err(error) => {
                fail_runtime(
                    &state_for_task,
                    format!("No se pudo crear el cliente de red: {error}"),
                );
                return;
            }
        };

        let progress_runtime = Arc::clone(&state_for_task.runtime);
        let result = crawler::run_with_progress(config.clone(), client, move |progress| {
            let completed = progress.pages_fetched + progress.pages_failed + progress.pages_blocked;
            let mut runtime = progress_runtime.write();
            if runtime.phase == ScanPhase::Running {
                runtime.message = format!(
                    "Procesadas {completed} de {} páginas programadas; {} registros extraídos.",
                    progress.pages_scheduled, progress.records_extracted
                );
                runtime.progress = Some(progress);
            }
        })
        .await;

        match result {
            Ok(summary) => {
                let dashboard_result = dashboard::generate(&config, &dashboard_path).await;
                let mut runtime = state_for_task.runtime.write();
                runtime.phase = ScanPhase::Completed;
                runtime.finished_at = Some(Utc::now());
                match dashboard_result {
                    Ok(_) => {
                        runtime.dashboard_path = Some(dashboard_path);
                        runtime.message = format!(
                            "Escaneo completado: {} páginas y {} registros.",
                            summary.pages_fetched, summary.records_extracted
                        );
                    }
                    Err(error) => {
                        runtime.message = format!(
                            "Escaneo completado, pero no se pudo generar el panel de resultados: {error}"
                        );
                    }
                }
                runtime.summary = Some(summary);
            }
            Err(error) => fail_runtime(&state_for_task, format!("El escaneo falló: {error}")),
        }
    });

    *task_slot = Some(handle);
    drop(task_slot);
    Ok((StatusCode::ACCEPTED, Json(status_response(&state))))
}

async fn cancel_scan(State(state): State<WebState>) -> Result<Json<StatusResponse>, ApiError> {
    if state.runtime.read().phase != ScanPhase::Running {
        return Err(ApiError::conflict("no hay ningún escaneo activo"));
    }
    let handle = state.task.lock().await.take();
    if let Some(handle) = handle {
        handle.abort();
        let _ = handle.await;
        let mut runtime = state.runtime.write();
        runtime.phase = ScanPhase::Cancelled;
        runtime.message = "Escaneo cancelado por el usuario.".to_owned();
        runtime.finished_at = Some(Utc::now());
        drop(runtime);
        return Ok(Json(status_response(&state)));
    }
    Err(ApiError::conflict("no hay ningún escaneo activo"))
}

async fn get_status(State(state): State<WebState>) -> Json<StatusResponse> {
    Json(status_response(&state))
}

async fn show_dashboard(State(state): State<WebState>) -> Result<Response, ApiError> {
    let path = state
        .runtime
        .read()
        .dashboard_path
        .clone()
        .ok_or_else(|| ApiError::not_found("todavía no hay un panel de resultados"))?;
    let html = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("cannot read dashboard {}", path.display()))
        .map_err(ApiError::internal)?;
    Ok(dashboard_asset(html))
}

fn ensure_not_running(state: &WebState) -> Result<(), ApiError> {
    if state.runtime.read().phase == ScanPhase::Running {
        return Err(ApiError::conflict(
            "ya existe un escaneo en ejecución; cancélalo o espera a que finalice",
        ));
    }
    Ok(())
}

fn fail_runtime(state: &WebState, message: String) {
    let mut runtime = state.runtime.write();
    runtime.phase = ScanPhase::Failed;
    runtime.message = message;
    runtime.finished_at = Some(Utc::now());
}

fn status_response(state: &WebState) -> StatusResponse {
    let runtime = state.runtime.read().clone();
    let dashboard_available = runtime
        .dashboard_path
        .as_ref()
        .is_some_and(|path| path.exists());
    StatusResponse {
        phase: runtime.phase,
        message: runtime.message,
        started_at: runtime.started_at,
        finished_at: runtime.finished_at,
        progress: runtime.progress,
        summary: runtime.summary,
        dashboard_available,
        dashboard_url: dashboard_available.then_some("/dashboard"),
        config_path: state.config_path.display().to_string(),
    }
}

async fn clean_outputs(config: &AppConfig) -> Result<()> {
    let files = [
        (&config.storage.sqlite_path, "SQLite"),
        (&config.storage.jsonl_path, "JSONL"),
        (&config.storage.summary_path, "resumen"),
    ];
    for (raw, label) in files {
        let path = safe_clean_path(raw, label)?;
        remove_file_if_present(&path, label).await?;
        if label == "SQLite" {
            let display = path.display().to_string();
            remove_file_if_present(Path::new(&format!("{display}-wal")), "SQLite WAL").await?;
            remove_file_if_present(Path::new(&format!("{display}-shm")), "SQLite SHM").await?;
        }
    }

    let bodies = safe_clean_path(&config.storage.bodies_dir, "directorio de cuerpos")?;
    match tokio::fs::remove_dir_all(&bodies).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot remove bodies directory {}", bodies.display()));
        }
    }

    let dashboard = dashboard_path_for(config);
    match tokio::fs::remove_file(&dashboard).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot remove dashboard {}", dashboard.display()));
        }
    }
    Ok(())
}

async fn remove_file_if_present(path: &Path, label: &str) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("cannot remove {label} output {}", path.display()))
        }
    }
}

fn safe_clean_path(raw: &str, label: &str) -> Result<PathBuf> {
    use std::path::Component;

    let path = Path::new(raw);
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("{label} must be a relative path below output/");
    }
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == std::ffi::OsStr::new("output") => {}
        _ => bail!("{label} can only be cleaned automatically when it is below output/"),
    }
    let mut has_child = false;
    for component in components {
        if !matches!(component, Component::Normal(_)) {
            bail!("{label} contains an unsafe path component");
        }
        has_child = true;
    }
    if !has_child {
        bail!("{label} cannot be the output directory itself");
    }
    Ok(path.to_path_buf())
}

fn dashboard_path_for(config: &AppConfig) -> PathBuf {
    let summary = Path::new(&config.storage.summary_path);
    summary
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("output"))
        .join("dashboard.html")
}

fn static_asset(content_type: &'static str, content: &'static str) -> Response {
    let mut response = Response::new(Body::from(content));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: https:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    response
}

fn dashboard_asset(content: String) -> Response {
    let mut response = Response::new(Body::from(content));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self' https: data:; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src https: data:; object-src 'none'; frame-ancestors 'none'",
        ),
    );
    response
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install Ctrl+C handler");
    }
}

fn open_in_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Err(error) = result {
        tracing::warn!(%error, %url, "could not open the browser automatically");
    }
}

#[cfg(test)]
mod tests {
    use super::safe_clean_path;

    #[test]
    fn clean_path_accepts_output_children() {
        assert!(safe_clean_path("output/results.jsonl", "test").is_ok());
    }

    #[test]
    fn clean_path_rejects_parent_traversal() {
        assert!(safe_clean_path("output/../secret", "test").is_err());
    }

    #[test]
    fn clean_path_rejects_absolute_paths() {
        assert!(safe_clean_path("/tmp/results.jsonl", "test").is_err());
    }
}
