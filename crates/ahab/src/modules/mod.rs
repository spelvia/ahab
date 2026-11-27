//! The features. Each module is a folder of components; each component keeps
//! its logic (`model`, `service`, `store`) and the thin layers that expose it
//! (`api`, `http`, `cli`, `daemon`) together.

pub mod clients;
pub mod machines;
pub mod notifications;
pub mod platform;
pub mod plugins;
pub mod projects;
pub mod sessions;
pub mod tools;
