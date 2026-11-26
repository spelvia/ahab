//! Request extractors whose failures are Problem Details responses.

use axum::extract::{FromRequest, FromRequestParts};

use crate::server::error::ApiError;

/// A JSON request body.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct JsonBody<T>(pub T);

/// The path parameters.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct PathParams<T>(pub T);

/// The query parameters.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct QueryParams<T>(pub T);

/// Who made the request, and where it came from.
pub use crate::server::access::{Caller, Requester, clear_cookie, set_cookie};
