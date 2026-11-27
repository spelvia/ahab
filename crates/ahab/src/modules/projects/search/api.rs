//! The search API's types, which are also the daemon's messages.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// What to search for, and where.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SearchQuery)]
pub struct SearchQueryDto {
    #[schema(example = "fn main")]
    pub query: String,
    #[serde(default)]
    pub match_case: bool,
    #[serde(default)]
    pub whole_word: bool,
    /// The query is a regular expression.
    #[serde(default)]
    pub regex: bool,
    /// Glob patterns; only files matching one are searched.
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns; matching files are skipped.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Also files that `.gitignore` excludes, and hidden ones.
    #[serde(default)]
    pub include_ignored: bool,
}

/// `search.run` on the daemon: a query in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub project: Uuid,
    #[serde(flatten)]
    pub query: SearchQueryDto,
}

/// `search.stream` on the daemon: a query whose results go out on a
/// channel as [`SearchFrame`]s, a JSON object per frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSearchRequest {
    pub project: Uuid,
    pub channel: Uuid,
    #[serde(flatten)]
    pub query: SearchQueryDto,
}

/// A streamed search's line: files with matches as they are found, then
/// how it ended. Sent to the browser as newline-delimited JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schema(as = SearchFrame)]
pub enum SearchFrameDto {
    /// More files with matches, in path order.
    Files { files: Vec<FileMatchesDto> },
    /// The search ended: matches sent, and why it stopped early if it did
    /// (`matches`, `time`).
    Done { total: u32, cut: Option<String> },
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SearchMatch)]
pub struct MatchDto {
    /// From 1.
    pub line: u64,
    /// The line, or the part of a long line around the match.
    pub preview: String,
    /// Where the matches are in `preview`, as character offsets `[start, end)`.
    pub ranges: Vec<[u32; 2]>,
}

/// A file's matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SearchFile)]
pub struct FileMatchesDto {
    /// Relative to the project.
    pub path: String,
    pub matches: Vec<MatchDto>,
    /// More matches in this file than were sent.
    pub more: bool,
}

/// A search's results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SearchResults)]
pub struct SearchResultsDto {
    pub files: Vec<FileMatchesDto>,
    /// Matches sent.
    pub total: u32,
    /// Why results stopped early: `matches` (the limit was reached) or
    /// `time` (the search took too long); absent when complete.
    pub cut: Option<String>,
}

/// `files.index` on the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRequest {
    pub project: Uuid,
}

/// A project's files, for quick open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FileIndex)]
pub struct FileIndexDto {
    /// Relative to the project, following its ignore rules.
    pub paths: Vec<String>,
    /// More files than were sent.
    pub truncated: bool,
}
