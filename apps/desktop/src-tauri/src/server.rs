use crate::{db, mdns, network};
use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{SecondsFormat, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::Manager;
use tokio::{
    sync::{mpsc, oneshot, Mutex, RwLock},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tower_http::services::{ServeDir, ServeFile};
use ulid::Ulid;
use uuid::Uuid;

pub const HTTP_PORT: u16 = 42100;
const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const SYNC_PAGE_SIZE: i64 = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshot {
    pub status: String,
    pub interfaces: Vec<String>,
    pub selected_ip: Option<String>,
    pub http_url: Option<String>,
    pub mdns_url: Option<String>,
    pub hostname_url: Option<String>,
    pub paired_devices: Vec<db::PairedDevice>,
    pub paired_device: Option<String>,
    pub pending_pairing: Option<PendingPairingView>,
    pub iphone_status: String,
    pub error: Option<String>,
    // Compatibility fields for desktop UI
    pub https_url: Option<String>,
    pub setup_url: Option<String>,
    pub certificate_setup_required: bool,
    pub ca_fingerprint: String,
    pub pairing_code: Option<String>,
    pub pairing_expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPairingView {
    pub device_id: String,
    pub name: String,
}

pub struct PendingPairingRequest {
    pub device_id: String,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub token_hash: String,
    pub responder: oneshot::Sender<bool>,
}

pub type DesktopMessage = db::StoredMessage;

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceOption {
    pub ip: String,
    pub label: String,
}

pub struct Core {
    pub pool: SqlitePool,
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub web_root: PathBuf,
    pub status: RwLock<ServerSnapshot>,
    pub active_client: Mutex<Option<ActiveClient>>,
    #[allow(dead_code)]
    pub auth_failures: Mutex<VecDeque<Instant>>,
    #[allow(dead_code)]
    pub message_windows: Mutex<HashMap<Uuid, VecDeque<Instant>>>,
    pub pending_pairing: Mutex<Option<PendingPairingRequest>>,
}

#[derive(Clone)]
pub struct ActiveClient {
    pub id: Uuid,
    pub device_id: String,
    pub sender: mpsc::Sender<String>,
    pub cancel: CancellationToken,
}

pub struct ServerRuntime {
    pub server_cancel: CancellationToken,
    pub server_task: JoinHandle<()>,
    pub mdns_handle: Option<mdns::MdnsHandle>,
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
    interfaces: Vec<Ipv4Addr>,
) -> Arc<Core> {
    let hostname = network::local_hostname();
    Arc::new(Core {
        pool,
        data_dir,
        web_root,
        status: RwLock::new(ServerSnapshot {
            status: "starting".into(),
            interfaces: interfaces.into_iter().map(|ip| ip.to_string()).collect(),
            selected_ip: None,
            http_url: None,
            mdns_url: Some(format!("http://chatlink.local:{HTTP_PORT}/")),
            hostname_url: Some(format!("http://{hostname}.local:{HTTP_PORT}/")),
            paired_devices: Vec::new(),
            paired_device: None,
            pending_pairing: None,
            iphone_status: "waiting".into(),
            error: None,
            https_url: None,
            setup_url: None,
            certificate_setup_required: false,
            ca_fingerprint: String::new(),
            pairing_code: None,
            pairing_expires_at: None,
        }),
        active_client: Mutex::new(None),
        auth_failures: Mutex::new(VecDeque::new()),
        message_windows: Mutex::new(HashMap::new()),
        pending_pairing: Mutex::new(None),
    })
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
    for asset in ["index.html"] {
        anyhow::ensure!(
            core.web_root.join(asset).is_file(),
            "mobile web asset {asset} is missing from {}",
            core.web_root.display()
        );
    }

    let addr = SocketAddr::from((ip, HTTP_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| {
        format!("bind HTTP server at {addr}; check if port {HTTP_PORT} is already in use")
    })?;

    let router = main_router(core.clone());
    let server_cancel = CancellationToken::new();
    let cancel_for_task = server_cancel.clone();

    let server_task = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router)
            .with_graceful_shutdown(cancel_for_task.cancelled_owned())
            .await
        {
            tracing::error!(%error, "HTTP server error");
        }
    });

    let mdns_handle = mdns::start_mdns(ip);
    let hostname = network::local_hostname();

    let devices = db::list_paired_devices(&core.pool).await.unwrap_or_default();
    let primary_device = devices.first().map(|d| d.name.clone());

    {
        let mut status = core.status.write().await;
        status.status = "online".into();
        status.selected_ip = Some(ip.to_string());
        status.http_url = Some(format!("http://{ip}:{HTTP_PORT}/"));
        status.https_url = status.http_url.clone();
        status.mdns_url = Some(format!("http://chatlink.local:{HTTP_PORT}/"));
        status.hostname_url = Some(format!("http://{hostname}.local:{HTTP_PORT}/"));
        status.setup_url = status.http_url.clone();
        status.paired_devices = devices;
        status.paired_device = primary_device;
        status.error = None;
    }

    tracing::info!(%ip, port = HTTP_PORT, "ChatLink HTTP V2 server started");

    Ok(ServerRuntime {
        server_cancel,
        server_task,
        mdns_handle,
    })
}

pub async fn stop(runtime: ServerRuntime) {
    runtime.server_cancel.cancel();
    if let Some(mdns) = runtime.mdns_handle {
        mdns.stop().await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), runtime.server_task).await;
}

fn main_router(core: Arc<Core>) -> Router {
    let root = core.web_root.clone();
    let index = root.join("index.html");
    Router::new()
        .route("/health", get(health))
        .route("/ws", get(websocket_upgrade))
        .fallback_service(ServeDir::new(root).not_found_service(ServeFile::new(index)))
        .with_state(core)
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "online", "service": "ChatLink V2" }))
}

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(core): State<Arc<Core>>,
    _headers: HeaderMap,
) -> Result<Response, StatusCode> {
    // In V2 LAN mode, accept local WebSocket connections cleanly
    Ok(ws
        .max_message_size(32 * 1024)
        .max_frame_size(32 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, core)))
}

async fn handle_socket(mut socket: WebSocket, core: Arc<Core>) {
    let first_text = match tokio::time::timeout(Duration::from_secs(15), socket.recv()).await {
        Ok(Some(Ok(WsMessage::Text(text)))) => text.to_string(),
        _ => {
            reject_socket(&mut socket, "AUTH_REQUIRED").await;
            return;
        }
    };

    let first = match serde_json::from_str::<WireEnvelope>(&first_text) {
        Ok(message) => message,
        Err(_) => {
            reject_socket(&mut socket, "AUTH_REQUIRED").await;
            return;
        }
    };

    let authed_device_id = if first.kind == "pairing_request" {
        let payload = first.payload.unwrap_or_default();
        let device_id = payload
            .get("deviceId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("iPhone")
            .to_string();
        let token = payload
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if device_id.is_empty() || token.is_empty() {
            reject_socket(&mut socket, "INVALID_PAIRING_REQUEST").await;
            return;
        }

        let hash = token_hash(&token);

        // Check if already paired
        let already_paired = match db::get_device(&core.pool, &device_id).await {
            Ok(Some(dev)) if equal_hashes(&hash, &dev.token_hash) => true,
            _ => false,
        };

        if already_paired {
            let _ = db::touch_device(&core.pool, &device_id).await;
            let _ = send_json(
                &mut socket,
                json!({ "type": "pairing_ok", "payload": { "deviceId": device_id } }),
            )
            .await;
            device_id
        } else {
            // Require 1-touch approval from Desktop
            let (tx, rx) = oneshot::channel();
            {
                let mut pending = core.pending_pairing.lock().await;
                *pending = Some(PendingPairingRequest {
                    device_id: device_id.clone(),
                    name: name.clone(),
                    token_hash: hash.clone(),
                    responder: tx,
                });
                let mut status = core.status.write().await;
                status.pending_pairing = Some(PendingPairingView {
                    device_id: device_id.clone(),
                    name: name.clone(),
                });
            }

            let approved = match tokio::time::timeout(Duration::from_secs(60), rx).await {
                Ok(Ok(true)) => true,
                _ => false,
            };

            {
                let mut pending = core.pending_pairing.lock().await;
                *pending = None;
                let mut status = core.status.write().await;
                status.pending_pairing = None;
            }

            if !approved {
                reject_socket(&mut socket, "PAIRING_REJECTED").await;
                return;
            }

            if let Err(err) = db::pair_device(&core.pool, &device_id, &name, &hash).await {
                tracing::error!(%err, "save paired device failed");
                reject_socket(&mut socket, "DATABASE_ERROR").await;
                return;
            }

            let devices = db::list_paired_devices(&core.pool).await.unwrap_or_default();
            {
                let mut status = core.status.write().await;
                status.paired_devices = devices;
                status.paired_device = Some(name);
            }

            if send_json(
                &mut socket,
                json!({ "type": "pairing_ok", "payload": { "deviceId": device_id } }),
            )
            .await
            .is_err()
            {
                return;
            }

            device_id
        }
    } else if first.kind == "auth" {
        let payload = first.payload.unwrap_or_default();
        let device_id = payload
            .get("deviceId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let token = payload
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let device = match db::get_device(&core.pool, device_id).await {
            Ok(Some(dev)) => dev,
            Ok(None) => {
                // If device not found, try fallback to the single paired device if token matches
                match db::paired_device(&core.pool).await {
                    Ok(Some(dev)) => dev,
                    _ => {
                        reject_socket(&mut socket, "AUTH_FAILED").await;
                        return;
                    }
                }
            }
            Err(err) => {
                tracing::error!(%err, "auth database error");
                reject_socket(&mut socket, "DATABASE_ERROR").await;
                return;
            }
        };

        if !equal_hashes(&token_hash(token), &device.token_hash) {
            reject_socket(&mut socket, "AUTH_FAILED").await;
            return;
        }

        let _ = db::touch_device(&core.pool, &device.id).await;
        device.id
    } else {
        reject_socket(&mut socket, "AUTH_REQUIRED").await;
        return;
    };

    // Authenticated! Send auth_ok
    let recent = db::recent_messages(&core.pool, None, 1).await.unwrap_or_default();
    let max_seq = recent.first().map(|m| m.seq).unwrap_or(0);

    if send_json(
        &mut socket,
        json!({ "type": "auth_ok", "payload": { "deviceId": authed_device_id, "maxSeq": max_seq } }),
    )
    .await
    .is_err()
    {
        return;
    }

    {
        let mut status = core.status.write().await;
        status.iphone_status = "connected".into();
    }

    let client_id = Uuid::new_v4();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(128);
    let cancel = CancellationToken::new();

    {
        let mut active = core.active_client.lock().await;
        if let Some(old) = active.take() {
            old.cancel.cancel();
        }
        *active = Some(ActiveClient {
            id: client_id,
            device_id: authed_device_id.clone(),
            sender: outbound_tx.clone(),
            cancel: cancel.clone(),
        });
    }

    let (mut ws_sender, mut ws_receiver) = socket.split();

    let write_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            if ws_sender.send(WsMessage::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    let mut ping_interval = tokio::time::interval(Duration::from_secs(15));
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = ping_interval.tick() => {
                if last_pong.elapsed() > Duration::from_secs(45) {
                    break;
                }
                let _ = outbound_tx.try_send(json!({ "type": "ping", "timestamp": timestamp() }).to_string());
            }
            res = ws_receiver.next() => {
                match res {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Err(err) = process_client_message(&text, &core, &outbound_tx, &mut last_pong).await {
                            tracing::warn!(%err, "process client message error");
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        last_pong = Instant::now();
                    }
                    Some(Ok(WsMessage::Ping(_bytes))) => {
                        let _ = outbound_tx.try_send(json!({ "type": "pong", "timestamp": timestamp() }).to_string());
                    }
                    Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    write_task.abort();

    {
        let mut active = core.active_client.lock().await;
        if let Some(cur) = active.as_ref() {
            if cur.id == client_id {
                active.take();
            }
        }
        let mut status = core.status.write().await;
        status.iphone_status = "waiting".into();
    }
}

async fn process_client_message(
    text: &str,
    core: &Arc<Core>,
    outbound: &mpsc::Sender<String>,
    last_pong: &mut Instant,
) -> Result<()> {
    let message: WireEnvelope = serde_json::from_str(text).context("parse message")?;
    let payload = message.payload.unwrap_or_default();

    match message.kind.as_str() {
        "ping" => {
            let _ = outbound.try_send(json!({ "type": "pong", "timestamp": timestamp() }).to_string());
        }
        "pong" => {
            *last_pong = Instant::now();
        }
        "sync_request" => {
            let after_seq = payload
                .get("afterSeq")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let batch = db::messages_after(&core.pool, after_seq, SYNC_PAGE_SIZE).await?;
            let next_seq = batch.last().map(|m| m.seq).unwrap_or(after_seq);
            let has_more = batch.len() as i64 == SYNC_PAGE_SIZE;
            let response = json!({
                "type": "sync_batch",
                "payload": {
                    "messages": batch,
                    "nextSeq": next_seq,
                    "hasMore": has_more
                }
            });
            outbound.send(response.to_string()).await?;
        }
        "chat_message" => {
            let id = message
                .request_id
                .unwrap_or_else(|| Ulid::new().to_string());
            let content = payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();

            if content.is_empty() || content.len() > MAX_MESSAGE_BYTES {
                return Ok(());
            }

            let outcome = db::insert_message(&core.pool, &id, "iphone", content, "stored").await?;
            let ack = json!({
                "type": "message_ack",
                "payload": {
                    "messageId": id,
                    "seq": outcome.message.seq,
                    "createdAt": outcome.message.created_at
                }
            });
            outbound.send(ack.to_string()).await?;
        }
        "delivered" => {
            if let Some(msg_id) = payload.get("messageId").and_then(Value::as_str) {
                let _ = db::mark_delivered(&core.pool, msg_id).await;
            }
        }
        _ => {}
    }

    Ok(())
}

async fn reject_socket(socket: &mut WebSocket, code: &str) {
    let _ = send_json(socket, json!({ "type": "error", "payload": { "code": code } })).await;
    let _ = socket.close().await;
}

async fn send_json(socket: &mut WebSocket, value: Value) -> Result<()> {
    socket
        .send(WsMessage::Text(value.to_string().into()))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub async fn approve_pairing(core: &Arc<Core>, device_id: &str, approve: bool) -> Result<()> {
    let mut pending = core.pending_pairing.lock().await;
    if let Some(req) = pending.take() {
        if req.device_id == device_id {
            let _ = req.responder.send(approve);
        }
    }
    let mut status = core.status.write().await;
    status.pending_pairing = None;
    Ok(())
}

pub async fn send_from_desktop(core: &Arc<Core>, content: String) -> Result<DesktopMessage> {
    let id = Ulid::new().to_string();
    let outcome = db::insert_message(&core.pool, &id, "windows-main", &content, "pending_delivery").await?;

    // If client is online, send immediately
    let client_opt = {
        let active = core.active_client.lock().await;
        active.clone()
    };

    if let Some(client) = client_opt {
        let wire = json!({
            "type": "chat_message",
            "requestId": outcome.message.id,
            "timestamp": outcome.message.created_at,
            "payload": {
                "seq": outcome.message.seq,
                "sender": "windows",
                "content": outcome.message.content
            }
        });
        let _ = client.sender.try_send(wire.to_string());
    }

    Ok(outcome.message)
}

pub async fn recent_messages(core: &Arc<Core>, before_seq: Option<i64>) -> Result<Vec<DesktopMessage>> {
    db::recent_messages(&core.pool, before_seq, 100).await
}

pub async fn unpair(core: &Arc<Core>, device_id: Option<&str>) -> Result<()> {
    {
        let mut active = core.active_client.lock().await;
        if let Some(client) = active.as_ref() {
            if device_id.is_none() || device_id == Some(client.device_id.as_str()) {
                if let Some(c) = active.take() {
                    c.cancel.cancel();
                }
            }
        }
    }

    if let Some(id) = device_id {
        db::unpair_specific_device(&core.pool, id).await?;
    } else {
        db::unpair_device(&core.pool).await?;
    }

    let devices = db::list_paired_devices(&core.pool).await.unwrap_or_default();
    let mut status = core.status.write().await;
    status.paired_devices = devices;
    status.paired_device = None;
    status.iphone_status = "waiting".into();
    Ok(())
}

pub fn available_interfaces() -> Vec<InterfaceOption> {
    network::lan_interfaces()
        .into_iter()
        .map(|i| InterfaceOption {
            ip: i.ip.to_string(),
            label: format!("{} ({})", i.name, i.ip),
        })
        .collect()
}

pub async fn restart(core: Arc<Core>, runtime: &Mutex<Option<ServerRuntime>>, ip: Ipv4Addr) -> Result<()> {
    let mut guard = runtime.lock().await;
    if let Some(old) = guard.take() {
        stop(old).await;
    }
    let next = start(core, ip).await?;
    *guard = Some(next);
    Ok(())
}

pub async fn status(core: &Arc<Core>) -> ServerSnapshot {
    core.status.read().await.clone()
}

pub async fn set_status_error(core: &Arc<Core>, error: String) {
    let mut s = core.status.write().await;
    s.error = Some(error);
}

pub fn data_directory(app: &tauri::AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("resolve app data directory")?;
    std::fs::create_dir_all(&dir).context("create app data directory")?;
    Ok(dir)
}

pub fn web_root(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .map(|dir| dir.join("mobile-web"))
        .unwrap_or_else(|_| PathBuf::from("apps/mobile-web/dist"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_core_snapshot() -> Result<()> {
        let pool = db::open_memory().await?;
        let core = create_core(
            pool,
            PathBuf::from("test_data"),
            PathBuf::from("apps/mobile-web/dist"),
            vec![Ipv4Addr::new(192, 168, 1, 50)],
        );
        let s = status(&core).await;
        assert_eq!(s.status, "starting");
        assert!(s.mdns_url.as_ref().unwrap().contains("chatlink.local"));
        Ok(())
    }
}
