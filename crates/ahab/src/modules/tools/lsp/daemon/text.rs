//! An open document's text, changed by LSP's content changes: positions
//! are lines and UTF-16 code units, as the web app and LSP both count.

use serde::Deserialize;
use serde_json::Value;

/// A line and a character in it (UTF-16 code units), both from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// One change: a range replaced, or the whole text when it has none.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Change {
    #[serde(default)]
    pub range: Option<Range>,
    pub text: String,
}

/// The byte offset of `position` in `text`, clamped to the text.
pub fn offset_at(text: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        match text[line_start..].find('\n') {
            Some(at) => line_start += at + 1,
            None => return text.len(),
        }
    }
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |at| line_start + at);
    let mut units = 0u32;
    for (index, c) in text[line_start..line_end].char_indices() {
        if units >= position.character {
            return line_start + index;
        }
        units += u32::try_from(c.len_utf16()).unwrap_or(1);
    }
    line_end
}

/// `changes` applied to `text`, in order.
pub fn apply(text: &mut String, changes: &[Change]) {
    for change in changes {
        match change.range {
            None => text.clone_from(&change.text),
            Some(range) => {
                let start = offset_at(text, range.start);
                let end = offset_at(text, range.end).max(start);
                text.replace_range(start..end, &change.text);
            }
        }
    }
}

/// The content changes as LSP sends them: whole text, or the ranges.
pub fn changes_value(changes: &[Change], full: Option<&str>) -> Value {
    match full {
        Some(text) => serde_json::json!([{ "text": text }]),
        None => Value::Array(
            changes
                .iter()
                .map(|change| match change.range {
                    Some(range) => serde_json::json!({
                        "range": {
                            "start": { "line": range.start.line, "character": range.start.character },
                            "end": { "line": range.end.line, "character": range.end.character },
                        },
                        "text": change.text,
                    }),
                    None => serde_json::json!({ "text": change.text }),
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn positions_count_utf16_units() {
        let text = "a😀b\nsecond\n";
        assert_eq!(offset_at(text, at(0, 0)), 0);
        // The emoji is two UTF-16 units and four bytes.
        assert_eq!(offset_at(text, at(0, 3)), 5);
        assert_eq!(offset_at(text, at(1, 3)), 10);
        assert_eq!(offset_at(text, at(1, 99)), 13);
        assert_eq!(offset_at(text, at(9, 0)), text.len());
    }

    #[test]
    fn changes_apply_in_order() {
        let mut text = "fn main() {}\n".to_owned();
        apply(
            &mut text,
            &[
                Change {
                    range: Some(Range {
                        start: at(0, 11),
                        end: at(0, 11),
                    }),
                    text: " run(); ".to_owned(),
                },
                Change {
                    range: Some(Range {
                        start: at(0, 3),
                        end: at(0, 7),
                    }),
                    text: "start".to_owned(),
                },
            ],
        );
        assert_eq!(text, "fn start() { run(); }\n");
        apply(
            &mut text,
            &[Change {
                range: None,
                text: "new".to_owned(),
            }],
        );
        assert_eq!(text, "new");
    }
}
