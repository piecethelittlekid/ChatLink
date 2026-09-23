mod certs;
mod db;
mod network;
mod server;

use anyhow::Context;
use rand::Rng;
use server::{Core, DesktopMessage, InterfaceOption, ServerRuntime, ServerSnapshot};
use std::{net::Ipv4Addr, sync::Arc};
use tauri::{Manager, State};
use tokio::sync::Mutex;

struct AppState {
    core: Arc<Core>,
    runtime: Mutex<Option<ServerRuntime>>,
    database_available: bool,
}

#[tauri::command]
async fn get_server_status(state: State<'_, AppState>) -> Result<ServerSnapshot, String> {
    Ok(server::status(&state.core).await)
}

#[tauri::command]
fn list_interfaces() -> Vec<InterfaceOption> {
    server::available_interfaces()
}

#[tauri::command]
async fn select_network_interface(ip: String, state: State<'_, AppState>) -> Result<ServerSnapshot, String> {
    if !state.database_available {
        return Err("Server chưa chạy vì cơ sở dữ liệu không khả dụng.".into());
    }
    let address = ip.parse::<Ipv4Addr>().map_err(|_| "Địa chỉ IPv4 không hợp lệ.".to_string())?;
    let known = server::available_interfaces().iter().any(|option| option.ip == ip);
    if !known {
        return Err("Địa chỉ không còn thuộc một interface LAN đang hoạt động.".into());
    }
    server::restart(state.core.clone(), &state.runtime, address)
        .await
        .map_err(|error| error.to_string())?;
    Ok(server::status(&state.core).await)
}

#[tauri::command]
async fn send_message(content: String, state: State<'_, AppState>) -> Result<DesktopMessage, String> {
    server::send_from_desktop(&state.core, content)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_recent_messages(state: State<'_, AppState>) -> Vec<DesktopMessage> {
    server::recent_messages(&state.core).await
}

#[tauri::command]
async fn open_data_folder(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _ = &state.core;
    let path = server::data_directory(&app).map_err(|error| error.to_string())?;
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Không mở được thư mục dữ liệu: {error}"))
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let data_dir = server::data_directory(&app_handle)?;
            std::fs::create_dir_all(&data_dir).context("create ChatLink data directory")?;
            let log_dir = data_dir.join("logs");
            std::fs::create_dir_all(&log_dir).context("create log directory")?;
            if let Ok(log_file) = std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join("chatlink.log")) {
                let _ = tracing_subscriber::fmt()
                    .with_env_filter("info")
                    .with_ansi(false)
                    .with_writer(log_file)
                    .try_init();
            }

            let db_path = data_dir.join("chatlink.db");
            let (pool, database_error, database_available) = match tauri::async_runtime::block_on(db::open(&db_path)) {
                Ok(pool) => (pool, None, true),
                Err(error) => {
                    tracing::error!(%error, "could not open persistent database");
                    let fallback = tauri::async_runtime::block_on(db::open_memory())
                        .expect("temporary in-memory SQLite must be available for the error screen");
                    (fallback, Some(format!("Không mở được cơ sở dữ liệu tại {}: {error}", db_path.display())), false)
                }
            };

            let root_certificate_der = certs::create_or_load_ca(&data_dir)?;
            let fingerprint = certs::root_fingerprint(&root_certificate_der);
            let interfaces = network::lan_ipv4_addresses();
            let session_code = format!("{:06}", rand::rng().random_range(0..1_000_000_u32));
            let web_root = server::web_root(&app_handle);
            let core = server::create_core(
                pool,
                data_dir,
                web_root,
                root_certificate_der,
                fingerprint,
                session_code,
                interfaces.clone(),
            );
            let runtime = if let Some(error) = database_error {
                tauri::async_runtime::block_on(server::set_status_error(&core, error));
                None
            } else if let Some(ip) = interfaces.first().copied() {
                match tauri::async_runtime::block_on(server::start(core.clone(), ip)) {
                    Ok(runtime) => Some(runtime),
                    Err(error) => {
                        tauri::async_runtime::block_on(server::set_status_error(&core, error.to_string()));
                        None
                    }
                }
            } else {
                tauri::async_runtime::block_on(server::set_status_error(
                    &core,
                    "Chưa tìm thấy IPv4 riêng trên Wi-Fi/Ethernet. Kết nối vào mạng LAN rồi thử lại.".into(),
                ));
                None
            };
            app.manage(AppState { core, runtime: Mutex::new(runtime), database_available });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_server_status,
            list_interfaces,
            select_network_interface,
            send_message,
            get_recent_messages,
            open_data_folder
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ChatLink desktop application");
}
