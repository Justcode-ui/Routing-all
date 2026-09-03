// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod error_shape;
mod health;
mod keychain_adapter;
mod proxy_server;
mod router;
mod translators;
mod tray_controller;
mod usage_tracker;

use auth::AuthState;
use keychain_adapter::KeychainAdapter;
use proxy_server::{start_proxy_server, AppState, ServerStatus};
use usage_tracker::{UsageSnapshot, UsageTracker};

use std::sync::{Arc, RwLock};
use tauri::{Manager, State, WindowEvent};

// ── Tauri IPC command handlers ────────────────────────────────────────────────

#[tauri::command]
fn get_master_key(auth: State<'_, AuthState>) -> String {
    auth.get_master_key()
}

#[tauri::command]
fn rotate_master_key(auth: State<'_, AuthState>) -> String {
    auth.rotate_master_key()
}

#[tauri::command]
fn save_provider_key(
    keychain: State<'_, Arc<KeychainAdapter>>,
    provider: String,
    key: String,
) -> Result<(), String> {
    if let Some(warning) = KeychainAdapter::validate_format(&provider, &key) {
        // Soft warning — logged but does not block save (PRD FR-5)
        eprintln!("Key format warning for {}: {}", provider, warning);
    }
    keychain.set_key(&provider, &key)
}

#[tauri::command]
fn get_health_status(
    keychain: State<'_, Arc<KeychainAdapter>>,
    server_status: State<'_, Arc<RwLock<ServerStatus>>>,
    usage: State<'_, Arc<UsageTracker>>,
) -> serde_json::Value {
    let status_guard = server_status.read().unwrap();
    let is_listening = status_guard.is_listening;
    let error_msg = status_guard.error.clone();
    let port = status_guard.port;

    serde_json::json!({
        "status": if is_listening { "ok" } else { "error" },
        "version": "1.6.0",
        "port": port,
        "is_listening": is_listening,
        "error": error_msg,
        "providers_configured": {
            "groq": keychain.has_key("groq"),
            "gemini": keychain.has_key("gemini"),
            "openrouter": keychain.has_key("openrouter"),
        },
        "keychain_access": "ok",
        "usage": usage.get_usage_snapshot()
    })
}

#[tauri::command]
fn get_usage_snapshot(usage: State<'_, Arc<UsageTracker>>) -> UsageSnapshot {
    usage.get_usage_snapshot()
}

#[tauri::command]
fn remove_all_keys(
    auth: State<'_, AuthState>,
    keychain: State<'_, Arc<KeychainAdapter>>,
) -> Result<(), String> {
    // Rotate master key immediately (PRD §9.3)
    auth.rotate_master_key();
    keychain.remove_all_keys()
}

#[tauri::command]
fn quit_app(app_handle: tauri::AppHandle) {
    app_handle.exit(0);
}

// ── Entry point ────────────────────────────────────────────────────────────────

fn main() {
    let auth_state = AuthState::new();
    let keychain_adapter = Arc::new(KeychainAdapter::new());
    let usage_tracker = Arc::new(UsageTracker::new());
    let http_client = reqwest::Client::new();
    const PROXY_PORT: u16 = 8081;

    let server_status = Arc::new(RwLock::new(ServerStatus {
        is_listening: false,
        port: PROXY_PORT,
        error: None,
    }));

    let app_state = AppState {
        auth: auth_state.clone(),
        keychain: keychain_adapter.clone(),
        http_client,
        port: PROXY_PORT,
        server_status: server_status.clone(),
        usage: usage_tracker.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Second-launch: focus existing window instead of opening new one (PRD FR-2)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(auth_state)
        .manage(keychain_adapter)
        .manage(server_status)
        .manage(usage_tracker)
        .setup(move |app| {
            // Build system tray (PRD FR-1)
            tray_controller::build_system_tray(app.handle())?;

            // Spawn proxy server on Tauri's own async runtime (PRD FR-2)
            let server_state = app_state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = start_proxy_server(server_state, PROXY_PORT).await {
                    eprintln!("ROUTINGALL proxy server failed to start: {}", e);
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Close (X) hides to tray only; Quit from tray menu is the only true exit (PRD FR-1a)
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_master_key,
            rotate_master_key,
            save_provider_key,
            get_health_status,
            get_usage_snapshot,
            remove_all_keys,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running ROUTINGALL application");
}
