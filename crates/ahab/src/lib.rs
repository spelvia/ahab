//! Ahab: one program that runs the server, the daemon or both, and is used
//! from the command line.
//!
//! - [`cli`]: the entry point: the command tree and what commands share
//! - [`config`]: settings and folders on this computer
//! - [`server`]: the HTTP plumbing: router, errors, state, startup
//! - [`daemon`]: the daemon's connection to its server
//! - [`modules`]: the features, each with its model, service, store and the
//!   thin HTTP and CLI layers that expose it
//! - [`shared`]: plain types every module uses

pub mod cli;
pub mod config;
pub mod daemon;
pub mod integrations;
pub mod modules;
pub mod server;
pub mod shared;
