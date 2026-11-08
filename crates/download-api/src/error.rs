use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use download_engine::EngineError;
use download_storage::StorageError;
use thiserror::Error;

use crate::api::ApiErrorBody;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("rule '{rule}' requires confirmation before this download can be added")]
    ConfirmationRequired { rule: String },

    #[error("engine error: {0}")]
    Engine(#[from] EngineError),

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("rule error: {0}")]
    Rule(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("rate limit exceeded")]
    RateLimited,
}

impl ServiceError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) | Self::Rule(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) | Self::ConfirmationRequired { .. } => StatusCode::CONFLICT,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Engine(error) => match error {
                EngineError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
                EngineError::Http { status, .. } if *status == 404 => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_GATEWAY,
            },
            Self::Storage(StorageError::NotFound(_)) => StatusCode::NOT_FOUND,
            Self::Storage(_) | Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::ConfirmationRequired { .. } => "confirmation_required",
            Self::Engine(e) => e.kind(),
            Self::Storage(_) => "storage_error",
            Self::Rule(_) => "rule_error",
            Self::Io(_) => "io_error",
            Self::Unauthorized => "unauthorized",
            Self::RateLimited => "rate_limited",
        }
    }

    pub fn body(&self) -> ApiErrorBody {
        let details = match self {
            Self::Engine(error) => serde_json::json!({
                "technical": error.to_string(),
                "hints": error.hints(),
            }),
            Self::ConfirmationRequired { rule } => serde_json::json!({ "rule": rule }),
            _ => serde_json::Value::Null,
        };
        ApiErrorBody {
            code: self.code().to_string(),
            message: self.to_string(),
            details,
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = self.body();
        (status, axum::Json(body)).into_response()
    }
}
