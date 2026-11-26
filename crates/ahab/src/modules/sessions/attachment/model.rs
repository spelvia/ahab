//! Attachment rules: names, sizes, types, and how a message lists them.

use crate::shared::error::{DomainError, FieldError};

/// The largest attachment, in bytes.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;
/// The most attachments in one message.
pub const MAX_PER_MESSAGE: usize = 10;
/// The line that starts the list of attached files in a message.
pub const HEADING: &str = "[Attached files]";

/// A file name safe to store: its last path segment, without control
/// characters, at most 120 characters; "file" when nothing is left.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(120).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        "file".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// The type agents accept as an image, from the name; `None` for other files.
pub fn image_type(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// The type to serve a file as.
pub fn content_type(name: &str) -> &'static str {
    if let Some(image) = image_type(name) {
        return image;
    }
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "txt" | "md" | "log" | "csv" | "json" | "yaml" | "yml" | "toml" => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    }
}

/// The message an agent receives: the text, then the attached files' paths
/// under [`HEADING`], so it can open any of them.
pub fn message(text: &str, paths: &[String]) -> String {
    if paths.is_empty() {
        return text.to_owned();
    }
    let list = paths.join("\n");
    if text.trim().is_empty() {
        format!("{HEADING}\n{list}")
    } else {
        format!("{text}\n\n{HEADING}\n{list}")
    }
}

/// Whether a message may carry `count` attachments: at most
/// [`MAX_PER_MESSAGE`].
pub fn check_count(count: usize) -> Result<(), DomainError> {
    if count > MAX_PER_MESSAGE {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "attachments",
            format!("Attach at most {MAX_PER_MESSAGE} files to a message."),
        )]));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_their_folders_and_stay_short() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\Users\\me\\shot.png"), "shot.png");
        assert_eq!(safe_name("..."), "file");
        assert_eq!(safe_name(".env"), "env");
        assert_eq!(safe_name("a\u{0}b.txt"), "ab.txt");
        assert_eq!(safe_name(&"x".repeat(200)).len(), 120);
    }

    #[test]
    fn images_are_known_by_their_extension() {
        assert_eq!(image_type("Shot.PNG"), Some("image/png"));
        assert_eq!(image_type("photo.jpeg"), Some("image/jpeg"));
        assert_eq!(image_type("notes.pdf"), None);
        assert_eq!(content_type("notes.pdf"), "application/pdf");
        assert_eq!(content_type("run.sh"), "application/octet-stream");
    }

    #[test]
    fn messages_list_their_files_after_the_text() {
        assert_eq!(message("hi", &[]), "hi");
        assert_eq!(
            message("look", &["/a/1/x.png".into(), "/a/2/y.pdf".into()]),
            "look\n\n[Attached files]\n/a/1/x.png\n/a/2/y.pdf"
        );
        assert_eq!(
            message(" ", &["/a/1/x.png".into()]),
            "[Attached files]\n/a/1/x.png"
        );
    }
}
