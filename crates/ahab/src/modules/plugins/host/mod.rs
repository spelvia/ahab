//! Running plugins' daemon entries on the machines: the server tells each
//! daemon what to run and sends it the packages it lacks; the daemon runs
//! each entry as a Node process speaking the plugin protocol, restarts it
//! when it crashes, and reports its state and logs.

pub mod api;
pub mod daemon;
pub mod http;
pub mod model;
pub mod service;
