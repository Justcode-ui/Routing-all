use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

use crate::auth::AuthState;
use crate::error_shape::ProxyError;
use crate::health::health_handler;
use crate::keychain_adapter::KeychainAdapter;
use crate::router::{parse_model_route, Provider};
use crate::translators::{gemini, groq, openrouter};
use crate::usage_tracker::UsageTracker;

#[derive(Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub is_listening: bool,
    pub port: u16,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub auth: AuthState,
    pub keychain: Arc<KeychainAdapter>,
    pub http_client: reqwest::Client,
    pub port: u16,
    pub server_status: Arc<RwLock<ServerStatus>>,
    pub usage: Arc<UsageTracker>,
}

pub async fn root_handler(
    State(state): State<AppState>,
) -> Json<Value> {
    let is_listening = state.server_status.read().unwrap().is_listening;
    Json(serde_json::json!({
        "service": "ROUTINGALL",
        "version": "1.6.0",
        "status": if is_listening { "listening" } else { "stopped" },
        "port": state.port,
        "description": "Lightweight Local Developer Proxy & Master-Key Gateway",
        "endpoints": {
            "health": format!("http://127.0.0.1:{}/v1/health", state.port),
            "chat_completions": format!("http://127.0.0.1:{}/v1/chat/completions", state.port)
        }
    }))
}

pub async fn chat_completions_handler(
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, ProxyError> {
    // 1. Auth check
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| ProxyError::unauthorized("Missing Authorization header"))?;

    if !state.auth.validate_bearer_token(auth_header) {
        return Err(ProxyError::unauthorized("Invalid Master Virtual Key"));
    }

    // 2. Extract JSON body
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|_| ProxyError::bad_request("Failed to read request body", "invalid_body"))?;

    let body_json: Value = serde_json::from_slice(&body_bytes)
        .map_err(|_| ProxyError::bad_request("Invalid JSON payload", "invalid_json"))?;

    // 3. Parse model prefix
    let raw_model = body_json
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| ProxyError::bad_request("Missing 'model' field in request body", "missing_model"))?;

    let route = parse_model_route(raw_model)
        .map_err(|e| ProxyError::bad_request(&e, "invalid_model_prefix"))?;

    // 4. Resolve provider key
    let provider_name = match route.provider {
        Provider::Groq => "groq",
        Provider::Gemini => "gemini",
        Provider::OpenRouter => "openrouter",
    };

    let provider_key = state
        .keychain
        .get_key(provider_name)
        .map_err(|_| ProxyError::missing_key(provider_name))?;

    // 5. Forward request with 60s timeout
    let timeout_duration = Duration::from_secs(60);
    let is_stream = body_json.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    match route.provider {
        Provider::Groq => {
            let res_result = timeout(
                timeout_duration,
                groq::forward_groq_request(&state.http_client, &provider_key, &route.target_model, body_json),
            )
            .await;

            let outbound_res = match res_result {
                Ok(Ok(res)) => res,
                Ok(Err(err_msg)) => return Err(ProxyError::bad_request(&err_msg, "upstream_provider_error")),
                Err(_) => return Err(ProxyError::timeout()),
            };

            // FR-8.1: record this routed request
            state.usage.record_request(provider_name);

            // FR-8.2: capture rate-limit header (Groq uses x-ratelimit-remaining-requests)
            let rl_remaining = outbound_res
                .headers()
                .get("x-ratelimit-remaining-requests")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            state.usage.set_rate_limit_remaining(provider_name, rl_remaining);

            let status = outbound_res.status();
            let content_type = outbound_res
                .headers()
                .get(header::CONTENT_TYPE)
                .cloned()
                .unwrap_or_else(|| HeaderValue::from_static("application/json"));

            let body_stream = outbound_res.bytes_stream().map(|r| r.map_err(std::io::Error::other));
            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from_stream(body_stream))
                .map_err(|_| ProxyError::bad_request("Response building error", "response_error"))
        }

        Provider::OpenRouter => {
            let res_result = timeout(
                timeout_duration,
                openrouter::forward_openrouter_request(&state.http_client, &provider_key, &route.target_model, body_json),
            )
            .await;

            let outbound_res = match res_result {
                Ok(Ok(res)) => res,
                Ok(Err(err_msg)) => return Err(ProxyError::bad_request(&err_msg, "upstream_provider_error")),
                Err(_) => return Err(ProxyError::timeout()),
            };

            // FR-8.1: record this routed request
            state.usage.record_request(provider_name);

            // FR-8.2: capture rate-limit header
            let rl_remaining = outbound_res
                .headers()
                .get("x-ratelimit-remaining-requests")
                .or_else(|| outbound_res.headers().get("x-ratelimit-remaining"))
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            state.usage.set_rate_limit_remaining(provider_name, rl_remaining);

            let status = outbound_res.status();
            let content_type = outbound_res
                .headers()
                .get(header::CONTENT_TYPE)
                .cloned()
                .unwrap_or_else(|| HeaderValue::from_static("application/json"));

            let body_stream = outbound_res.bytes_stream().map(|r| r.map_err(std::io::Error::other));
            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from_stream(body_stream))
                .map_err(|_| ProxyError::bad_request("Response building error", "response_error"))
        }

        Provider::Gemini => {
            let res_result = timeout(
                timeout_duration,
                gemini::forward_gemini_request(&state.http_client, &provider_key, &route.target_model, body_json),
            )
            .await;

            let outbound_res = match res_result {
                Ok(Ok(res)) => res,
                Ok(Err(err_msg)) => {
                    if err_msg.contains("gemini_multimodal_unsupported") {
                        return Err(ProxyError::bad_request(&err_msg, "gemini_multimodal_unsupported"));
                    }
                    return Err(ProxyError::bad_request(&err_msg, "upstream_provider_error"));
                }
                Err(_) => return Err(ProxyError::timeout()),
            };

            // FR-8.1: record this routed request
            state.usage.record_request(provider_name);

            // FR-8.2: capture rate-limit header (Gemini generally does not send rate limit headers)
            let rl_remaining = outbound_res
                .headers()
                .get("x-ratelimit-remaining-requests")
                .or_else(|| outbound_res.headers().get("x-ratelimit-remaining"))
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            state.usage.set_rate_limit_remaining(provider_name, rl_remaining);

            let status = outbound_res.status();
            if !status.is_success() {
                let err_bytes = outbound_res.bytes().await.unwrap_or_default();
                let err_text = String::from_utf8_lossy(&err_bytes);
                return Err(ProxyError::bad_request(
                    &format!("Gemini API error ({}): {}", status, err_text),
                    "gemini_api_error",
                ));
            }

            if is_stream {
                // Streaming mode: Transform Gemini NDJSON/SSE into OpenAI SSE
                let target_model = route.target_model.clone();
                let chunk_id = format!("chatcmpl-{}", Uuid::new_v4().simple());
                let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(64);

                tokio::spawn(async move {
                    let mut stream = outbound_res.bytes_stream();
                    let mut buffer = String::new();

                    while let Some(chunk_res) = stream.next().await {
                        match chunk_res {
                            Ok(bytes) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));

                                while let Some(pos) = buffer.find('\n') {
                                    let line = buffer[..pos].trim().to_string();
                                    buffer = buffer[pos + 1..].to_string();

                                    if line.starts_with("data: ") {
                                        let json_str = line.trim_start_matches("data: ").trim();
                                        if json_str == "[DONE]" {
                                            continue;
                                        }
                                        if let Ok(val) = serde_json::from_str::<Value>(json_str) {
                                            if let Some(openai_chunk) = gemini::transform_gemini_chunk_to_openai(&val, &target_model, &chunk_id) {
                                                if let Ok(serialized) = serde_json::to_string(&openai_chunk) {
                                                    let sse_event = format!("data: {}\n\n", serialized);
                                                    if tx.send(Ok(axum::body::Bytes::from(sse_event))).await.is_err() {
                                                        return;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(std::io::Error::other(e))).await;
                                return;
                            }
                        }
                    }

                    // Flush any remaining buffer line
                    let remaining = buffer.trim();
                    if remaining.starts_with("data: ") {
                        let json_str = remaining.trim_start_matches("data: ").trim();
                        if let Ok(val) = serde_json::from_str::<Value>(json_str) {
                            if let Some(openai_chunk) = gemini::transform_gemini_chunk_to_openai(&val, &target_model, &chunk_id) {
                                if let Ok(serialized) = serde_json::to_string(&openai_chunk) {
                                    let sse_event = format!("data: {}\n\n", serialized);
                                    let _ = tx.send(Ok(axum::body::Bytes::from(sse_event))).await;
                                }
                            }
                        }
                    }

                    // Terminate SSE stream
                    let _ = tx.send(Ok(axum::body::Bytes::from("data: [DONE]\n\n"))).await;
                });

                let body_stream = futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|item| (item, rx))
                });

                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream; charset=utf-8")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .body(Body::from_stream(body_stream))
                    .map_err(|_| ProxyError::bad_request("Response building error", "response_error"))
            } else {
                // Non-streaming: Transform full response JSON to OpenAI shape
                let resp_bytes = outbound_res
                    .bytes()
                    .await
                    .map_err(|e| ProxyError::bad_request(&format!("Failed reading Gemini response: {}", e), "gemini_read_error"))?;

                let gemini_json: Value = serde_json::from_slice(&resp_bytes)
                    .map_err(|_| ProxyError::bad_request("Invalid JSON from Gemini", "gemini_json_error"))?;

                let openai_response = gemini::transform_gemini_response_to_openai(&gemini_json, &route.target_model);
                let response_body = serde_json::to_vec(&openai_response)
                    .map_err(|_| ProxyError::bad_request("Failed serializing translated response", "serialize_error"))?;

                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(response_body))
                    .map_err(|_| ProxyError::bad_request("Response building error", "response_error"))
            }
        }
    }
}

pub fn create_proxy_router(app_state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/v1", get(root_handler))
        .route("/v1/health", get(health_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .with_state(app_state)
}

pub async fn start_proxy_server(app_state: AppState, port: u16) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => {
            let mut status = app_state.server_status.write().unwrap();
            status.is_listening = true;
            status.port = port;
            status.error = None;
            l
        }
        Err(e) => {
            let err_msg = format!("Port conflict: failed to bind to {}: {}", addr, e);
            let mut status = app_state.server_status.write().unwrap();
            status.is_listening = false;
            status.port = port;
            status.error = Some(err_msg.clone());
            return Err(err_msg);
        }
    };

    let router = create_proxy_router(app_state);
    axum::serve(listener, router)
        .await
        .map_err(|e| format!("Server error: {}", e))?;

    Ok(())
}
