//! The server's plumbing: the router, errors, extractors, the services the
//! routes reach, logging and startup. Feature routes live in
//! each component's `http.rs`; this mounts them.

pub mod access;
pub mod error;
pub mod extract;
pub mod health;
pub mod hosts;
pub mod router;
pub mod start;
pub mod state;
pub mod telemetry;
pub mod web;
