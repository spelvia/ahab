//! The daemon's side of files: which projects it may
//! serve, and every file operation, checked against them.
//!
//! - Methods inside a project name the project and a relative path;
//!   `..`, absolute paths and backslashes are refused, and the real path
//!   (after following symlinks) must stay inside the project's folder.
//! - The folder picker (`fs.home`, `fs.listDirectories`, `fs.checkFolder`)
//!   is the one exception: it may look anywhere this user can read, but only
//!   ever returns folder names, never file names or contents.
//! - Writes (`fs.write`, `fs.create`, `fs.rename`, `fs.move`,
//!   `fs.delete`)
//!   are refused in read-only projects. A save names the version it
//!   started from and is refused with `file_changed` when the file is no
//!   longer at it; it replaces the file atomically.

pub mod watch;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::modules::projects::files::api::{
    FileContentDto, FileEntryDto, FileKindDto, FileListingDto, FolderCheckDto, FolderDto,
    FolderListingDto, HomeDto, MovedFileDto, NewKindDto, SavedFileDto, SyncAnswer, SyncProjects,
};
use crate::shared::error::{DaemonError, fail};

/// Files above this size are not sent.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// At most this many entries per folder.
pub const MAX_ENTRIES: usize = 2000;

struct Grant {
    root: PathBuf,
    read_only: bool,
}

/// The projects this daemon may serve, as the server last sent them.
#[derive(Default)]
pub struct Grants(RwLock<HashMap<Uuid, Grant>>);

impl Grants {
    /// Replaces the list, and says which folders are gone.
    pub fn sync(&self, list: SyncProjects) -> SyncAnswer {
        let mut missing = Vec::new();
        let mut grants = HashMap::new();
        for project in list.projects {
            let root = PathBuf::from(&project.path);
            if !root.is_dir() {
                missing.push(project.id);
            }
            grants.insert(
                project.id,
                Grant {
                    root,
                    read_only: project.read_only,
                },
            );
        }
        *self
            .0
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = grants;
        SyncAnswer { missing }
    }

    /// The folder of `project`, and whether it is read-only.
    fn root(&self, project: Uuid) -> Result<(PathBuf, bool), DaemonError> {
        let grants = self
            .0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let grant = grants
            .get(&project)
            .ok_or_else(|| fail("project_unknown", "This machine has no such project."))?;
        Ok((grant.root.clone(), grant.read_only))
    }

    /// A project's folder and whether it is read-only: where sessions run.
    pub fn project(&self, project: Uuid) -> Result<(PathBuf, bool), DaemonError> {
        self.root(project)
    }

    /// Whether `project` is read-only; writes check this.
    pub fn is_read_only(&self, project: Uuid) -> Result<bool, DaemonError> {
        self.root(project).map(|(_, read_only)| read_only)
    }

    /// `fs.list`: a folder's entries inside a project.
    pub fn list(&self, project: Uuid, relative: &str) -> Result<FileListingDto, DaemonError> {
        let (root, _) = self.root(project)?;
        let (root, folder) = resolve(&root, relative)?;
        if !folder.is_dir() {
            return Err(fail("not_a_folder", "That is not a folder."));
        }
        let mut entries = Vec::new();
        let mut truncated = false;
        for entry in std::fs::read_dir(&folder).map_err(io)? {
            let Ok(entry) = entry else { continue };
            if entries.len() == MAX_ENTRIES {
                truncated = true;
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let symlink = entry.file_type().is_ok_and(|kind| kind.is_symlink());
            let target = entry.path();
            // A link counts as what it points to, if that stays inside.
            let inside = !symlink
                || std::fs::canonicalize(&target).is_ok_and(|real| real.starts_with(&root));
            let meta = std::fs::metadata(&target).ok().filter(|_| inside);
            let kind = match &meta {
                Some(meta) if meta.is_dir() => FileKindDto::Dir,
                Some(meta) if meta.is_file() => FileKindDto::File,
                _ => FileKindDto::Other,
            };
            entries.push(FileEntryDto {
                path: join(relative, &name),
                hidden: name.starts_with('.'),
                size: meta
                    .as_ref()
                    .filter(|m| m.is_file())
                    .map(std::fs::Metadata::len),
                modified_at: meta.as_ref().and_then(|m| m.modified().ok()).map(utc),
                name,
                kind,
                symlink,
            });
        }
        entries.sort_by(|a, b| {
            (a.kind != FileKindDto::Dir)
                .cmp(&(b.kind != FileKindDto::Dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(FileListingDto {
            path: normalise(relative)?,
            entries,
            truncated,
        })
    }

    /// Reads complete image bytes under the same project grant as text files.
    pub fn image(&self, project: Uuid, relative: &str) -> Result<String, DaemonError> {
        use base64::Engine as _;
        const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
        let (root, _) = self.root(project)?;
        let (_, path) = resolve(&root, relative)?;
        let meta = std::fs::metadata(&path).map_err(io)?;
        if !meta.is_file() {
            return Err(fail("not_a_file", "That is not a file."));
        }
        let mut bytes = Vec::new();
        if meta.len() <= MAX_IMAGE_BYTES {
            std::fs::File::open(path)
                .and_then(|file| file.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes))
                .map_err(io)?;
        }
        if meta.len() > MAX_IMAGE_BYTES || bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(fail(
                "file_too_large",
                "Image previews support files up to 10 MiB.",
            ));
        }
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    /// `fs.read`: a file's content inside a project, up to
    /// [`MAX_FILE_BYTES`]; with `partial`, a larger file's first part.
    pub fn read(
        &self,
        project: Uuid,
        relative: &str,
        partial: bool,
    ) -> Result<FileContentDto, DaemonError> {
        let (root, _) = self.root(project)?;
        let (_, file) = resolve(&root, relative)?;
        let meta = std::fs::metadata(&file).map_err(io)?;
        if !meta.is_file() {
            return Err(fail("not_a_file", "That is not a file."));
        }
        let truncated = meta.len() > MAX_FILE_BYTES;
        if truncated && !partial {
            return Err(fail(
                "file_too_large",
                format!(
                    "The file has {} bytes; Ahab shows files up to {MAX_FILE_BYTES} bytes.",
                    meta.len()
                ),
            ));
        }
        let mut bytes =
            Vec::with_capacity(usize::try_from(meta.len().min(MAX_FILE_BYTES)).unwrap_or(0));
        std::fs::File::open(&file)
            .and_then(|f| f.take(MAX_FILE_BYTES).read_to_end(&mut bytes))
            .map_err(io)?;
        let version = (!truncated).then(|| version_of(&bytes));
        let binary = bytes.iter().take(8192).any(|b| *b == 0);
        let text = if binary {
            None
        } else if truncated {
            Some(first_part(bytes))
        } else {
            String::from_utf8(bytes).ok()
        };
        Ok(FileContentDto {
            path: normalise(relative)?,
            size: meta.len(),
            modified_at: meta.modified().ok().map(utc),
            binary: text.is_none(),
            content: text,
            version,
            truncated,
        })
    }

    /// The folder of a project that may be written to.
    fn writable(&self, project: Uuid) -> Result<PathBuf, DaemonError> {
        let (root, read_only) = self.root(project)?;
        if read_only {
            return Err(fail("read_only", "This project is read-only."));
        }
        std::fs::canonicalize(&root).map_err(|_| folder_missing())
    }

    /// `fs.write`: replaces a file's content atomically, only over
    /// `expected` when given (else `file_changed`). A file that is gone is
    /// made again only without `expected` (Save anyway).
    pub fn write(
        &self,
        project: Uuid,
        relative: &str,
        content: &str,
        expected: Option<&str>,
    ) -> Result<SavedFileDto, DaemonError> {
        let root = self.writable(project)?;
        if content.len() as u64 > MAX_FILE_BYTES {
            return Err(fail(
                "file_too_large",
                format!("Ahab saves files up to {MAX_FILE_BYTES} bytes."),
            ));
        }
        let (parent, name) = entry(&root, relative)?;
        let mut target = parent.join(&name);
        // A link is followed, if it stays inside: its target is written.
        if target
            .symlink_metadata()
            .is_ok_and(|meta| meta.is_symlink())
        {
            let real = std::fs::canonicalize(&target).map_err(io)?;
            if !real.starts_with(&root) {
                return Err(outside());
            }
            target = real;
        }
        let existing = std::fs::metadata(&target).ok();
        if existing.as_ref().is_some_and(|meta| !meta.is_file()) {
            return Err(fail("not_a_file", "That is not a file."));
        }
        if let Some(expected) = expected {
            match file_version(&target)? {
                None => {
                    return Err(fail(
                        "file_changed",
                        "The file was deleted since you started editing it.",
                    ));
                }
                Some(current) if current != expected => {
                    return Err(fail(
                        "file_changed",
                        "The file changed since you started editing it.",
                    ));
                }
                Some(_) => {}
            }
        }
        let folder = target.parent().unwrap_or(&root).to_path_buf();
        let mut temporary = tempfile::Builder::new()
            .prefix(".ahab-save-")
            .tempfile_in(&folder)
            .map_err(io)?;
        temporary.write_all(content.as_bytes()).map_err(io)?;
        temporary.as_file().sync_all().map_err(io)?;
        if let Some(meta) = &existing {
            // The file keeps its permissions (an executable script stays one).
            std::fs::set_permissions(temporary.path(), meta.permissions()).map_err(io)?;
        }
        temporary
            .persist(&target)
            .map_err(|error| io(error.error))?;
        let meta = std::fs::metadata(&target).map_err(io)?;
        Ok(SavedFileDto {
            path: normalise(relative)?,
            version: version_of(content.as_bytes()),
            size: meta.len(),
            modified_at: meta.modified().ok().map(utc),
        })
    }

    /// `fs.create`: a new file (with `content`) or folder, making its
    /// missing parent folders; `already_exists` when something is there.
    pub fn create(
        &self,
        project: Uuid,
        relative: &str,
        kind: NewKindDto,
        content: Option<&str>,
    ) -> Result<FileEntryDto, DaemonError> {
        let root = self.writable(project)?;
        let relative = inside(relative)?;
        let (parent_path, name) = split_last(&relative)?;
        let content = content.unwrap_or_default();
        if content.len() as u64 > MAX_FILE_BYTES {
            return Err(fail(
                "file_too_large",
                format!("Ahab saves files up to {MAX_FILE_BYTES} bytes."),
            ));
        }
        let parent = ensure_folder(&root, parent_path)?;
        let target = parent.join(name);
        if target.symlink_metadata().is_ok() {
            return Err(already_exists());
        }
        match kind {
            NewKindDto::Folder => std::fs::create_dir(&target).map_err(created)?,
            NewKindDto::File => {
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(created)?;
                file.write_all(content.as_bytes()).map_err(io)?;
            }
        }
        let meta = std::fs::metadata(&target).map_err(io)?;
        Ok(FileEntryDto {
            path: relative.clone(),
            hidden: name.starts_with('.'),
            kind: match kind {
                NewKindDto::Folder => FileKindDto::Dir,
                NewKindDto::File => FileKindDto::File,
            },
            symlink: false,
            size: meta.is_file().then_some(meta.len()),
            modified_at: meta.modified().ok().map(utc),
            name: name.to_owned(),
        })
    }

    /// `fs.rename`: moves a file or folder (a link itself, not what it
    /// points to) inside the project, never over something.
    pub fn rename(&self, project: Uuid, from: &str, to: &str) -> Result<MovedFileDto, DaemonError> {
        let root = self.writable(project)?;
        let (from_parent, from_name) = entry(&root, from)?;
        let source = from_parent.join(&from_name);
        source.symlink_metadata().map_err(io)?;
        let to = inside(to)?;
        let (to_parent, to_name) = split_last(&to)?;
        let source_relative = normalise(from)?;
        if to == source_relative || to.starts_with(&format!("{source_relative}/")) {
            return Err(fail("invalid_path", "A folder cannot move into itself."));
        }
        let parent = ensure_folder(&root, to_parent)?;
        let target = parent.join(to_name);
        if target.symlink_metadata().is_ok() {
            return Err(already_exists());
        }
        std::fs::rename(&source, &target).map_err(io)?;
        Ok(MovedFileDto { path: to })
    }

    /// `fs.move`: moves or copies a file or folder to `to` inside the
    /// project, its missing folders made. Something already at `to` is
    /// `already_exists`, unless `replace`: then it goes first (a file over
    /// a file is replaced atomically). Never into itself, and never over a
    /// folder that holds it. A copy copies links as links.
    pub fn move_entry(
        &self,
        project: Uuid,
        from: &str,
        to: &str,
        copy: bool,
        replace: bool,
    ) -> Result<MovedFileDto, DaemonError> {
        let root = self.writable(project)?;
        let (from_parent, from_name) = entry(&root, from)?;
        let source = from_parent.join(&from_name);
        let source_meta = source.symlink_metadata().map_err(io)?;
        let to = inside(to)?;
        let (to_parent, to_name) = split_last(&to)?;
        let source_relative = normalise(from)?;
        if to == source_relative {
            return Err(fail("invalid_path", "It is already there."));
        }
        if to.starts_with(&format!("{source_relative}/")) {
            return Err(fail("invalid_path", "A folder cannot move into itself."));
        }
        let parent = ensure_folder(&root, to_parent)?;
        let target = parent.join(to_name);
        if let Ok(meta) = target.symlink_metadata() {
            if !replace {
                return Err(already_exists());
            }
            if source.starts_with(&target) {
                return Err(fail(
                    "invalid_path",
                    "A folder cannot be replaced by something inside it.",
                ));
            }
            // A file over a file is replaced atomically by the rename below.
            let atomic = !copy && meta.is_file() && source_meta.is_file();
            if !atomic {
                if meta.is_dir() {
                    std::fs::remove_dir_all(&target).map_err(io)?;
                } else {
                    std::fs::remove_file(&target).map_err(io)?;
                }
            }
        }
        if copy {
            copy_entry(&source, &target)?;
        } else {
            std::fs::rename(&source, &target).map_err(io)?;
        }
        Ok(MovedFileDto { path: to })
    }

    /// `fs.delete`: a file (only at `expected`, when given), a link, or a
    /// folder with everything in it.
    pub fn delete(
        &self,
        project: Uuid,
        relative: &str,
        expected: Option<&str>,
    ) -> Result<(), DaemonError> {
        let root = self.writable(project)?;
        let (parent, name) = entry(&root, relative)?;
        let target = parent.join(name);
        let meta = target.symlink_metadata().map_err(io)?;
        if meta.is_dir() {
            return std::fs::remove_dir_all(&target).map_err(io);
        }
        if let Some(expected) = expected
            && meta.is_file()
            && file_version(&target)?.as_deref() != Some(expected)
        {
            return Err(fail(
                "file_changed",
                "The file changed since you last saw it.",
            ));
        }
        std::fs::remove_file(&target).map_err(io)
    }

    /// The real folder of `project`, for watching its files.
    pub fn real_root(&self, project: Uuid) -> Result<PathBuf, DaemonError> {
        let (root, _) = self.root(project)?;
        std::fs::canonicalize(&root).map_err(|_| folder_missing())
    }
}

/// A file's version: the first 16 bytes of its content's SHA-256, in hex.
/// Equal content is the same version, whenever it was written.
pub fn version_of(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes)[..16])
}

/// The version of the file at `path`, read in pieces; `None` when it is
/// gone or not a file.
pub fn file_version(path: &Path) -> Result<Option<String>, DaemonError> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    if !file.metadata().map_err(io)?.is_file() {
        return Ok(None);
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Some(hex(&hasher.finalize()[..16])))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// The start of a file too large to send whole: whole lines of valid
/// UTF-8.
fn first_part(mut bytes: Vec<u8>) -> String {
    let valid = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error) => error.valid_up_to(),
    };
    bytes.truncate(valid);
    if let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') {
        bytes.truncate(end + 1);
    }
    String::from_utf8(bytes).unwrap_or_default()
}

/// A path inside the project, in canonical form; absolute paths and
/// `..` are refused.
fn inside(relative: &str) -> Result<String, DaemonError> {
    if relative.starts_with('/') {
        return Err(outside());
    }
    normalise(relative)
}

/// A relative path's folder part and its last part; the root itself has
/// none.
fn split_last(relative: &str) -> Result<(&str, &str), DaemonError> {
    if relative.is_empty() {
        return Err(fail(
            "invalid_path",
            "Name a file or folder inside the project.",
        ));
    }
    Ok(relative.rsplit_once('/').unwrap_or(("", relative)))
}

/// The real folder an entry is in (inside `root`), and the entry's name:
/// the entry itself is not followed, so a link is the link.
fn entry(root: &Path, relative: &str) -> Result<(PathBuf, String), DaemonError> {
    let relative = inside(relative)?;
    let (parent, name) = split_last(&relative)?;
    let folder = if parent.is_empty() {
        root.to_path_buf()
    } else {
        std::fs::canonicalize(root.join(parent)).map_err(io)?
    };
    if !folder.starts_with(root) {
        return Err(outside());
    }
    if !folder.is_dir() {
        return Err(fail("not_a_folder", "That is not in a folder."));
    }
    Ok((folder, name.to_owned()))
}

/// The real folder at `relative` inside `root` (real), made with its
/// missing parents; each existing step must stay inside.
fn ensure_folder(root: &Path, relative: &str) -> Result<PathBuf, DaemonError> {
    let mut current = root.to_path_buf();
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        let next = current.join(part);
        match next.symlink_metadata() {
            Ok(_) => {
                let real = std::fs::canonicalize(&next).map_err(io)?;
                if !real.starts_with(root) {
                    return Err(outside());
                }
                if !real.is_dir() {
                    return Err(fail("not_a_folder", format!("{part} is not a folder.")));
                }
                current = real;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&next).map_err(io)?;
                current = next;
            }
            Err(error) => return Err(io(error)),
        }
    }
    Ok(current)
}

/// Copies a file, a link (as a link) or a folder with everything in it to
/// `target`, which must not exist.
fn copy_entry(source: &Path, target: &Path) -> Result<(), DaemonError> {
    let meta = source.symlink_metadata().map_err(io)?;
    if meta.is_symlink() {
        let points = std::fs::read_link(source).map_err(io)?;
        #[cfg(unix)]
        return std::os::unix::fs::symlink(points, target).map_err(created);
        #[cfg(windows)]
        return if source.is_dir() {
            std::os::windows::fs::symlink_dir(points, target).map_err(created)
        } else {
            std::os::windows::fs::symlink_file(points, target).map_err(created)
        };
    }
    if meta.is_dir() {
        std::fs::create_dir(target).map_err(created)?;
        for child in std::fs::read_dir(source).map_err(io)? {
            let child = child.map_err(io)?;
            copy_entry(&child.path(), &target.join(child.file_name()))?;
        }
        return Ok(());
    }
    // A new file only: never over one that appeared meanwhile.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(created)?;
    std::fs::copy(source, target).map_err(io)?;
    Ok(())
}

fn already_exists() -> DaemonError {
    fail(
        "already_exists",
        "Something with that name is already there.",
    )
}

fn folder_missing() -> DaemonError {
    fail(
        "folder_missing",
        "The project's folder is gone from this machine.",
    )
}

/// An error making something new: one already there is `already_exists`.
fn created(error: std::io::Error) -> DaemonError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        already_exists()
    } else {
        io(error)
    }
}

/// `fs.home`: the daemon user's home folder.
pub fn home() -> Result<HomeDto, DaemonError> {
    directories::BaseDirs::new()
        .map(|dirs| HomeDto {
            path: dirs.home_dir().display().to_string(),
        })
        .ok_or_else(|| fail("not_found", "This user has no home folder."))
}

/// `fs.listDirectories`: the subfolders of an absolute folder; names only.
pub fn list_directories(path: &str, hidden: bool) -> Result<FolderListingDto, DaemonError> {
    let folder = absolute(path)?;
    let mut folders = Vec::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(&folder).map_err(io)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_hidden = name.starts_with('.');
        if is_hidden && !hidden {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if folders.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        folders.push(FolderDto {
            is_git_repo: path.join(".git").exists(),
            path: path.display().to_string(),
            hidden: is_hidden,
            name,
        });
    }
    folders.sort_by_key(|folder| folder.name.to_lowercase());
    Ok(FolderListingDto {
        parent: folder.parent().map(|p| p.display().to_string()),
        path: folder.display().to_string(),
        folders,
        truncated,
    })
}

/// `fs.checkFolder`: whether an absolute folder can be a project.
pub fn check_folder(path: &str) -> Result<FolderCheckDto, DaemonError> {
    let folder = absolute(path)?;
    let meta = std::fs::metadata(&folder).ok();
    let is_dir = meta.as_ref().is_some_and(std::fs::Metadata::is_dir);
    Ok(FolderCheckDto {
        exists: meta.is_some(),
        is_dir,
        readable: is_dir && std::fs::read_dir(&folder).is_ok(),
        writable: is_dir && rustix::fs::access(&folder, rustix::fs::Access::WRITE_OK).is_ok(),
        real_path: std::fs::canonicalize(&folder)
            .ok()
            .map(|real| real.display().to_string()),
    })
}

/// A picker path: absolute, without `..`.
fn absolute(path: &str) -> Result<PathBuf, DaemonError> {
    let path = Path::new(path);
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(fail("invalid_path", "Give an absolute path without `..`."));
    }
    Ok(path.to_path_buf())
}

/// A relative path in canonical form: `a/b`, empty for the root.
fn normalise(relative: &str) -> Result<String, DaemonError> {
    let mut parts = Vec::new();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(outside()),
            part if part.contains('\\') || part.contains('\0') => {
                return Err(fail("invalid_path", "The path has invalid characters."));
            }
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

fn outside() -> DaemonError {
    fail(
        "path_outside_folder",
        "The path leads outside the project's folder.",
    )
}

/// The real project folder, and the real path of `relative` inside it.
fn resolve(root: &Path, relative: &str) -> Result<(PathBuf, PathBuf), DaemonError> {
    if relative.starts_with('/') {
        return Err(outside());
    }
    let relative = normalise(relative)?;
    let root = std::fs::canonicalize(root).map_err(|_| folder_missing())?;
    let candidate = if relative.is_empty() {
        root.clone()
    } else {
        root.join(&relative)
    };
    let real = std::fs::canonicalize(&candidate).map_err(io)?;
    if !real.starts_with(&root) {
        return Err(outside());
    }
    Ok((root, real))
}

fn join(base: &str, name: &str) -> String {
    let base = base.trim_matches('/');
    if base.is_empty() {
        name.to_owned()
    } else {
        format!("{base}/{name}")
    }
}

fn io(error: std::io::Error) -> DaemonError {
    match error.kind() {
        std::io::ErrorKind::NotFound => fail("not_found", "There is no such file or folder."),
        std::io::ErrorKind::PermissionDenied => fail(
            "permission_denied",
            "This user may not read or change that.",
        ),
        _ => fail("io_error", error.to_string()),
    }
}

fn utc(time: SystemTime) -> OffsetDateTime {
    OffsetDateTime::from(time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::projects::files::api::ProjectGrant;

    fn grants(root: &Path, read_only: bool) -> (Grants, Uuid) {
        let id = Uuid::now_v7();
        let grants = Grants::default();
        let answer = grants.sync(SyncProjects {
            projects: vec![ProjectGrant {
                id,
                path: root.display().to_string(),
                read_only,
            }],
        });
        assert!(answer.missing.is_empty());
        (grants, id)
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_keeps_links_as_links_and_a_replace_never_eats_its_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("a/inner")).unwrap();
        std::fs::write(root.join("a/inner/f.txt"), "f").unwrap();
        std::os::unix::fs::symlink("inner/f.txt", root.join("a/link")).unwrap();
        let (grants, id) = grants(&root, false);
        grants.move_entry(id, "a", "b", true, false).unwrap();
        assert_eq!(
            std::fs::read_link(root.join("b/link")).unwrap(),
            Path::new("inner/f.txt")
        );
        assert_eq!(
            std::fs::read_to_string(root.join("b/inner/f.txt")).unwrap(),
            "f"
        );
        assert!(root.join("a/inner/f.txt").exists());
        // Replacing a folder with a file, and refusing to replace an ancestor.
        std::fs::write(root.join("g.txt"), "g").unwrap();
        grants.move_entry(id, "g.txt", "b", false, true).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("b")).unwrap(), "g");
        assert!(grants.move_entry(id, "a/inner", "a", false, true).is_err());
        assert!(root.join("a/inner/f.txt").exists());
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("ws");
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("README.md"), "# hi\n").unwrap();
        std::fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(root.join("blob.bin"), [0u8, 159, 146, 150]).unwrap();
        std::fs::write(dir.path().join("outside.txt"), "nope").unwrap();
        dir
    }

    #[test]
    fn lists_folders_first_then_files() {
        let dir = tree();
        let (grants, id) = grants(&dir.path().join("ws"), false);
        let root = grants.list(id, "").unwrap();
        let names: Vec<_> = root.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src", ".env", "blob.bin", "README.md"]);
        assert!(root.entries[1].hidden);
        let src = grants.list(id, "src").unwrap();
        assert_eq!(src.entries[1].path, "src/main.rs");
        assert_eq!(src.entries[1].size, Some(13));
    }

    #[test]
    fn reads_text_and_marks_binary_files() {
        let dir = tree();
        let (grants, id) = grants(&dir.path().join("ws"), true);
        let text = grants.read(id, "src/main.rs", false).unwrap();
        assert_eq!(text.content.as_deref(), Some("fn main() {}\n"));
        assert!(!text.binary);
        let blob = grants.read(id, "blob.bin", false).unwrap();
        assert!(blob.binary);
        assert_eq!(blob.content, None);
        assert!(grants.is_read_only(id).unwrap());
    }

    #[test]
    fn reads_image_bytes_only_inside_the_project_and_within_the_limit() {
        use base64::Engine as _;
        let dir = tree();
        let root = dir.path().join("ws");
        let (grants, id) = grants(&root, true);
        let bytes = [0, 255, 31];
        std::fs::write(root.join("shot.png"), bytes).unwrap();
        assert_eq!(
            grants.image(id, "shot.png").unwrap(),
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        assert!(grants.image(id, "../outside.png").is_err());
        std::fs::File::create(root.join("large.png"))
            .unwrap()
            .set_len(10 * 1024 * 1024 + 1)
            .unwrap();
        assert_eq!(
            grants.image(id, "large.png").unwrap_err().code,
            "file_too_large"
        );
    }

    #[test]
    fn refuses_every_way_out_of_the_project() {
        let dir = tree();
        let (grants, id) = grants(&dir.path().join("ws"), false);
        for path in [
            "../outside.txt",
            "src/../../outside.txt",
            "/etc/passwd",
            "src\\..\\x",
        ] {
            let error = grants.read(id, path, false).unwrap_err();
            assert!(
                ["path_outside_folder", "invalid_path"].contains(&error.code),
                "{path}: {error:?}"
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                dir.path().join("outside.txt"),
                dir.path().join("ws/escape"),
            )
            .unwrap();
            assert_eq!(
                grants.read(id, "escape", false).unwrap_err().code,
                "path_outside_folder"
            );
            let listed = grants.list(id, "").unwrap();
            let escape = listed.entries.iter().find(|e| e.name == "escape").unwrap();
            assert_eq!(escape.kind, FileKindDto::Other);
        }
        assert_eq!(
            grants
                .read(Uuid::now_v7(), "README.md", false)
                .unwrap_err()
                .code,
            "project_unknown"
        );
    }

    #[test]
    fn large_files_are_not_sent() {
        let dir = tree();
        let root = dir.path().join("ws");
        let big = vec![b'a'; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
        std::fs::write(root.join("big.txt"), big).unwrap();
        let (grants, id) = grants(&root, false);
        assert_eq!(
            grants.read(id, "big.txt", false).unwrap_err().code,
            "file_too_large"
        );
    }

    #[test]
    fn saves_only_over_the_version_it_started_from() {
        let dir = tree();
        let root = dir.path().join("ws");
        let (grants, id) = grants(&root, false);
        let read = grants.read(id, "src/main.rs", false).unwrap();
        let start = read.version.unwrap();
        assert_eq!(start, version_of(b"fn main() {}\n"));
        let saved = grants
            .write(id, "src/main.rs", "fn main() { go(); }\n", Some(&start))
            .unwrap();
        assert_eq!(saved.version, version_of(b"fn main() { go(); }\n"));
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
            "fn main() { go(); }\n"
        );
        // Another save from the old version is refused, and changes nothing.
        let refused = grants
            .write(id, "src/main.rs", "stale", Some(&start))
            .unwrap_err();
        assert_eq!(refused.code, "file_changed");
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
            "fn main() { go(); }\n"
        );
        // Save anyway.
        grants.write(id, "src/main.rs", "mine", None).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).unwrap(),
            "mine"
        );
        // A file deleted meanwhile: refused with a version, made without.
        std::fs::remove_file(root.join("README.md")).unwrap();
        let gone = grants
            .write(id, "README.md", "# back", Some(&version_of(b"# hi\n")))
            .unwrap_err();
        assert_eq!(gone.code, "file_changed");
        grants.write(id, "README.md", "# back", None).unwrap();
        // Nothing left behind but the files.
        let names: Vec<_> = std::fs::read_dir(root.join("src"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_save_keeps_the_files_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tree();
        let root = dir.path().join("ws");
        std::fs::write(root.join("run.sh"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(root.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let (grants, id) = grants(&root, false);
        grants
            .write(id, "run.sh", "#!/bin/sh\necho\n", None)
            .unwrap();
        let mode = std::fs::metadata(root.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn read_only_projects_refuse_every_change() {
        let dir = tree();
        let root = dir.path().join("ws");
        let (grants, id) = grants(&root, true);
        let codes = [
            grants.write(id, "README.md", "x", None).unwrap_err().code,
            grants
                .create(id, "new.txt", NewKindDto::File, None)
                .unwrap_err()
                .code,
            grants.rename(id, "README.md", "R.md").unwrap_err().code,
            grants.delete(id, "README.md", None).unwrap_err().code,
        ];
        assert!(codes.iter().all(|code| *code == "read_only"), "{codes:?}");
        assert_eq!(
            std::fs::read_to_string(root.join("README.md")).unwrap(),
            "# hi\n"
        );
    }

    #[test]
    fn creates_renames_and_deletes_inside_only() {
        let dir = tree();
        let root = dir.path().join("ws");
        let (grants, id) = grants(&root, false);
        let made = grants
            .create(
                id,
                "docs/guide/intro.md",
                NewKindDto::File,
                Some("# Intro\n"),
            )
            .unwrap();
        assert_eq!(made.path, "docs/guide/intro.md");
        assert_eq!(made.kind, FileKindDto::File);
        assert_eq!(
            std::fs::read_to_string(root.join("docs/guide/intro.md")).unwrap(),
            "# Intro\n"
        );
        assert_eq!(
            grants
                .create(id, "docs/guide/intro.md", NewKindDto::File, None)
                .unwrap_err()
                .code,
            "already_exists"
        );
        grants
            .create(id, "empty", NewKindDto::Folder, None)
            .unwrap();
        assert!(root.join("empty").is_dir());

        let moved = grants
            .rename(id, "docs/guide/intro.md", "docs/start.md")
            .unwrap();
        assert_eq!(moved.path, "docs/start.md");
        assert!(root.join("docs/start.md").is_file());
        assert_eq!(
            grants
                .rename(id, "docs/start.md", "README.md")
                .unwrap_err()
                .code,
            "already_exists"
        );
        assert_eq!(
            grants.rename(id, "docs", "docs/inner").unwrap_err().code,
            "invalid_path"
        );
        assert_eq!(
            grants
                .rename(id, "nothing.txt", "else.txt")
                .unwrap_err()
                .code,
            "not_found"
        );

        // Deleting at a version, and a folder with what is in it.
        assert_eq!(
            grants
                .delete(id, "docs/start.md", Some("0000"))
                .unwrap_err()
                .code,
            "file_changed"
        );
        grants
            .delete(id, "docs/start.md", Some(&version_of(b"# Intro\n")))
            .unwrap();
        grants.delete(id, "docs", None).unwrap();
        assert!(!root.join("docs").exists());

        // Never outside, nor the folder itself.
        for path in ["../outside.txt", "/etc/x", "src/../../outside.txt"] {
            let codes = [
                grants.write(id, path, "x", None).unwrap_err().code,
                grants
                    .create(id, path, NewKindDto::File, None)
                    .unwrap_err()
                    .code,
                grants.delete(id, path, None).unwrap_err().code,
                grants.rename(id, "README.md", path).unwrap_err().code,
            ];
            assert!(
                codes
                    .iter()
                    .all(|code| ["path_outside_folder", "invalid_path"].contains(code)),
                "{path}: {codes:?}"
            );
        }
        assert_eq!(
            grants.delete(id, "", None).unwrap_err().code,
            "invalid_path"
        );
        assert!(dir.path().join("outside.txt").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert_eq!(
                grants
                    .create(id, "up/planted.txt", NewKindDto::File, None)
                    .unwrap_err()
                    .code,
                "path_outside_folder"
            );
            assert_eq!(
                grants
                    .write(id, "up/outside.txt", "x", None)
                    .unwrap_err()
                    .code,
                "path_outside_folder"
            );
            // Deleting the link removes the link, not what it points to.
            grants.delete(id, "up", None).unwrap();
            assert!(dir.path().join("outside.txt").exists());
        }
    }

    #[test]
    fn a_large_file_is_sent_in_part_when_asked() {
        let dir = tree();
        let root = dir.path().join("ws");
        let line = "a line of text\n";
        let big = line.repeat(usize::try_from(MAX_FILE_BYTES).unwrap() / line.len() + 10);
        std::fs::write(root.join("big.log"), &big).unwrap();
        let (grants, id) = grants(&root, false);
        let part = grants.read(id, "big.log", true).unwrap();
        assert!(part.truncated);
        assert_eq!(part.version, None);
        let text = part.content.unwrap();
        assert!(text.len() as u64 <= MAX_FILE_BYTES);
        assert!(text.ends_with('\n'));
        let small = grants.read(id, "README.md", true).unwrap();
        assert!(!small.truncated);
        assert!(small.version.is_some());
    }

    #[test]
    fn sync_reports_missing_folders() {
        let grants = Grants::default();
        let id = Uuid::now_v7();
        let answer = grants.sync(SyncProjects {
            projects: vec![ProjectGrant {
                id,
                path: "/definitely/not/here".into(),
                read_only: false,
            }],
        });
        assert_eq!(answer.missing, [id]);
    }

    #[test]
    fn the_picker_lists_only_folders() {
        let dir = tree();
        let listing = list_directories(&dir.path().display().to_string(), false).unwrap();
        let names: Vec<_> = listing.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["ws"]);
        assert!(list_directories("relative/path", false).is_err());
        let check = check_folder(&dir.path().join("ws").display().to_string()).unwrap();
        assert!(check.exists && check.is_dir && check.readable && check.writable);
        let file = check_folder(&dir.path().join("outside.txt").display().to_string()).unwrap();
        assert!(file.exists && !file.is_dir);
    }
}
