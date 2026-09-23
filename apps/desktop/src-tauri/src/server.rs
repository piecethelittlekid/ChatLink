use crate::{certs, db, network};
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{ws::{Message as WsMessage, WebSocket, WebSocketUpgrade}, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{SecondsFormat, Utc};
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{path::BaseDirectory, Manager};
use tokio::{sync::{mpsc, Mutex, RwLock}, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tower_http::services::{ServeDir, ServeFile};
use ulid::Ulid;
use uuid::Uuid;

const HTTPS_PORT: u16 = 42100;
const SETUP_PORT: u16 = 42101;
const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const RATE_WINDOW: Duration = Duration::from_secs(10);
const RATE_LIMIT: usize = 30;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshot {
    pub status: String,
    pub interfaces: Vec<String>,
    pub selected_ip: Option<String>,
    pub https_url: Option<String>,
    pub setup_url: Option<String>,
    pub ca_fingerprint: String,
    pub session_code: String,
    pub iphone_status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMessage {
    pub id: String,
    pub sender: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceOption {
    pub ip: String,
    pub label: String,
}

pub struct Core {
    pub pool: SqlitePool,
    pub data_dir: PathBuf,
    pub web_root: PathBuf,
    pub root_certificate_der: Vec<u8>,
    pub status: RwLock<ServerSnapshot>,
    pub active_client: Mutex<Option<ActiveClient>>,
    pub auth_failures: Mutex<VecDeque<Instant>>,
    pub message_windows: Mutex<HashMap<Uuid, VecDeque<Instant>>>,
    pub recent_messages: Mutex<VecDeque<DesktopMessage>>,
}

#[derive(Clone)]
pub struct ActiveClient {
    pub id: Uuid,
    pub sender: mpsc::UnboundedSender<String>,
    pub cancel: CancellationToken,
}

pub struct ServerRuntime {
    pub ip: Ipv4Addr,
    pub tls_handle: axum_server::Handle<SocketAddr>,
    pub tls_task: JoinHandle<()>,
    pub setup_cancel: CancellationToken,
    pub setup_task: Option<JoinHandle<()>>,
}

#[derive(Deserialize)]
struct WireEnvelope {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    payload: Option<Value>,
}

pub fn create_core(
    pool: SqlitePool,
    data_dir: PathBuf,
    web_root: PathBuf,
    root_certificate_der: Vec<u8>,
    fingerprint: String,
    session_code: String,
    interfaces: Vec<Ipv4Addr>,
) -> Arc<Core> {
    Arc::new(Core {
        pool,
        data_dir,
        web_root,
        root_certificate_der,
        status: RwLock::new(ServerSnapshot {
            status: "starting".into(),
            interfaces: interfaces.into_iter().map(|ip| ip.to_string()).collect(),
            selected_ip: None,
            https_url: None,
            setup_url: None,
            ca_fingerprint: fingerprint,
            session_code,
            iphone_status: "waiting".into(),
            error: None,
        }),
        active_client: Mutex::new(None),
        auth_failures: Mutex::new(VecDeque::new()),
        message_windows: Mutex::new(HashMap::new()),
        recent_messages: Mutex::new(VecDeque::new()),
    })
}

pub async fn start(core: Arc<Core>, ip: Ipv4Addr) -> Result<ServerRuntime> {
    let tls = certs::issue_server_certificate(&core.data_dir, ip)?;
    let tls_config = axum_server::tls_rustls::RustlsConfig::from_der(
        vec![tls.certificate_der],
        tls.private_key_der,
    )
    .await
    .context("configure HTTPS listener")?;

    let https_addr = SocketAddr::from((ip, HTTPS_PORT));
    let std_listener = TcpListener::bind(https_addr)
        .with_context(|| format!("bind HTTPS server at {https_addr}; check if port {HTTPS_PORT} is already in use"))?;
    std_listener.set_nonblocking(true).context("configure HTTPS listener")?;
    let router = main_router(core.clone());
    let tls_handle = axum_server::Handle::new();
    let server = axum_server::from_tcp_rustls(std_listener, tls_config)
        .context("create HTTPS server")?
        .handle(tls_handle.clone());
    let tls_task = tokio::spawn(async move {
        if let Err(error) = server.serve(router.into_make_service()).await {
            tracing::error!(%error, "HTTPS server stopped");
        }
    });

    let setup_cancel = CancellationToken::new();
    let setup_addr = SocketAddr::from((ip, SETUP_PORT));
    let setup_task = match tokio::net::TcpListener::bind(setup_addr).await {
        Ok(listener) => {
            let setup_router = Router::new()
                .route("/", get(setup_page))
                .route("/chatlink-root.mobileconfig", get(root_profile))
                .with_state(core.clone());
            let cancel = setup_cancel.clone();
            Some(tokio::spawn(async move {
                if let Err(error) = axum::serve(listener, setup_router)
                    .with_graceful_shutdown(cancel.cancelled_owned())
                    .await
                {
                    tracing::warn!(%error, "certificate setup listener stopped");
                }
            }))
        }
        Err(error) => {
            tracing::warn!(%error, "certificate setup listener unavailable");
            None
        }
    };

    {
        let mut status = core.status.write().await;
        status.status = "online".into();
        status.selected_ip = Some(ip.to_string());
        status.https_url = Some(format!("https://{ip}:{HTTPS_PORT}/"));
        status.setup_url = setup_task
            .as_ref()
            .map(|_| format!("http://{ip}:{SETUP_PORT}/"));
        status.error = setup_task
            .is_none()
            .then(|| format!("Cổng tải chứng chỉ {SETUP_PORT} đang bận."));
    }
    tracing::info!(%ip, https_port = HTTPS_PORT, setup_port = SETUP_PORT, "ChatLink LAN server started");

    Ok(ServerRuntime { ip, tls_handle, tls_task, setup_cancel, setup_task })
}

pub async fn stop(runtime: ServerRuntime) {
    runtime.tls_handle.graceful_shutdown(Some(Duration::from_secs(2)));
    runtime.setup_cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), runtime.tls_task).await;
    if let Some(task) = runtime.setup_task {
        let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
    }
}

fn main_router(core: Arc<Core>) -> Router {
    let root = core.web_root.clone();
    let index = root.join("index.html");
    Router::new()
        .route("/health", get(health))
        .route("/ws", get(websocket_upgrade))
        .route("/chatlink-root.mobileconfig", get(root_profile))
        .fallback_service(ServeDir::new(root).not_found_service(ServeFile::new(index)))
        .with_state(core)
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "online", "service": "ChatLink" }))
}

async fn setup_page(State(core): State<Arc<Core>>) -> Html<String> {
    let fingerprint = core.status.read().await.ca_fingerprint.clone();
    Html(format!(r#"<!doctype html><html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="theme-color" content="#f7f8fa"><title>Cài chứng chỉ ChatLink</title><style>
      *{{box-sizing:border-box}}body{{margin:0;padding:30px 18px;background:#f7f8fa;color:#252b3a;font:16px -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}}main{{max-width:480px;margin:8vh auto;padding:30px;border:1px solid #e8eaf0;border-radius:20px;background:white;box-shadow:0 20px 60px #202a4010}}.mark{{display:grid;place-items:center;width:44px;height:44px;border-radius:14px;background:#52659b;color:white;font-weight:800}}h1{{margin:22px 0 10px;font-size:23px;letter-spacing:-.04em}}p{{color:#737b8d;font-size:14px;line-height:1.6}}.fingerprint{{display:block;margin:20px 0;padding:14px;border-radius:11px;background:#f5f6f9;color:#35436e;overflow-wrap:anywhere;font:12px/1.6 ui-monospace,monospace}}a{{display:block;margin-top:20px;padding:14px;border-radius:11px;background:#52659b;color:white;text-align:center;text-decoration:none;font-weight:700}}.hint{{margin-top:20px;font-size:12px;color:#8b91a0}}
      </style></head><body><main><div class="mark">C</div><h1>Cài chứng chỉ HTTPS</h1><p>Trước khi cài, hãy so sánh fingerprint này với mã hiển thị trong ChatLink trên máy tính.</p><code class="fingerprint">{fingerprint}</code><a href="/chatlink-root.mobileconfig">Tải profile ChatLink</a><p class="hint">Sau khi tải: mở Cài đặt iPhone để cài profile, rồi vào Cài đặt → Cài đặt chung → Giới thiệu → Cài đặt tin cậy chứng chỉ và bật tin cậy đầy đủ cho ChatLink.</p></main></body></html>"#))
}

async fn root_profile(State(core): State<Arc<Core>>) -> Response<Body> {
    let profile = certs::mobileconfig(&core.root_certificate_der);
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-apple-aspen-config"));
    headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=ChatLink-Root.mobileconfig"));
    (StatusCode::OK, headers, profile).into_response()
}

async fn websocket_upgrade(ws: WebSocketUpgrade, State(core): State<Arc<Core>>) -> impl IntoResponse {
    ws.max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, core))
}

async fn handle_socket(mut socket: WebSocket, core: Arc<Core>) {
    let auth_text = match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
        Ok(Some(Ok(WsMessage::Text(text)))) => text.to_string(),
        _ => {
            let _ = send_json(&mut socket, json!({ "type": "error", "payload": { "code": "AUTH_REQUIRED" } })).await;
            let _ = socket.close().await;
            return;
        }
    };

    let parsed = serde_json::from_str::<WireEnvelope>(&auth_text);
    let code = parsed.ok().filter(|message| message.kind == "auth")
        .and_then(|message| message.payload)
        .and_then(|payload| payload.get("code").and_then(Value::as_str).map(str::to_owned));
    let expected_code = core.status.read().await.session_code.clone();
    if code.as_deref() != Some(expected_code.as_str()) {
        let limited = record_auth_failure(&core).await;
        let _ = send_json(&mut socket, json!({
            "type": "error",
            "payload": { "code": if limited { "RATE_LIMITED" } else { "AUTH_FAILED" } }
        })).await;
        let _ = socket.close().await;
        return;
    }

    let session_id = Uuid::new_v4();
    let cancel = CancellationToken::new();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
    {
        let mut active = core.active_client.lock().await;
        if let Some(previous) = active.take() {
            previous.cancel.cancel();
        }
        *active = Some(ActiveClient { id: session_id, sender: outbound_tx.clone(), cancel: cancel.clone() });
    }
    core.status.write().await.iphone_status = "connected".into();
    tracing::info!("iPhone WebSocket authenticated");
    if send_json(&mut socket, json!({ "type": "auth_ok", "payload": { "deviceId": "iphone-main" } })).await.is_err() {
        clear_active(&core, session_id).await;
        return;
    }

    let pending_core = core.clone();
    let pending_sender = outbound_tx.clone();
    tokio::spawn(async move { send_pending_delivery(&pending_core, &pending_sender).await; });
    let mut ping = tokio::time::interval(Duration::from_secs(15));
    let mut last_pong = Instant::now();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = socket.close().await;
                break;
            }
            outgoing = outbound_rx.recv() => match outgoing {
                Some(text) => if socket.send(WsMessage::Text(text.into())).await.is_err() { break; },
                None => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(WsMessage::Text(text))) => {
                    if let Err(error) = process_client_message(text.as_str(), &core, session_id, &outbound_tx, &mut last_pong).await {
                        tracing::warn!(%error, "rejected WebSocket message");
                        let code = if error.to_string().contains("rate exceeded") { "MESSAGE_RATE_LIMITED" } else { "INVALID_MESSAGE" };
                        let _ = outbound_tx.send(json!({ "type": "error", "payload": { "code": code } }).to_string());
                    }
                }
                Some(Ok(WsMessage::Ping(bytes))) => { let _ = socket.send(WsMessage::Pong(bytes)).await; }
                Some(Ok(WsMessage::Pong(_))) => { last_pong = Instant::now(); }
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if last_pong.elapsed() > Duration::from_secs(45) { break; }
                let _ = outbound_tx.send(json!({ "type": "ping", "timestamp": timestamp() }).to_string());
            }
        }
    }
    clear_active(&core, session_id).await;
}

async fn record_auth_failure(core: &Core) -> bool {
    let mut failures = core.auth_failures.lock().await;
    let now = Instant::now();
    while failures.front().is_some_and(|time| now.duration_since(*time) > Duration::from_secs(60)) {
        failures.pop_front();
    }
    failures.push_back(now);
    failures.len() >= 5
}

async fn process_client_message(
    text: &str,
    core: &Arc<Core>,
    session_id: Uuid,
    outbound: &mpsc::UnboundedSender<String>,
    last_pong: &mut Instant,
) -> Result<()> {
    let message: WireEnvelope = serde_json::from_str(text).context("parse WebSocket message")?;
    let payload = message.payload.unwrap_or(Value::Null);
    match message.kind.as_str() {
        "ping" => {
            outbound.send(json!({ "type": "pong", "timestamp": timestamp() }).to_string())?;
        }
        "pong" => *last_pong = Instant::now(),
        "chat_message" => {
            let id = message.request_id.context("chat message missing requestId")?;
            anyhow::ensure!(Ulid::from_string(&id).is_ok(), "invalid ULID message id");
            let content = payload.get("content").and_then(Value::as_str).context("missing message content")?;
            anyhow::ensure!(!content.trim().is_empty(), "message content is empty");
            anyhow::ensure!(content.len() <= MAX_MESSAGE_BYTES, "message exceeds 8 KB");
            enforce_message_rate_limit(core, session_id).await?;

            let inserted = db::insert_message(&core.pool, &id, "iphone-main", content, "stored").await?;
            let created_at = timestamp();
            if inserted {
                push_recent(core, DesktopMessage {
                    id: id.clone(),
                    sender: "iphone".into(),
                    content: content.to_owned(),
                    status: "stored".into(),
                    created_at: created_at.clone(),
                }).await;
            }
            outbound.send(json!({
                "type": "message_ack",
                "requestId": id,
                "timestamp": created_at,
                "payload": { "messageId": id, "status": "stored" }
            }).to_string())?;
        }
        "delivered" => {
            if let Some(id) = payload.get("messageId").and_then(Value::as_str) {
                db::mark_delivered(&core.pool, id).await?;
                update_recent_status(core, id, "delivered").await;
            }
        }
        _ => anyhow::bail!("unsupported message type"),
    }
    Ok(())
}

async fn enforce_message_rate_limit(core: &Core, session_id: Uuid) -> Result<()> {
    let mut windows = core.message_windows.lock().await;
    let now = Instant::now();
    let samples = windows.entry(session_id).or_default();
    while samples.front().is_some_and(|time| now.duration_since(*time) > RATE_WINDOW) {
        samples.pop_front();
    }
    anyhow::ensure!(samples.len() < RATE_LIMIT, "message rate exceeded");
    samples.push_back(now);
    Ok(())
}

async fn send_pending_delivery(core: &Core, outbound: &mpsc::UnboundedSender<String>) {
    if let Ok(pending) = db::pending_delivery(&core.pool).await {
        for message in pending {
            let wire = json!({
                "type": "chat_message",
                "requestId": message.id,
                "timestamp": message.created_at,
                "payload": { "sender": "windows", "content": message.content }
            }).to_string();
            if outbound.send(wire).is_err() {
                break;
            }
        }
    }
}

async fn clear_active(core: &Core, session_id: Uuid) {
    let mut active = core.active_client.lock().await;
    if active.as_ref().is_some_and(|client| client.id == session_id) {
        active.take();
        core.status.write().await.iphone_status = "waiting".into();
        tracing::info!("iPhone WebSocket disconnected");
    }
    core.message_windows.lock().await.remove(&session_id);
}

async fn push_recent(core: &Core, message: DesktopMessage) {
    let mut messages = core.recent_messages.lock().await;
    messages.push_back(message);
    while messages.len() > 250 {
        messages.pop_front();
    }
}

async fn update_recent_status(core: &Core, id: &str, status: &str) {
    if let Some(message) = core.recent_messages.lock().await.iter_mut().find(|message| message.id == id) {
        message.status = status.into();
    }
}

async fn send_json(socket: &mut WebSocket, value: Value) -> Result<()> {
    socket.send(WsMessage::Text(value.to_string().into())).await.context("send WebSocket response")?;
    Ok(())
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub async fn send_from_desktop(core: &Arc<Core>, content: String) -> Result<DesktopMessage> {
    anyhow::ensure!(!content.trim().is_empty(), "Tin nhắn không được để trống.");
    anyhow::ensure!(content.len() <= MAX_MESSAGE_BYTES, "Tin nhắn vượt giới hạn 8 KB.");
    let id = Ulid::new().to_string();
    let created_at = timestamp();
    db::insert_message(&core.pool, &id, "windows-main", &content, "pending_delivery").await?;
    let message = DesktopMessage {
        id: id.clone(),
        sender: "windows".into(),
        content: content.clone(),
        status: "stored".into(),
        created_at: created_at.clone(),
    };
    push_recent(core, message.clone()).await;
    if let Some(active) = core.active_client.lock().await.clone() {
        let envelope = json!({
            "type": "chat_message",
            "requestId": id,
            "timestamp": created_at,
            "payload": { "sender": "windows", "content": content }
        }).to_string();
        let _ = active.sender.send(envelope);
    }
    Ok(message)
}

pub async fn mark_delivered_from_iphone(core: &Core, id: &str) -> Result<()> {
    db::mark_delivered(&core.pool, id).await?;
    update_recent_status(core, id, "delivered").await;
    Ok(())
}

pub async fn status(core: &Core) -> ServerSnapshot {
    core.status.read().await.clone()
}

pub async fn recent_messages(core: &Core) -> Vec<DesktopMessage> {
    core.recent_messages.lock().await.iter().cloned().collect()
}

pub async fn set_status_error(core: &Core, error: String) {
    let mut status = core.status.write().await;
    status.status = "error".into();
    status.error = Some(error);
}

pub fn available_interfaces() -> Vec<InterfaceOption> {
    network::lan_ipv4_addresses()
        .into_iter()
        .map(|ip| InterfaceOption { ip: ip.to_string(), label: format!("IPv4 — {ip}") })
        .collect()
}

pub async fn restart(core: Arc<Core>, runtime_slot: &Mutex<Option<ServerRuntime>>, ip: Ipv4Addr) -> Result<()> {
    let previous = runtime_slot.lock().await.take();
    if let Some(previous) = previous {
        stop(previous).await;
    }
    if let Some(active) = core.active_client.lock().await.take() {
        active.cancel.cancel();
    }
    core.status.write().await.iphone_status = "waiting".into();
    match start(core.clone(), ip).await {
        Ok(runtime) => {
            *runtime_slot.lock().await = Some(runtime);
            Ok(())
        }
        Err(error) => {
            set_status_error(&core, error.to_string()).await;
            Err(error)
        }
    }
}

pub fn web_root(app: &tauri::AppHandle) -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mobile-web/dist")
    } else {
        app.path()
            .resolve("mobile-web", BaseDirectory::Resource)
            .unwrap_or_else(|_| PathBuf::from("mobile-web"))
    }
}

pub fn data_directory(app: &tauri::AppHandle) -> Result<PathBuf> {
    app.path().app_data_dir().context("resolve ChatLink app data directory")
}
