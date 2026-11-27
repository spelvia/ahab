//! Context references: lines of a file, a whole file, a diff
//! hunk, terminal output or search results, added to a message for the
//! agent. They reach the agent as text after the message, and the web app
//! reads them back from it to show them as chips.

pub mod api;
pub mod model;
