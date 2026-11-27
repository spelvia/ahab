//! A project: one folder on one machine, its mode, its name and group, and
//! whether it can be reached.

use time::OffsetDateTime;
use uuid::Uuid;

use crate::shared::error::{DomainError, FieldError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: Uuid,
    pub name: ProjectName,
    pub description: Option<String>,
    pub state: ProjectState,
    pub machine_id: Uuid,
    /// Absolute, as the machine's daemon resolved it.
    pub path: String,
    pub mode: ProjectMode,
    /// The heading it is listed under; none: with the ungrouped ones, last.
    pub group: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    /// How it looks in lists: where it sits, and its own mark.
    pub look: ProjectLook,
}

/// Where a project sits when they are ordered by hand, and the mark shown
/// for it instead of its initial.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectLook {
    /// None: after the ordered ones, by name.
    pub position: Option<i64>,
    pub emoji: Option<String>,
    /// It has an image (its bytes are read on their own).
    pub image: bool,
    /// When its mark last changed, to tell an image's versions apart.
    pub icon_at: Option<OffsetDateTime>,
}

/// A project's own mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectIcon {
    Emoji(String),
    Image {
        content_type: String,
        bytes: Vec<u8>,
    },
}

/// The largest image a project's mark may be.
pub const MAX_ICON_BYTES: usize = 256 * 1024;

/// An emoji (or a few letters) for a project's mark: at most 8 characters.
pub fn icon_emoji(text: &str) -> Result<ProjectIcon, DomainError> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 8 || text.chars().any(char::is_control) {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "emoji",
            "Use an emoji, or up to 8 characters.",
        )]));
    }
    Ok(ProjectIcon::Emoji(text.to_owned()))
}

/// An image for a project's mark: PNG, JPEG, WebP or GIF (never SVG, which
/// could run scripts), at most 256 KiB, its bytes what its type says.
pub fn icon_image(content_type: &str, bytes: Vec<u8>) -> Result<ProjectIcon, DomainError> {
    let invalid = |message: &str| DomainError::Invalid(vec![FieldError::new("image", message)]);
    if bytes.len() > MAX_ICON_BYTES {
        return Err(invalid("Use an image of at most 256 KiB."));
    }
    let matches = match content_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => bytes.starts_with(b"GIF8"),
        "image/webp" => bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        _ => return Err(invalid("Use a PNG, JPEG, WebP or GIF image.")),
    };
    if !matches {
        return Err(invalid("The image is not what its type says."));
    }
    Ok(ProjectIcon::Image {
        content_type: content_type.to_owned(),
        bytes,
    })
}

/// Archived projects are hidden and keep everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectState {
    Active,
    Archived,
}

impl ProjectState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

/// A project's name, like a folder name: 1 to 64 characters of letters,
/// digits, spaces, `-`, `_` and `.`, without leading or trailing spaces.
/// Not unique: lists tell projects of the same name apart by their machine
/// and folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectName(String);

const NAME_MAX: usize = 64;

impl ProjectName {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let invalid = |message: &str| DomainError::Invalid(vec![FieldError::new("name", message)]);
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(invalid("Enter a name."));
        }
        if trimmed.chars().count() > NAME_MAX {
            return Err(invalid("Use at most 64 characters."));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || " -_.".contains(c))
        {
            return Err(invalid(
                "Use letters, digits, spaces, hyphens, underscores and dots.",
            ));
        }
        if trimmed.starts_with('.') {
            return Err(invalid("Do not start the name with a dot."));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// A name for a folder's project: its last part, cleaned up so it is a
    /// valid name ("my app (old)" → "my app old").
    pub fn from_folder(path: &str) -> Option<Self> {
        let last = path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()?;
        let cleaned: String = last
            .chars()
            .filter(|c| c.is_alphanumeric() || " -_.".contains(*c))
            .collect();
        Self::parse(cleaned.trim_start_matches('.')).ok()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The last part of a path: `/home/me/ahab` → `ahab`.
pub fn folder_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_owned()
}

/// Whether agents and tools may change a project's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMode {
    ReadWrite,
    ReadOnly,
}

impl ProjectMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadWrite => "read-write",
            Self::ReadOnly => "read-only",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read-write" => Some(Self::ReadWrite),
            "read-only" => Some(Self::ReadOnly),
            _ => None,
        }
    }
}

/// Whether a project can be used right now; computed, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Available,
    /// Its machine is not connected.
    MachineOffline,
    /// Its machine says the folder is gone.
    Missing,
}

impl Availability {
    pub fn of(machine_online: bool, missing: bool) -> Self {
        match (machine_online, missing) {
            (false, _) => Self::MachineOffline,
            (true, true) => Self::Missing,
            (true, false) => Self::Available,
        }
    }
}

/// An absolute folder path as the user typed it: Unix (`/home/me/ahab`) or
/// Windows (`C:\work`). The daemon resolves symlinks before it is stored.
pub fn absolute_path(value: &str) -> Result<String, DomainError> {
    let value = value.trim();
    let windows = value.len() >= 3
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value[1..].starts_with(":\\");
    if !(value.starts_with('/') || windows) {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "path",
            "Give the folder's absolute path, such as /home/me/projects/app.",
        )]));
    }
    Ok(value.to_owned())
}

/// A group: optional, at most 64 characters; blank means none.
pub fn group(value: Option<&str>) -> Result<Option<String>, DomainError> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > 64 {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "group",
            "Use at most 64 characters.",
        )]));
    }
    Ok(Some(value.to_owned()))
}

/// A description: optional, at most 500 characters; blank means none.
pub fn description(value: Option<&str>) -> Result<Option<String>, DomainError> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > 500 {
        return Err(DomainError::Invalid(vec![FieldError::new(
            "description",
            "Use at most 500 characters.",
        )]));
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_like_folder_names() {
        assert_eq!(ProjectName::parse(" ahab ").unwrap().as_str(), "ahab");
        assert!(ProjectName::parse("my app_v2.1").is_ok());
        assert!(ProjectName::parse("").is_err());
        assert!(ProjectName::parse("a/b").is_err());
        assert!(ProjectName::parse(".hidden").is_err());
        assert!(ProjectName::parse(&"x".repeat(65)).is_err());
    }

    #[test]
    fn a_folder_gives_a_project_name() {
        let name = |path| ProjectName::from_folder(path).map(|n| n.as_str().to_owned());
        assert_eq!(name("/home/me/spelvia/ahab").as_deref(), Some("ahab"));
        assert_eq!(
            name("/home/me/code/my app (old)/").as_deref(),
            Some("my app old")
        );
        assert_eq!(name("C:\\work\\tools").as_deref(), Some("tools"));
        assert_eq!(name("/home/me/.dotfiles").as_deref(), Some("dotfiles"));
        assert_eq!(name("/").as_deref(), None);
    }

    #[test]
    fn paths_must_be_absolute() {
        assert_eq!(absolute_path(" /home/me/ahab ").unwrap(), "/home/me/ahab");
        assert!(absolute_path("C:\\work").is_ok());
        assert!(absolute_path("ahab").is_err());
        assert!(absolute_path("~/ahab").is_err());
    }

    #[test]
    fn availability_puts_the_machine_first() {
        assert_eq!(Availability::of(false, true), Availability::MachineOffline);
        assert_eq!(Availability::of(true, true), Availability::Missing);
        assert_eq!(Availability::of(true, false), Availability::Available);
    }

    #[test]
    fn folders_have_names_and_modes_parse() {
        assert_eq!(folder_name("/home/me/ahab/"), "ahab");
        assert_eq!(folder_name("/"), "/");
        assert_eq!(ProjectMode::parse("read-only"), Some(ProjectMode::ReadOnly));
        assert_eq!(ProjectMode::parse("rw"), None);
    }

    #[test]
    fn groups_are_optional_and_short() {
        assert_eq!(group(Some("  ")).unwrap(), None);
        assert_eq!(group(Some(" work ")).unwrap().as_deref(), Some("work"));
        assert!(group(Some(&"g".repeat(65))).is_err());
    }

    #[test]
    fn descriptions_are_optional_and_short() {
        assert_eq!(description(Some("  ")).unwrap(), None);
        assert_eq!(description(Some(" x ")).unwrap().as_deref(), Some("x"));
        assert!(description(Some(&"y".repeat(501))).is_err());
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn marks_are_short_emoji_or_small_images_of_known_types() {
        assert_eq!(
            icon_emoji(" 🐋 ").unwrap(),
            ProjectIcon::Emoji("🐋".to_owned())
        );
        assert!(icon_emoji("").is_err());
        assert!(icon_emoji("far too long a name").is_err());
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        assert!(icon_image("image/png", png.clone()).is_ok());
        assert!(icon_image("image/jpeg", png.clone()).is_err());
        assert!(icon_image("image/svg+xml", b"<svg/>".to_vec()).is_err());
        let mut big = png;
        big.resize(MAX_ICON_BYTES + 1, 0);
        assert!(icon_image("image/png", big).is_err());
    }
}
