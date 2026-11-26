//! Language servers: one per root and tool on each machine, and the
//! channels windows reach them through.

pub mod api;
pub mod daemon;
pub mod http;
pub mod service;
