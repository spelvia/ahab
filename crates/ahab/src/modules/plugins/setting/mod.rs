//! Plugins' settings: declared by the plugin as a JSON Schema,
//! kept per server, machine and project, the narrowest winning.

pub mod api;
pub mod http;
pub mod model;
pub mod service;
pub mod store;
