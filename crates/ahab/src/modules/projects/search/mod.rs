//! Searching a project's files: contents, with match case,
//! whole word, regular expressions and include and exclude patterns; and
//! the list of its files for quick open. The project's machine searches.

pub mod api;
pub mod daemon;
pub mod http;
pub mod service;
