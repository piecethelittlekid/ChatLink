use crate::{certs, db, network};
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{SecondsFormat, Utc};
use futures_util::SinkExt;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{path::BaseDirectory, Manager};
use tokio::{
    sync::{mpsc, Mutex, RwLock},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tower_http::services::{ServeDir, ServeFile};
use ulid::Ulid;
use uuid::Uuid;

const HTTPS_PORT: u16 = 42100;
const SETUP_PORT: u16 = 42101;
const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const RATE_WINDOW: Duration = Duration::from_secs(10);
const RATE_LIMIT: usize = 30;
const SYNC_PAGE_SIZE: i64 = 50;
const PAIRING_LIFETIME: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshot {
    pub status: String,
    pub interfaces: Vec<String>,
    pub selected_ip: Option<String>,
    pub https_url: Option<String>,
    pub setup_url: Option<String>,
    pub certificate_setup_required: bool,
    pub ca_fingerprint: String,
    pub pairing_code: Option<String>,
    pub pairing_expires_at: Option<String>,
    pub paired_device: Option<String>,
    pub iphone_status: String,
    pub error: Option<String>,
}

pub type DesktopMessage = db::StoredMessage;

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
    pub pairing: Mutex<PairingState>,
    pub auth_gate: Mutex<()>,
}

pub struct PairingState {
    code: String,
    expires_at: Instant,
    expires_at_text: String,
}

#[derive(Clone)]
pub struct ActiveClient {
    pub id: Uuid,
    pub sender: mpsc::Sender<String>,
    pub cancel: CancellationToken,
}

pub struct ServerRuntime {
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
    interfaces: Vec<Ipv4Addr>,
) -> Arc<Core> {
    let pairing = fresh_pairing();
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
            certificate_setup_required: true,
            ca_fingerprint: fingerprint,
            pairing_code: Some(pairing.code.clone()),
            pairing_expires_at: Some(pairing.expires_at_text.clone()),
            paired_device: None,
            iphone_status: "waiting".into(),
            error: None,
        }),
        active_client: Mutex::new(None),
        auth_failures: Mutex::new(VecDeque::new()),
        message_windows: Mutex::new(HashMap::new()),
        pairing: Mutex::new(pairing),
        auth_gate: Mutex::new(()),
    })
}

fn fresh_pairing() -> PairingState {
    let code = format!("{:06}", rand::rng().random_range(0..1_000_000_u32));
    PairingState {
        code,
        expires_at: Instant::now() + PAIRING_LIFETIME,
        expires_at_text: (Utc::now() + chrono::Duration::minutes(10))
            .to_rfc3339_opts(SecondsFormat::Millis, true),
    }
}

async fn refresh_pairing(core: &Core) {
    let mut pairing = core.pairing.lock().await;
    if Instant::now() >= pairing.expires_at {
        *pairing = fresh_pairing();
    }
    let mut status = core.status.write().await;
    status.pairing_code = Some(pairing.code.clone());
    status.pairing_expires_at = Some(pairing.expires_at_text.clone());
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn equal_hashes(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub async fn start(core: Arc<Core>, ip: Ipv4Addr) -> Result<ServerRuntime> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    for asset in ["index.html", "sw.js", "manifest.webmanifest"] {
        anyhow::ensure!(
            core.web_root.join(asset).is_file(),
            "mobile PWA asset {asset} is missing from {}",
            core.web_root.display()
        );
    }
    let setup_required = !db::certificate_setup_complete(&core.pool).await?;
    let tls = certs::issue_server_certificate(&core.data_dir, ip)?;
    let tls_config = axum_server::tls_rustls::RustlsConfig::from_der(
        vec![tls.certificate_der],
        tls.private_key_der,
    )
    .await
    .context("configure HTTPS listener")?;

    let https_addr = SocketAddr::from((ip, HTTPS_PORT));
    let std_listener = TcpListener::bind(https_addr).with_context(|| {
        format!("bind HTTPS server at {https_addr}; check if port {HTTPS_PORT} is already in use")
    })?;
    std_listener
        .set_nonblocking(true)
        .context("configure HTTPS listener")?;
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
    let setup_task = if setup_required {
        match tokio::net::TcpListener::bind(setup_addr).await {
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
        }
    } else {
        None
    };

    {
        let mut status = core.status.write().await;
        status.status = "online".into();
        status.selected_ip = Some(ip.to_string());
        status.https_url = Some(format!("https://{ip}:{HTTPS_PORT}/"));
        status.setup_url = setup_task
            .as_ref()
            .map(|_| format!("http://{ip}:{SETUP_PORT}/"));
        status.certificate_setup_required = setup_required;
        status.error = (setup_required && setup_task.is_none())
            .then(|| format!("Cổng tải chứng chỉ {SETUP_PORT} đang bận."));
    }
    tracing::info!(%ip, https_port = HTTPS_PORT, setup_port = SETUP_PORT, "ChatLink LAN server started");

    Ok(ServerRuntime {
        tls_handle,
        tls_task,
        setup_cancel,
        setup_task,
    })
}

pub async fn stop(mut runtime: ServerRuntime) {
    runtime
        .tls_handle
        .graceful_shutdown(Some(Duration::from_secs(2)));
    stop_setup_listener(&mut runtime).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), runtime.tls_task).await;
}

async fn stop_setup_listener(runtime: &mut ServerRuntime) {
    runtime.setup_cancel.cancel();
    if let Some(task) = runtime.setup_task.take() {
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
    Html(format!(
        r##"<!doctype html><html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="theme-color" content="#f7f8fa"><title>Cài chứng chỉ ChatLink</title><style>
      *{{box-sizing:border-box}}body{{margin:0;padding:30px 18px;background:#f7f8fa;color:#252b3a;font:16px -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}}main{{max-width:480px;margin:8vh auto;padding:30px;border:1px solid #e8eaf0;border-radius:20px;background:white;box-shadow:0 20px 60px #202a4010}}.mark{{display:grid;place-items:center;width:44px;height:44px;border-radius:14px;background:#52659b;color:white;font-weight:800}}h1{{margin:22px 0 10px;font-size:23px;letter-spacing:-.04em}}p{{color:#737b8d;font-size:14px;line-height:1.6}}.fingerprint{{display:block;margin:20px 0;padding:14px;border-radius:11px;background:#f5f6f9;color:#35436e;overflow-wrap:anywhere;font:12px/1.6 ui-monospace,monospace}}a{{display:block;margin-top:20px;padding:14px;border-radius:11px;background:#52659b;color:white;text-align:center;text-decoration:none;font-weight:700}}.hint{{margin-top:20px;font-size:12px;color:#8b91a0}}
      </style></head><body><main><div class="mark">C</div><h1>Cài chứng chỉ HTTPS</h1><p>Trước khi cài, hãy so sánh fingerprint này với mã hiển thị trong ChatLink trên máy tính.</p><code class="fingerprint">{fingerprint}</code><a href="/chatlink-root.mobileconfig">Tải profile ChatLink</a><p class="hint">Sau khi tải: mở Cài đặt iPhone để cài profile, rồi vào Cài đặt → Cài đặt chung → Giới thiệu → Cài đặt tin cậy chứng chỉ và bật tin cậy đầy đủ cho ChatLink.</p></main></body></html>"##
    ))
}

async fn root_profile(State(core): State<Arc<Core>>) -> Response<Body> {
    let profile = certs::mobileconfig(&core.root_certificate_der);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-apple-aspen-config"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=ChatLink-Root.mobileconfig"),
    );
    (StatusCode::OK, headers, profile).into_response()
}

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(core): State<Arc<Core>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let expected = core
        .status
        .read()
        .await
        .https_url
        .clone()
        .unwrap_or_default();
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let development_origin = cfg!(debug_assertions)
        && matches!(origin, "http://localhost:5173" | "http://127.0.0.1:5173");
    if origin != expected.trim_end_matches('/') && !development_origin {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(ws
        .max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, core)))
}

async fn handle_socket(mut socket: WebSocket, core: Arc<Core>) {
    let first_text = match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
        Ok(Some(Ok(WsMessage::Text(text)))) => text.to_string(),
        _ => {
            reject_socket(&mut socket, "AUTH_REQUIRED").await;
            return;
        }
    };
    let mut first = match serde_json::from_str::<WireEnvelope>(&first_text) {
        Ok(message) => message,
        Err(_) => {
            reject_socket(&mut socket, "AUTH_REQUIRED").await;
            return;
        }
    };
    if first.kind == "pairing_request" {
        let pairing_guard = core.auth_gate.lock().await;
        match db::paired_device(&core.pool).await {
            Ok(Some(_)) => {
                reject_socket(&mut socket, "ALREADY_PAIRED").await;
                return;
            }
            Err(error) => {
                tracing::error!(%error, "pairing database failure");
                reject_socket(&mut socket, "DATABASE_ERROR").await;
                return;
            }
            Ok(None) => {}
        }
        let submitted = first
            .payload
            .as_ref()
            .and_then(|payload| payload.get("code"))
            .and_then(Value::as_str);
        let valid = {
            let pairing = core.pairing.lock().await;
            Instant::now() < pairing.expires_at
                && submitted.is_some_and(|code| code == pairing.code)
        };
        match record_auth_attempt(&core, valid).await {
            AuthDecision::Accepted => {}
            AuthDecision::Rejected => {
                reject_socket(&mut socket, "PAIRING_CODE_INVALID").await;
                return;
            }
            AuthDecision::RateLimited => {
                reject_socket(&mut socket, "RATE_LIMITED").await;
                return;
            }
        }
        let device_id = Uuid::new_v4().to_string();
        let mut token_bytes = [0_u8; 32];
        rand::rng().fill_bytes(&mut token_bytes);
        let token = URL_SAFE_NO_PAD.encode(token_bytes);
        match db::pair_device(&core.pool, &device_id, "iPhone", &token_hash(&token)).await {
            Ok(true) => {}
            Ok(false) => {
                reject_socket(&mut socket, "ALREADY_PAIRED").await;
                return;
            }
            Err(error) => {
                tracing::error!(%error, "pairing database failure");
                reject_socket(&mut socket, "DATABASE_ERROR").await;
                return;
            }
        }
        {
            let mut status = core.status.write().await;
            status.paired_device = Some("iPhone".into());
            status.pairing_code = None;
            status.pairing_expires_at = None;
        }
        drop(pairing_guard);
        if send_json(
            &mut socket,
            json!({ "type": "pairing_ok", "payload": { "deviceId": device_id, "token": token } }),
        )
        .await
        .is_err()
        {
            return;
        }
        first = match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
            Ok(Some(Ok(WsMessage::Text(text)))) => {
                match serde_json::from_str::<WireEnvelope>(&text) {
                    Ok(message) => message,
                    Err(_) => {
                        reject_socket(&mut socket, "AUTH_REQUIRED").await;
                        return;
                    }
                }
            }
            _ => {
                reject_socket(&mut socket, "AUTH_REQUIRED").await;
                return;
            }
        };
    }

    if first.kind != "auth" {
        reject_socket(&mut socket, "AUTH_REQUIRED").await;
        return;
    }

    let auth_guard = core.auth_gate.lock().await;
    let supplied = first.payload.as_ref();
    let supplied_id = supplied
        .and_then(|payload| payload.get("deviceId"))
        .and_then(Value::as_str);
    let supplied_token = supplied
        .and_then(|payload| payload.get("token"))
        .and_then(Value::as_str);
    let device = match db::paired_device(&core.pool).await {
        Ok(device) => device,
        Err(error) => {
            tracing::error!(%error, "authentication database failure");
            reject_socket(&mut socket, "DATABASE_ERROR").await;
            return;
        }
    };
    let valid = first.kind == "auth"
        && device.as_ref().is_some_and(|device| {
            supplied_id == Some(device.id.as_str())
                && supplied_token.is_some_and(|token| {
                    token.len() == 43 && equal_hashes(&token_hash(token), &device.token_hash)
                })
        });
    match record_auth_attempt(&core, valid).await {
        AuthDecision::Accepted => {}
        decision @ (AuthDecision::Rejected | AuthDecision::RateLimited) => {
            let error_code = if matches!(decision, AuthDecision::RateLimited) {
                "RATE_LIMITED"
            } else {
                "AUTH_FAILED"
            };
            reject_socket(&mut socket, error_code).await;
            return;
        }
    }

    if let Some(device) = device {
        if let Err(error) = db::touch_device(&core.pool, &device.id).await {
            tracing::warn!(%error, "could not update paired device last seen");
        }
    }

    let session_id = Uuid::new_v4();
    let cancel = CancellationToken::new();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(256);
    {
        let mut active = core.active_client.lock().await;
        if let Some(previous) = active.take() {
            previous.cancel.cancel();
        }
        *active = Some(ActiveClient {
            id: session_id,
            sender: outbound_tx.clone(),
            cancel: cancel.clone(),
        });
    }
    drop(auth_guard);
    core.status.write().await.iphone_status = "connected".into();
    tracing::info!("iPhone WebSocket authenticated");
    if send_json(
        &mut socket,
        json!({ "type": "auth_ok", "payload": { "deviceId": supplied_id } }),
    )
    .await
    .is_err()
    {
        clear_active(&core, session_id).await;
        return;
    }

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
                    if let Err(error) = process_client_message(text.as_str(), &core, &outbound_tx, &mut last_pong).await {
                        tracing::warn!(%error, "rejected WebSocket message");
                        if error.downcast_ref::<mpsc::error::TrySendError<String>>().is_some() { break; }
                        let detail = error.to_string();
                        let code = if detail.contains("rate exceeded") { "MESSAGE_RATE_LIMITED" }
                            else if detail.contains("8 KB") { "MESSAGE_TOO_LARGE" }
                            else if detail.contains("store message") || detail.contains("load messages") || detail.contains("mark Windows message") { "DATABASE_ERROR" }
                            else { "INVALID_MESSAGE" };
                        let _ = outbound_tx.try_send(json!({ "type": "error", "payload": { "code": code } }).to_string());
                    }
                }
                Some(Ok(WsMessage::Ping(bytes))) => { let _ = socket.send(WsMessage::Pong(bytes)).await; }
                Some(Ok(WsMessage::Pong(_))) => { last_pong = Instant::now(); }
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            _ = ping.tick() => {
                if last_pong.elapsed() > Duration::from_secs(45) { break; }
                if outbound_tx.try_send(json!({ "type": "ping", "timestamp": timestamp() }).to_string()).is_err() { break; }
            }
        }
    }
    clear_active(&core, session_id).await;
}

async fn reject_socket(socket: &mut WebSocket, code: &str) {
    let _ = send_json(
        socket,
        json!({ "type": "error", "payload": { "code": code } }),
    )
    .await;
    let _ = socket.close().await;
}

enum AuthDecision {
    Accepted,
    Rejected,
    RateLimited,
}

async fn record_auth_attempt(core: &Core, succeeded: bool) -> AuthDecision {
    let mut failures = core.auth_failures.lock().await;
    let now = Instant::now();
    while failures
        .front()
        .is_some_and(|time| now.duration_since(*time) > Duration::from_secs(60))
    {
        failures.pop_front();
    }
    if failures.len() >= 5 {
        return AuthDecision::RateLimited;
    }
    if succeeded {
        failures.clear();
        return AuthDecision::Accepted;
    }
    failures.push_back(now);
    if failures.len() >= 5 {
        AuthDecision::RateLimited
    } else {
        AuthDecision::Rejected
    }
}

async fn process_client_message(
    text: &str,
    core: &Arc<Core>,
    outbound: &mpsc::Sender<String>,
    last_pong: &mut Instant,
) -> Result<()> {
    let message: WireEnvelope = serde_json::from_str(text).context("parse WebSocket message")?;
    let payload = message.payload.unwrap_or(Value::Null);
    match message.kind.as_str() {
        "ping" => {
            outbound.try_send(json!({ "type": "pong", "timestamp": timestamp() }).to_string())?;
        }
        "pong" => *last_pong = Instant::now(),
        "sync_request" => {
            let after_seq = payload
                .get("afterSeq")
                .and_then(Value::as_i64)
                .context("missing sync cursor")?;
            anyhow::ensure!(after_seq >= 0, "invalid sync cursor");
            let mut messages =
                db::messages_after(&core.pool, after_seq, SYNC_PAGE_SIZE + 1).await?;
            let has_more = messages.len() as i64 > SYNC_PAGE_SIZE;
            messages.truncate(SYNC_PAGE_SIZE as usize);
            let next_seq = messages.last().map_or(after_seq, |message| message.seq);
            outbound.try_send(
                json!({
                    "type": "sync_batch",
                    "payload": { "messages": messages, "nextSeq": next_seq, "hasMore": has_more }
                })
                .to_string(),
            )?;
        }
        "chat_message" => {
            let id = message
                .request_id
                .context("chat message missing requestId")?;
            anyhow::ensure!(Ulid::from_string(&id).is_ok(), "invalid ULID message id");
            let content = payload
                .get("content")
                .and_then(Value::as_str)
                .context("missing message content")?;
            anyhow::ensure!(!content.trim().is_empty(), "message content is empty");
            anyhow::ensure!(content.len() <= MAX_MESSAGE_BYTES, "message exceeds 8 KB");
            enforce_message_rate_limit(core, Uuid::from_u128(1)).await?;

            let stored =
                db::insert_message(&core.pool, &id, "iphone-main", content, "stored").await?;
            if stored.inserted {
                tracing::debug!(message_id = %id, "iPhone message stored");
            }
            outbound.try_send(json!({
                "type": "message_ack",
                "requestId": id,
                "timestamp": stored.message.created_at,
                "payload": { "messageId": id, "status": "stored", "seq": stored.message.seq, "createdAt": stored.message.created_at }
            }).to_string())?;
        }
        "delivered" => {
            if let Some(id) = payload.get("messageId").and_then(Value::as_str) {
                db::mark_delivered(&core.pool, id).await?;
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
    while samples
        .front()
        .is_some_and(|time| now.duration_since(*time) > RATE_WINDOW)
    {
        samples.pop_front();
    }
    anyhow::ensure!(samples.len() < RATE_LIMIT, "message rate exceeded");
    samples.push_back(now);
    Ok(())
}

async fn clear_active(core: &Core, session_id: Uuid) {
    let mut active = core.active_client.lock().await;
    if active
        .as_ref()
        .is_some_and(|client| client.id == session_id)
    {
        active.take();
        core.status.write().await.iphone_status = "waiting".into();
        tracing::info!("iPhone WebSocket disconnected");
    }
}

async fn send_json(socket: &mut WebSocket, value: Value) -> Result<()> {
    socket
        .send(WsMessage::Text(value.to_string().into()))
        .await
        .context("send WebSocket response")?;
    Ok(())
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub async fn send_from_desktop(core: &Arc<Core>, content: String) -> Result<DesktopMessage> {
    anyhow::ensure!(!content.trim().is_empty(), "Tin nhắn không được để trống.");
    anyhow::ensure!(
        content.len() <= MAX_MESSAGE_BYTES,
        "Tin nhắn vượt giới hạn 8 KB."
    );
    enforce_message_rate_limit(core, Uuid::nil()).await?;
    let id = Ulid::new().to_string();
    let message = db::insert_message(
        &core.pool,
        &id,
        "windows-main",
        &content,
        "pending_delivery",
    )
    .await?
    .message;
    if let Some(active) = core.active_client.lock().await.clone() {
        let envelope = json!({
            "type": "chat_message",
            "requestId": id,
            "timestamp": message.created_at,
            "payload": { "sender": "windows", "content": content, "seq": message.seq }
        })
        .to_string();
        if active.sender.try_send(envelope).is_err() {
            active.cancel.cancel();
        }
    }
    Ok(message)
}

pub async fn status(core: &Core) -> ServerSnapshot {
    match db::paired_device(&core.pool).await {
        Ok(Some(device)) => {
            let mut status = core.status.write().await;
            status.paired_device = Some(device.name);
            status.pairing_code = None;
            status.pairing_expires_at = None;
        }
        Ok(None) => {
            core.status.write().await.paired_device = None;
            refresh_pairing(core).await;
        }
        Err(error) => {
            let mut status = core.status.write().await;
            status.status = "error".into();
            status.error = Some(format!(
                "Không đọc được trạng thái ghép đôi từ SQLite: {error}"
            ));
        }
    }
    core.status.read().await.clone()
}

pub async fn recent_messages(core: &Core, before_seq: Option<i64>) -> Result<Vec<DesktopMessage>> {
    db::recent_messages(&core.pool, before_seq, 100).await
}

pub async fn unpair(core: &Core) -> Result<ServerSnapshot> {
    let _auth_guard = core.auth_gate.lock().await;
    db::unpair_device(&core.pool).await?;
    if let Some(active) = core.active_client.lock().await.take() {
        active.cancel.cancel();
    }
    core.status.write().await.iphone_status = "waiting".into();
    *core.pairing.lock().await = fresh_pairing();
    Ok(status(core).await)
}

pub async fn set_status_error(core: &Core, error: String) {
    let mut status = core.status.write().await;
    status.status = "error".into();
    status.error = Some(error);
}

pub fn available_interfaces() -> Vec<InterfaceOption> {
    network::lan_interfaces()
        .into_iter()
        .map(|interface| InterfaceOption {
            ip: interface.ip.to_string(),
            label: format!("{} — {}", interface.name, interface.ip),
        })
        .collect()
}

pub async fn restart(
    core: Arc<Core>,
    runtime_slot: &Mutex<Option<ServerRuntime>>,
    ip: Ipv4Addr,
) -> Result<()> {
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

pub async fn confirm_certificate_setup(
    core: &Arc<Core>,
    runtime_slot: &Mutex<Option<ServerRuntime>>,
) -> Result<ServerSnapshot> {
    db::set_certificate_setup_complete(&core.pool).await?;
    if let Some(runtime) = runtime_slot.lock().await.as_mut() {
        stop_setup_listener(runtime).await;
    }
    {
        let mut status = core.status.write().await;
        status.certificate_setup_required = false;
        status.setup_url = None;
        if status.status == "online" {
            status.error = None;
        }
    }
    Ok(status(core).await)
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
    app.path()
        .app_data_dir()
        .context("resolve ChatLink app data directory")
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{client::IntoClientRequest, Message as ClientMessage},
        MaybeTlsStream, WebSocketStream,
    };

    type TestSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

    async fn client_send(socket: &mut TestSocket, value: Value) -> Result<()> {
        socket
            .send(ClientMessage::Text(value.to_string().into()))
            .await?;
        Ok(())
    }

    async fn client_receive(socket: &mut TestSocket) -> Result<Value> {
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(3), socket.next())
                .await
                .context("WebSocket response timed out")?
                .context("WebSocket closed before response")??;
            match frame {
                ClientMessage::Text(text) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["type"] == "ping" {
                        client_send(socket, json!({"type":"pong","payload":{}})).await?;
                        continue;
                    }
                    return Ok(value);
                }
                ClientMessage::Ping(bytes) => {
                    socket.send(ClientMessage::Pong(bytes)).await?;
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn ack_is_queued_only_after_sqlite_insert() -> Result<()> {
        let pool = db::open_memory().await?;
        let core = create_core(
            pool.clone(),
            PathBuf::new(),
            PathBuf::new(),
            Vec::new(),
            String::new(),
            Vec::new(),
        );
        let (sender, mut receiver) = mpsc::channel(2);
        let mut last_pong = Instant::now();
        let id = Ulid::new().to_string();
        let wire =
            json!({ "type": "chat_message", "requestId": id, "payload": { "content": "hello" } })
                .to_string();
        process_client_message(&wire, &core, &sender, &mut last_pong).await?;
        let ack: Value = serde_json::from_str(&receiver.recv().await.context("missing ack")?)?;
        assert_eq!(ack["type"], "message_ack");
        assert_eq!(ack["payload"]["messageId"], id);
        assert_eq!(db::recent_messages(&pool, None, 50).await?.len(), 1);
        process_client_message(&wire, &core, &sender, &mut last_pong).await?;
        assert_eq!(db::recent_messages(&pool, None, 50).await?.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn bad_access_codes_are_limited() -> Result<()> {
        let pool = db::open_memory().await?;
        let core = create_core(
            pool,
            PathBuf::new(),
            PathBuf::new(),
            Vec::new(),
            String::new(),
            Vec::new(),
        );
        for _ in 0..4 {
            assert!(matches!(
                record_auth_attempt(&core, false).await,
                AuthDecision::Rejected
            ));
        }
        assert!(matches!(
            record_auth_attempt(&core, false).await,
            AuthDecision::RateLimited
        ));
        assert!(matches!(
            record_auth_attempt(&core, true).await,
            AuthDecision::RateLimited
        ));
        assert!(equal_hashes(
            &token_hash("valid token"),
            &token_hash("valid token")
        ));
        assert!(!equal_hashes(
            &token_hash("valid token"),
            &token_hash("wrong token")
        ));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_large_messages_and_limits_send_rate() -> Result<()> {
        let pool = db::open_memory().await?;
        let core = create_core(
            pool.clone(),
            PathBuf::new(),
            PathBuf::new(),
            Vec::new(),
            String::new(),
            Vec::new(),
        );
        let (sender, mut receiver) = mpsc::channel(64);
        let mut last_pong = Instant::now();
        let oversized = json!({
            "type":"chat_message","requestId":Ulid::new().to_string(),
            "payload":{"content":"a".repeat(MAX_MESSAGE_BYTES + 1)}
        })
        .to_string();
        assert!(
            process_client_message(&oversized, &core, &sender, &mut last_pong)
                .await
                .is_err()
        );
        assert!(db::recent_messages(&pool, None, 50).await?.is_empty());
        for _ in 0..RATE_LIMIT {
            let wire = json!({
                "type":"chat_message","requestId":Ulid::new().to_string(),
                "payload":{"content":"ok"}
            })
            .to_string();
            process_client_message(&wire, &core, &sender, &mut last_pong).await?;
            assert_eq!(
                serde_json::from_str::<Value>(&receiver.recv().await.context("missing ack")?)?
                    ["type"],
                "message_ack"
            );
        }
        let blocked = json!({
            "type":"chat_message","requestId":Ulid::new().to_string(),
            "payload":{"content":"over limit"}
        })
        .to_string();
        assert!(
            process_client_message(&blocked, &core, &sender, &mut last_pong)
                .await
                .is_err()
        );
        assert_eq!(
            db::recent_messages(&pool, None, 50).await?.len(),
            RATE_LIMIT
        );
        Ok(())
    }

    #[tokio::test]
    async fn sync_pages_advance_without_skipping_messages() -> Result<()> {
        let pool = db::open_memory().await?;
        let core = create_core(
            pool.clone(),
            PathBuf::new(),
            PathBuf::new(),
            Vec::new(),
            String::new(),
            Vec::new(),
        );
        for index in 0..(SYNC_PAGE_SIZE + 3) {
            db::insert_message(
                &pool,
                &Ulid::new().to_string(),
                "iphone-main",
                &format!("{index}"),
                "stored",
            )
            .await?;
        }
        let (sender, mut receiver) = mpsc::channel(2);
        let mut last_pong = Instant::now();
        process_client_message(
            "{\"type\":\"sync_request\",\"payload\":{\"afterSeq\":0}}",
            &core,
            &sender,
            &mut last_pong,
        )
        .await?;
        let first: Value =
            serde_json::from_str(&receiver.recv().await.context("missing first page")?)?;
        assert_eq!(
            first["payload"]["messages"].as_array().unwrap().len(),
            SYNC_PAGE_SIZE as usize
        );
        assert_eq!(first["payload"]["hasMore"], true);
        let cursor = first["payload"]["nextSeq"]
            .as_i64()
            .context("missing cursor")?;
        process_client_message(
            &json!({"type":"sync_request","payload":{"afterSeq":cursor}}).to_string(),
            &core,
            &sender,
            &mut last_pong,
        )
        .await?;
        let second: Value =
            serde_json::from_str(&receiver.recv().await.context("missing last page")?)?;
        assert_eq!(second["payload"]["messages"].as_array().unwrap().len(), 3);
        assert_eq!(second["payload"]["hasMore"], false);
        assert!(second["payload"]["nextSeq"].as_i64().unwrap() > cursor);
        Ok(())
    }

    #[tokio::test]
    async fn websocket_pair_chat_sync_and_unpair() -> Result<()> {
        let pool = db::open_memory().await?;
        let core = create_core(
            pool.clone(),
            PathBuf::new(),
            PathBuf::new(),
            Vec::new(),
            String::new(),
            Vec::new(),
        );
        core.status.write().await.https_url = Some("https://127.0.0.1:42100/".into());
        let code = core.pairing.lock().await.code.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let router = main_router(core.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        let url = format!("ws://{address}/ws");

        let mut wrong_origin = url.as_str().into_client_request()?;
        wrong_origin
            .headers_mut()
            .insert(header::ORIGIN, "https://elsewhere.test".parse()?);
        assert!(connect_async(wrong_origin).await.is_err());

        let mut request = url.as_str().into_client_request()?;
        request
            .headers_mut()
            .insert(header::ORIGIN, "https://127.0.0.1:42100".parse()?);
        let (mut socket, _) = connect_async(request).await?;
        client_send(
            &mut socket,
            json!({"type":"pairing_request","payload":{"code":code}}),
        )
        .await?;
        let pairing = client_receive(&mut socket).await?;
        assert_eq!(pairing["type"], "pairing_ok");
        let device_id = pairing["payload"]["deviceId"]
            .as_str()
            .context("missing device ID")?;
        let token = pairing["payload"]["token"]
            .as_str()
            .context("missing token")?;
        assert_eq!(token.len(), 43);
        assert_ne!(db::paired_device(&pool).await?.unwrap().token_hash, token);
        client_send(
            &mut socket,
            json!({"type":"auth","payload":{"deviceId":device_id,"token":token}}),
        )
        .await?;
        assert_eq!(client_receive(&mut socket).await?["type"], "auth_ok");

        let id = Ulid::new().to_string();
        let message =
            json!({"type":"chat_message","requestId":id,"payload":{"content":"from iPhone"}});
        client_send(&mut socket, message.clone()).await?;
        let ack = client_receive(&mut socket).await?;
        assert_eq!(ack["type"], "message_ack");
        assert_eq!(ack["payload"]["status"], "stored");
        assert_eq!(db::recent_messages(&pool, None, 50).await?.len(), 1);
        client_send(&mut socket, message).await?;
        assert_eq!(
            client_receive(&mut socket).await?["payload"]["seq"],
            ack["payload"]["seq"]
        );
        assert_eq!(db::recent_messages(&pool, None, 50).await?.len(), 1);

        let windows = send_from_desktop(&core, "from Windows".into()).await?;
        let delivered_message = client_receive(&mut socket).await?;
        assert_eq!(delivered_message["type"], "chat_message");
        assert_eq!(delivered_message["requestId"], windows.id);
        client_send(
            &mut socket,
            json!({"type":"delivered","payload":{"messageId":id}}),
        )
        .await?;
        assert_eq!(
            db::recent_messages(&pool, None, 50).await?[0].status,
            "stored"
        );
        client_send(
            &mut socket,
            json!({"type":"delivered","payload":{"messageId":windows.id}}),
        )
        .await?;
        client_send(
            &mut socket,
            json!({"type":"sync_request","payload":{"afterSeq":0}}),
        )
        .await?;
        let batch = client_receive(&mut socket).await?;
        assert_eq!(batch["type"], "sync_batch");
        assert_eq!(batch["payload"]["messages"].as_array().unwrap().len(), 2);
        assert_eq!(batch["payload"]["messages"][1]["status"], "delivered");
        assert_eq!(batch["payload"]["hasMore"], false);

        unpair(&core).await?;
        assert!(db::paired_device(&pool).await?.is_none());
        assert_eq!(db::recent_messages(&pool, None, 50).await?.len(), 2);
        let mut old_request = url.as_str().into_client_request()?;
        old_request
            .headers_mut()
            .insert(header::ORIGIN, "https://127.0.0.1:42100".parse()?);
        let (mut old_socket, _) = connect_async(old_request).await?;
        client_send(
            &mut old_socket,
            json!({"type":"auth","payload":{"deviceId":device_id,"token":token}}),
        )
        .await?;
        assert_eq!(
            client_receive(&mut old_socket).await?["payload"]["code"],
            "AUTH_FAILED"
        );
        server.abort();
        Ok(())
    }
}
