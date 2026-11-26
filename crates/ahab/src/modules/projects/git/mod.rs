//! Git in a project: its status, side-by-side diffs, staging,
//! committing, branches, remotes, history, stashes and conflicts, done by
//! the machine's own `git` through its daemon.

pub mod api;
pub mod daemon;
pub mod http;
pub mod model;
pub mod service;
