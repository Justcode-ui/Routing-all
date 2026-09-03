use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorDetail {
    pub message: String,
    pub r#type: String,
    pub code: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

pub struct ProxyError {
    pub status: StatusCode,
    pub message: String,
    pub error_type: String,
    pub code: String,
}

impl ProxyError {
    pub fn unauthorized(msg: &str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: msg.to_string(),
            error_type: "authentication_error".to_string(),
            code: "invalid_master_key".to_string(),
        }
    }

    pub fn bad_request(msg: &str, code: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.to_string(),
            error_type: "invalid_request_error".to_string(),
            code: code.to_string(),
        }
    }

    pub fn missing_key(provider: &str) -> Self {
        Self {
            status: StatusCode::FAILED_DEPENDENCY,
            message: format!("No API key configured for provider: {}", provider),
            error_type: "routingall_configuration_error".to_string(),
            code: "missing_provider_key".to_string(),
        }
    }

    pub fn timeout() -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: "Provider request timed out after 60 seconds".to_string(),
            error_type: "routingall_timeout_error".to_string(),
            code: "provider_timeout".to_string(),
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse {
            error: ErrorDetail {
                message: self.message,
                r#type: self.error_type,
                code: self.code,
            },
        });
        (self.status, body).into_response()
    }
}
