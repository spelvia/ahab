//! Context references on the wire: what a message may carry besides its
//! text.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::modules::sessions::reference::model::{Kind, Reference};

/// What a reference holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ContextReferenceKind)]
pub enum ReferenceKindDto {
    /// Lines of a file, as they were seen.
    #[default]
    Lines,
    /// A whole file: its path only; the agent reads it itself.
    File,
    /// A hunk of a file's diff.
    DiffHunk,
    /// Output selected in a terminal.
    TerminalOutput,
    /// Results of a search.
    SearchResults,
    /// An earlier conversation, as a transcript: how a fork carries a
    /// chat's history to any agent and model.
    ChatHistory,
    /// A kind from a newer web app: sent as its label and text.
    #[serde(other)]
    Other,
}

/// Context for the agent, sent with a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = ContextReference)]
pub struct ReferenceDto {
    #[serde(default)]
    pub kind: ReferenceKindDto,
    /// The file, relative to the project (lines, files, diff hunks).
    #[serde(default)]
    #[schema(example = "src/join.rs")]
    pub path: Option<String>,
    /// The first line, from 1.
    #[serde(default)]
    #[schema(example = 40)]
    pub start_line: Option<u32>,
    /// The last line, included.
    #[serde(default)]
    #[schema(example = 58)]
    pub end_line: Option<u32>,
    /// The text as it was seen; empty for a whole file.
    #[serde(default)]
    pub text: String,
    /// The file's version when it was selected (`FileContent.version`): a
    /// file changed since is said to have changed.
    #[serde(default)]
    pub version: Option<String>,
    /// What it is, for terminal output (the terminal's title), search
    /// results (the query) and chat history (the chat's title).
    #[serde(default)]
    pub label: Option<String>,
    /// The code block's language, when known.
    #[serde(default)]
    #[schema(example = "rust")]
    pub language: Option<String>,
}

impl ReferenceDto {
    /// The reference to render, `stale` when its file changed since.
    pub fn to_model(&self, stale: bool) -> Reference {
        Reference {
            kind: match self.kind {
                ReferenceKindDto::Lines => Kind::Lines,
                ReferenceKindDto::File => Kind::File,
                ReferenceKindDto::DiffHunk => Kind::DiffHunk,
                ReferenceKindDto::TerminalOutput => Kind::TerminalOutput,
                ReferenceKindDto::SearchResults => Kind::SearchResults,
                ReferenceKindDto::ChatHistory => Kind::ChatHistory,
                ReferenceKindDto::Other => Kind::Other,
            },
            path: self.path.clone(),
            lines: match (self.start_line, self.end_line) {
                (Some(start), Some(end)) => Some((start.min(end), start.max(end))),
                (Some(line), None) | (None, Some(line)) => Some((line, line)),
                (None, None) => None,
            },
            text: self.text.clone(),
            label: self.label.clone(),
            language: self.language.clone(),
            stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_from_a_newer_web_app_reads_as_other() {
        let reference: ReferenceDto =
            serde_json::from_str(r#"{"kind":"fromTheFuture","text":"x"}"#).unwrap();
        assert_eq!(reference.kind, ReferenceKindDto::Other);
        let lines: ReferenceDto = serde_json::from_str(r#"{"path":"a.rs","startLine":3}"#).unwrap();
        assert_eq!(lines.kind, ReferenceKindDto::Lines);
        assert_eq!(lines.to_model(false).lines, Some((3, 3)));
    }
}
