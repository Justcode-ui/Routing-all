use axum::{extract::State, Json};
use serde::Serialize;

use crate::proxy_server::AppState;
use crate::usage_tracker::UsageSnapshot;

#[derive(Serialize, Clone)]
pub struct ProvidersConfigured {
    pub groq: bool,
    pub gemini: bool,
    pub openrouter: bool,
}

#[derive(Serialize, Clone)]
pub struct HealthStatusResponse {
    pub status: String,
    pub version: String,
    pub port: u16,
    pub is_listening: bool,
    pub error: Option<String>,
    pub providers_configured: ProvidersConfigured,
    pub keychain_access: String,
    pub usage: UsageSnapshot,
}

/// GET /v1/health — no Master Key required (PRD §7).
pub async fn health_handler(
    State(state): State<AppState>,
) -> Json<HealthStatusResponse> {
    let status_guard = state.server_status.read().unwrap();
    let is_listening = status_guard.is_listening;
    let error_msg = status_guard.error.clone();

    Json(HealthStatusResponse {
        status: if is_listening { "ok".to_string() } else { "error".to_string() },
        version: "1.6.0".to_string(),
        port: state.port,
        is_listening,
        error: error_msg,
        providers_configured: ProvidersConfigured {
            groq: state.keychain.has_key("groq"),
            gemini: state.keychain.has_key("gemini"),
            openrouter: state.keychain.has_key("openrouter"),
        },
        keychain_access: "ok".to_string(),
        usage: state.usage.get_usage_snapshot(),
    })
}
