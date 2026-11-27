//! API errors in the Problem Details format (RFC 9457), with a stable `code`
//! and the `requestId` that also appears in the logs.

use std::any::Any;

use axum::body::Body;
use axum::extract::Request;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, RETRY_AFTER};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use crate::shared::error::FieldError as DomainFieldError;
use crate::shared::error::ServiceError;

pub const PROBLEM_JSON: &str = "application/problem+json";
const MAX_PROBLEM_BYTES: usize = 64 * 1024;

/// The body of every error response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub title: String,
    pub status: u16,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<FieldError>,
    /// With `command_failed`: the code the plugin gave, when it gave one.
    /// Each plugin chooses its own codes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// One invalid field, for `validation_failed`. Nested fields use dots, such as
/// `folder.path`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FieldError {
    #[schema(example = "name")]
    pub field: String,
    #[schema(example = "Enter a name.")]
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl From<DomainFieldError> for FieldError {
    fn from(error: DomainFieldError) -> Self {
        Self::new(error.field, error.message)
    }
}

/// An error a handler can return.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: Option<String>,
    errors: Vec<FieldError>,
    plugin_code: Option<String>,
    /// Seconds to wait, sent as `Retry-After`.
    retry_after: Option<u32>,
}

impl ApiError {
    fn new(
        status: StatusCode,
        code: &'static str,
        title: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code,
            title,
            detail: Some(detail.into()),
            errors: Vec::new(),
            plugin_code: None,
            retry_after: None,
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::not_found_as("not_found", detail)
    }

    /// Not found, with the code of what was missing, such as
    /// `machine_not_found`.
    pub fn not_found_as(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, "Not found", detail)
    }

    pub fn validation(errors: Vec<FieldError>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "validation_failed",
            title: "The request is invalid",
            detail: None,
            errors,
            plugin_code: None,
            retry_after: None,
        }
    }

    /// A request that cannot be read at all, such as malformed JSON.
    pub fn unreadable(detail: impl Into<String>) -> Self {
        Self {
            detail: Some(detail.into()),
            ..Self::validation(Vec::new())
        }
    }

    pub fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, "Conflict", detail)
    }

    pub fn unauthorized(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, "Not authorized", detail)
    }

    pub fn forbidden(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, "Not allowed", detail)
    }

    /// `421`: the request was addressed to a name this server does not
    /// answer to.
    pub fn misdirected(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::MISDIRECTED_REQUEST,
            code,
            "Host not allowed",
            detail,
        )
    }

    pub fn unavailable(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, code, "Unavailable", detail)
    }

    /// Something unexpected. The details belong in the logs, never in the
    /// response.
    pub fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            title: "Something went wrong",
            detail: None,
            errors: Vec::new(),
            plugin_code: None,
            retry_after: None,
        }
    }

    /// `429 too_many_attempts`: wait `retry_after` seconds.
    pub fn too_many_attempts(detail: impl Into<String>, retry_after: u32) -> Self {
        Self {
            retry_after: Some(retry_after),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_attempts",
                "Too many attempts",
                detail,
            )
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::Invalid(errors) => {
                Self::validation(errors.into_iter().map(FieldError::from).collect())
            }
            ServiceError::NotFound { code, message } => Self::not_found_as(code, message),
            ServiceError::Storage(error) => {
                tracing::error!(%error, "storage failed");
                Self::internal()
            }
            ServiceError::Conflict { code, message } => Self::conflict(code, message),
            ServiceError::Unavailable { code, message } => Self::unavailable(code, message),
            ServiceError::Unauthorized { code, message } => Self::unauthorized(code, message),
            ServiceError::Forbidden { code, message } => Self::forbidden(code, message),
            ServiceError::TooManyAttempts {
                message,
                retry_after,
            } => Self::too_many_attempts(message, retry_after),
            ServiceError::Plugin { code, message } => Self {
                plugin_code: Some(code),
                ..Self::conflict("command_failed", message)
            },
            ServiceError::Internal(error) => {
                tracing::error!(%error, "internal error");
                Self::internal()
            }
        }
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::unreadable(rejection.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::unreadable(rejection.body_text())
    }
}

/// A path parameter that does not parse, such as an ID that is not a UUID,
/// cannot name anything that exists.
impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        Self::not_found("There is nothing at this path.")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem = Problem {
            title: self.title.to_owned(),
            status: self.status.as_u16(),
            code: self.code.to_owned(),
            detail: self.detail,
            errors: self.errors,
            plugin_code: self.plugin_code,
            request_id: None,
        };
        let body = serde_json::to_vec(&problem).unwrap_or_default();
        let mut response = (self.status, body).into_response();
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

/// Turns a panic inside a handler into an `internal_error` response.
pub fn panic_response(panic: Box<dyn Any + Send + 'static>) -> Response {
    let message = panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned());
    tracing::error!(panic = %message, "handler panicked");
    ApiError::internal().into_response()
}

/// Adds the request ID to every Problem Details response.
pub async fn attach_request_id(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let response = next.run(request).await;

    let is_problem = response
        .headers()
        .get(CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes() == PROBLEM_JSON.as_bytes());
    let Some(request_id) = request_id.filter(|_| is_problem) else {
        return response;
    };

    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_PROBLEM_BYTES).await else {
        return ApiError::internal().into_response();
    };
    let Ok(mut problem) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    problem["requestId"] = serde_json::Value::String(request_id);
    parts.headers.remove(CONTENT_LENGTH);
    Response::from_parts(
        parts,
        Body::from(serde_json::to_vec(&problem).unwrap_or_default()),
    )
}
