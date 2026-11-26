//! Attachments: files sent with a chat message. They are kept on the
//! project's machine, outside the project's folder, and given to the agent as
//! images and as paths in the message.

pub mod api;
pub mod daemon;
pub mod http;
pub mod model;
pub mod service;
