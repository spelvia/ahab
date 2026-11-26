//! What every command shares: the command tree, the API client, exit codes,
//! output and questions at the terminal. Each component's commands are in
//! its `cli.rs`, in `modules/`.

pub mod client;
pub mod command;
pub mod credentials;
pub mod exit;
pub mod lifecycle;
pub mod output;
pub mod prompt;
pub mod remote;
pub mod service;
pub mod setup;
pub mod up;

pub use command::run;
