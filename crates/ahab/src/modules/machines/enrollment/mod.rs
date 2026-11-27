//! Joining a server: a machine asks to join, proving it
//! holds its key, and is approved in the web app or at once with a one-time
//! code.

pub mod api;
pub mod http;
pub mod model;
pub mod service;
pub mod store;
