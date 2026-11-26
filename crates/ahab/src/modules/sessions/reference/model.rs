//! How context references reach the agent: after the message's text, each
//! as a `Context:` line (what it is and where) and, unless it is a whole
//! file, the text as it was seen in a fenced code block:
//!
//! ````text
//! Context: src/join.rs, lines 40–58
//! ```rust
//! …the lines…
//! ```
//! ````
//!
//! The web app reads this back to show the references as chips
//! (`apps/ui/src/modules/sessions/reference/model.ts`); the two change together.

use crate::shared::error::{DomainError, FieldError};

/// The most references in one message.
pub const MAX_REFERENCES: usize = 20;
/// The most text one reference carries, in bytes.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
/// The most text a message's references carry together, in bytes.
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024;
/// What starts each reference's line.
pub const PREFIX: &str = "Context: ";
/// Said of lines whose file changed since they were selected.
pub const STALE: &str = " (changed on disk since it was selected)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Lines,
    File,
    DiffHunk,
    TerminalOutput,
    SearchResults,
    ChatHistory,
    Other,
}

/// One reference, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub kind: Kind,
    pub path: Option<String>,
    /// First and last line, from 1.
    pub lines: Option<(u32, u32)>,
    pub text: String,
    pub label: Option<String>,
    pub language: Option<String>,
    /// Its file changed since it was selected.
    pub stale: bool,
}

/// Whether a message may carry `references`.
pub fn check(references: &[Reference]) -> Result<(), DomainError> {
    let invalid = |message: String| {
        Err(DomainError::Invalid(vec![FieldError::new(
            "references",
            message,
        )]))
    };
    if references.len() > MAX_REFERENCES {
        return invalid(format!(
            "Add at most {MAX_REFERENCES} references to a message."
        ));
    }
    let mut total = 0;
    for reference in references {
        if reference.text.len() > MAX_TEXT_BYTES {
            return invalid(format!(
                "A reference carries at most {} KiB of text.",
                MAX_TEXT_BYTES / 1024
            ));
        }
        total += reference.text.len();
        let needs_path = matches!(reference.kind, Kind::Lines | Kind::File | Kind::DiffHunk);
        if needs_path
            && reference
                .path
                .as_deref()
                .is_none_or(|path| path.trim().is_empty())
        {
            return invalid("A reference to a file needs its path.".to_owned());
        }
    }
    if total > MAX_TOTAL_BYTES {
        return invalid(format!(
            "A message's references carry at most {} KiB of text.",
            MAX_TOTAL_BYTES / 1024
        ));
    }
    Ok(())
}

/// One line, without line breaks, at most 300 characters.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(300)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn lines(range: Option<(u32, u32)>) -> String {
    match range {
        Some((start, end)) if start == end => format!(", line {start}"),
        Some((start, end)) => format!(", lines {start}\u{2013}{end}"),
        None => String::new(),
    }
}

/// The reference's `Context:` line.
pub fn heading(reference: &Reference) -> String {
    let path = reference.path.as_deref().map(one_line).unwrap_or_default();
    let label = reference.label.as_deref().map(one_line);
    let what = match reference.kind {
        Kind::Lines => format!("{path}{}", lines(reference.lines)),
        Kind::File => format!("{path} (the whole file)"),
        Kind::DiffHunk => format!("the diff of {path}{}", lines(reference.lines)),
        Kind::TerminalOutput => match label {
            Some(label) if !label.is_empty() => format!("terminal output from {label}"),
            _ => "terminal output".to_owned(),
        },
        Kind::SearchResults => match label {
            Some(label) if !label.is_empty() => format!("search results for {label}"),
            _ => "search results".to_owned(),
        },
        Kind::ChatHistory => match label {
            Some(label) if !label.is_empty() => format!("chat history from {label}"),
            _ => "chat history".to_owned(),
        },
        Kind::Other => label
            .filter(|label| !label.is_empty())
            .or_else(|| (!path.is_empty()).then(|| path.clone()))
            .unwrap_or_else(|| "a reference".to_owned()),
    };
    let stale = if reference.stale { STALE } else { "" };
    format!("{PREFIX}{what}{stale}")
}

/// What a reference is, without the prefix: for a title when a message
/// has no text of its own.
pub fn summary(reference: &Reference) -> String {
    heading(reference).trim_start_matches(PREFIX).to_owned()
}

/// A code block's language for a file, from its extension.
pub fn language(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, extension) = name.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    let known = match extension.as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "py" => "python",
        "rb" => "ruby",
        "go" => "go",
        "md" | "mdx" => "markdown",
        "yml" | "yaml" => "yaml",
        "sh" | "bash" | "zsh" => "sh",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "h" | "c" => "c",
        "hpp" | "cc" | "cpp" | "cxx" => "cpp",
        other if other.chars().all(|c| c.is_ascii_alphanumeric()) && other.len() <= 10 => other,
        _ => return None,
    };
    Some(known.to_owned())
}

/// A fence longer than any run of backticks in `text`.
fn fence(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat((longest + 1).max(3))
}

/// One reference as the agent reads it.
pub fn render(reference: &Reference) -> String {
    let heading = heading(reference);
    if reference.kind == Kind::File || reference.text.is_empty() {
        return heading;
    }
    let language = reference
        .language
        .as_deref()
        .map(one_line)
        .filter(|language| {
            !language.is_empty() && !language.contains(' ') && !language.contains('`')
        })
        .or_else(|| match reference.kind {
            Kind::DiffHunk => Some("diff".to_owned()),
            Kind::ChatHistory => Some("markdown".to_owned()),
            Kind::TerminalOutput | Kind::SearchResults | Kind::Other => Some("text".to_owned()),
            Kind::Lines | Kind::File => reference.path.as_deref().and_then(language),
        })
        .unwrap_or_default();
    let fence = fence(&reference.text);
    let body = reference.text.strip_suffix('\n').unwrap_or(&reference.text);
    format!("{heading}\n{fence}{language}\n{body}\n{fence}")
}

/// The message the agent receives: `text`, then each reference.
pub fn message(text: &str, references: &[Reference]) -> String {
    let rendered: Vec<String> = references.iter().map(render).collect();
    match (text.trim().is_empty(), rendered.is_empty()) {
        (_, true) => text.to_owned(),
        (true, false) => rendered.join("\n\n"),
        (false, false) => format!("{text}\n\n{}", rendered.join("\n\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(path: &str, start: u32, end: u32, text: &str) -> Reference {
        Reference {
            kind: Kind::Lines,
            path: Some(path.to_owned()),
            lines: Some((start, end)),
            text: text.to_owned(),
            label: None,
            language: None,
            stale: false,
        }
    }

    #[test]
    fn references_follow_the_message() {
        let join = lines_of("src/join.rs", 40, 58, "fn join() {}\n");
        assert_eq!(
            message("Why is this slow?", std::slice::from_ref(&join)),
            "Why is this slow?\n\nContext: src/join.rs, lines 40\u{2013}58\n```rust\nfn join() {}\n```"
        );
        let file = Reference {
            kind: Kind::File,
            text: String::new(),
            lines: None,
            ..lines_of("README.md", 1, 1, "")
        };
        assert_eq!(
            message("", &[file, lines_of("a.txt", 3, 3, "x")]),
            "Context: README.md (the whole file)\n\nContext: a.txt, line 3\n```txt\nx\n```"
        );
        assert_eq!(message("Just text", &[]), "Just text");
    }

    #[test]
    fn every_kind_says_what_it_is() {
        let base = lines_of("src/a.rs", 2, 4, "+ added\n");
        let stale = Reference {
            stale: true,
            ..base.clone()
        };
        assert_eq!(
            heading(&stale),
            "Context: src/a.rs, lines 2\u{2013}4 (changed on disk since it was selected)"
        );
        let hunk = Reference {
            kind: Kind::DiffHunk,
            ..base.clone()
        };
        assert!(
            render(&hunk).starts_with("Context: the diff of src/a.rs, lines 2\u{2013}4\n```diff\n")
        );
        let terminal = Reference {
            kind: Kind::TerminalOutput,
            path: None,
            lines: None,
            label: Some("cargo test\nnext".to_owned()),
            ..base.clone()
        };
        assert_eq!(
            heading(&terminal),
            "Context: terminal output from cargo test next"
        );
        let search = Reference {
            kind: Kind::SearchResults,
            path: None,
            lines: None,
            label: Some("\"join\"".to_owned()),
            ..base
        };
        assert!(render(&search).starts_with("Context: search results for \"join\"\n```text\n"));
        let history = Reference {
            kind: Kind::ChatHistory,
            label: Some("Fix the join".to_owned()),
            text: "## You\n\nHi\n".to_owned(),
            ..search
        };
        assert_eq!(
            render(&history),
            "Context: chat history from Fix the join\n```markdown\n## You\n\nHi\n```"
        );
    }

    #[test]
    fn a_fence_is_longer_than_the_text_s_backticks() {
        let markdown = lines_of("README.md", 1, 3, "```sh\nmake\n```\n");
        assert_eq!(
            render(&markdown),
            "Context: README.md, lines 1\u{2013}3\n````markdown\n```sh\nmake\n```\n````"
        );
    }

    #[test]
    fn limits_how_much_a_message_carries() {
        assert!(check(&[lines_of("a.rs", 1, 1, "x")]).is_ok());
        let many = vec![lines_of("a.rs", 1, 1, "x"); MAX_REFERENCES + 1];
        assert!(check(&many).is_err());
        let big = lines_of("a.rs", 1, 1, &"x".repeat(MAX_TEXT_BYTES + 1));
        assert!(check(&[big]).is_err());
        let pathless = Reference {
            path: None,
            ..lines_of("a.rs", 1, 1, "x")
        };
        assert!(check(&[pathless]).is_err());
        assert_eq!(language("src/main.rs").as_deref(), Some("rust"));
        assert_eq!(language("Makefile"), None);
    }
}
