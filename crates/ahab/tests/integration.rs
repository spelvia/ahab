//! The server end to end, each test with a fresh database. The modules
//! mirror `src/`: one file per component, or a folder for a large one.

// Clippy allows unwrap in test functions but not in their helpers.
#![expect(clippy::unwrap_used, reason = "a failed unwrap is a failed test")]

mod modules;
mod server;
mod support;
