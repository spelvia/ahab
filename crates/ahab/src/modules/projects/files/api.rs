//! Files on the wire: the daemon's `fs.*` and `projects.sync` messages,
//! and what the API returns. Paths inside a project are relative, with `/`.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

/// One project a daemon may serve files from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGrant {
    pub id: Uuid,
    pub path: String,
    pub read_only: bool,
}

/// `projects.sync`: every project on the machine, replacing the last
/// list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncProjects {
    pub projects: Vec<ProjectGrant>,
}

/// The daemon's answer: the projects whose folder is gone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncAnswer {
    #[serde(default)]
    pub missing: Vec<Uuid>,
}

/// `fs.listDirectories` and `fs.checkFolder` take an absolute path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerParams {
    pub path: String,
    #[serde(default)]
    pub hidden: bool,
}

/// `fs.list` and `fs.read` take a project and a relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectPathParams {
    pub project: Uuid,
    #[serde(default)]
    pub path: String,
    /// `fs.read` of a file over the size limit: send its first part
    /// (marked `truncated`) instead of refusing. Older daemons
    /// ignore it and refuse.
    #[serde(default)]
    pub partial: bool,
}

/// `fs.write`: a file's new content, saved only over
/// `expectedVersion` when given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteParams {
    pub project: Uuid,
    pub path: String,
    pub content: String,
    /// The version the edit started from; absent writes whatever is there
    /// (Save anyway), creating the file if it is gone.
    #[serde(default)]
    pub expected_version: Option<String>,
}

/// What `fs.create` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = NewFileKind)]
pub enum NewKindDto {
    File,
    Folder,
}

/// `fs.create`: a new file or folder, its missing parent folders
/// too; never over something that exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateParams {
    pub project: Uuid,
    pub path: String,
    pub kind: NewKindDto,
    /// A new file's content; empty when absent.
    #[serde(default)]
    pub content: Option<String>,
}

/// `fs.rename`: moves a file or folder inside the project;
/// never over something that exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameParams {
    pub project: Uuid,
    pub from: String,
    pub to: String,
}

/// `fs.move`: moves or copies a file or folder inside the project,
/// over what is there only with `replace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveParams {
    pub project: Uuid,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub copy: bool,
    #[serde(default)]
    pub replace: bool,
}

/// `fs.delete`: a file, or a folder with everything in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteParams {
    pub project: Uuid,
    pub path: String,
    /// A file is deleted only at this version, when given.
    #[serde(default)]
    pub expected_version: Option<String>,
}

/// `fs.watch`: tell changes to these files on a channel, until
/// it closes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchParams {
    pub project: Uuid,
    pub paths: Vec<String>,
    pub channel: Uuid,
}

/// What a file watch says on its channel, one JSON object per frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FileWatchFrame {
    /// Watching started; each watched file's version now (absent when it
    /// does not exist or cannot be read).
    Watching {
        versions: std::collections::BTreeMap<String, Option<String>>,
    },
    /// A watched file changed: its new version, none when it is gone.
    Changed {
        path: String,
        version: Option<String>,
    },
    /// The files cannot be watched (the system's limit).
    Unwatched { reason: String },
}

/// A folder on a machine, for picking a project's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FolderListing)]
pub struct FolderListingDto {
    /// The folder, absolute.
    #[schema(example = "/home/alex/projects")]
    pub path: String,
    /// Its parent, or none at the root.
    pub parent: Option<String>,
    pub folders: Vec<FolderDto>,
    /// More subfolders exist than were listed.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = Folder)]
pub struct FolderDto {
    #[schema(example = "ahab")]
    pub name: String,
    /// Absolute.
    pub path: String,
    /// It holds a `.git` folder.
    pub is_git_repo: bool,
    /// Its name starts with a dot.
    pub hidden: bool,
}

/// What a machine says about a folder before it becomes a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FolderCheck)]
pub struct FolderCheckDto {
    pub exists: bool,
    pub is_dir: bool,
    pub readable: bool,
    pub writable: bool,
    /// With symlinks resolved; the path a project stores.
    pub real_path: Option<String>,
}

/// The daemon user's home folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = HomeFolder)]
pub struct HomeDto {
    #[schema(example = "/home/alex")]
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FileKind)]
pub enum FileKindDto {
    File,
    Dir,
    /// A link whose target is missing or outside reach; also a kind from
    /// a newer daemon this server does not know.
    #[serde(other)]
    Other,
}

/// One entry of a folder inside a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FileEntry)]
pub struct FileEntryDto {
    #[schema(example = "service.rs")]
    pub name: String,
    /// Relative to the project, with `/`.
    #[schema(example = "crates/ahab/src/service.rs")]
    pub path: String,
    pub kind: FileKindDto,
    /// It is a symbolic link.
    pub symlink: bool,
    pub hidden: bool,
    /// Bytes, for files.
    pub size: Option<u64>,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub modified_at: Option<OffsetDateTime>,
}

/// A folder inside a project: folders first, then files, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FileListing)]
pub struct FileListingDto {
    /// Relative to the project; empty for its root.
    pub path: String,
    pub entries: Vec<FileEntryDto>,
    /// More entries exist than were listed.
    pub truncated: bool,
}

/// A file's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = FileContent)]
pub struct FileContentDto {
    pub path: String,
    pub size: u64,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub modified_at: Option<OffsetDateTime>,
    /// Not text: no content is sent.
    pub binary: bool,
    /// The text, for text files.
    pub content: Option<String>,
    /// The file's version: a hash of its content. A save names the version
    /// it started from. Absent from older machines, and for a
    /// file sent only in part.
    #[serde(default)]
    #[schema(example = "9f2c4e1a0b7d3c5e8f1a2b3c4d5e6f70")]
    pub version: Option<String>,
    /// Only the file's first part was sent (`partial`); it cannot be
    /// edited.
    #[serde(default)]
    pub truncated: bool,
}

/// A file as saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SavedFile)]
pub struct SavedFileDto {
    pub path: String,
    /// Its new version, for the next save.
    pub version: String,
    pub size: u64,
    #[serde(default, with = "crate::shared::time::rfc3339::option")]
    pub modified_at: Option<OffsetDateTime>,
}

/// A file's new content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SaveFile)]
pub struct SaveFileDto {
    pub content: String,
    /// The version the edit started from (`FileContent.version`): the save
    /// is refused with `file_changed` when the file is no longer at it.
    /// Absent saves over whatever is there (Save anyway).
    #[serde(default)]
    pub expected_version: Option<String>,
}

/// A new file or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = CreateFile)]
pub struct CreateFileDto {
    /// Relative to the project; missing parent folders are made too.
    #[schema(example = "src/new.rs")]
    pub path: String,
    pub kind: NewKindDto,
    /// A new file's content; empty when absent.
    #[serde(default)]
    pub content: Option<String>,
}

/// Moving a file or folder inside the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = RenameFile)]
pub struct RenameFileDto {
    #[schema(example = "src/old.rs")]
    pub from: String,
    #[schema(example = "src/new.rs")]
    pub to: String,
}

/// Moving or copying a file or folder inside the project (dragging it in
/// the file tree).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = MoveFile)]
pub struct MoveFileDto {
    #[schema(example = "src/old.rs")]
    pub from: String,
    #[schema(example = "lib/old.rs")]
    pub to: String,
    /// Copy instead of moving.
    #[serde(default)]
    pub copy: bool,
    /// Replace what is at `to` (else `already_exists`).
    #[serde(default)]
    pub replace: bool,
}

/// Where a file or folder went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = MovedFile)]
pub struct MovedFileDto {
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_daemons_file_reads_without_a_version() {
        let file: FileContentDto = serde_json::from_value(serde_json::json!({
            "path": "a.rs", "size": 1, "binary": false, "content": "a"
        }))
        .unwrap();
        assert_eq!(file.version, None);
        assert!(!file.truncated);
    }

    #[test]
    fn a_value_from_a_newer_daemon_reads_as_the_fallback() {
        let value: FileKindDto = serde_json::from_str(r#""fromTheFuture""#).unwrap();
        assert_eq!(value, FileKindDto::Other);
    }
}
