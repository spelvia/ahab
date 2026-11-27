//! Keeping attachments on this machine, under the data folder:
//! `attachments/<id>/<name>`, outside every project.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use uuid::Uuid;

use crate::modules::sessions::attachment::api::{AttachmentDto, GetAttachment, PutAttachment};
use crate::modules::sessions::attachment::model::{self, MAX_BYTES, MAX_PER_MESSAGE};
use crate::shared::error::{DaemonError, fail};

fn not_found() -> DaemonError {
    fail(
        "attachment_not_found",
        "There is no such attachment on this machine.",
    )
}

/// A file ready for an agent: its path, and its image type if it is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    pub path: PathBuf,
    pub image: Option<&'static str>,
}

/// The attachments folder.
#[derive(Debug, Clone)]
pub struct Attachments {
    root: PathBuf,
}

impl Attachments {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("attachments"),
        }
    }

    /// The folder agents may read without asking, created if missing.
    pub fn readable_root(&self) -> PathBuf {
        if let Err(error) = std::fs::create_dir_all(&self.root) {
            tracing::warn!(%error, folder = %self.root.display(), "the attachments folder cannot be created");
        }
        self.root.clone()
    }

    /// Keeps a file; its name is made safe.
    pub fn put(&self, file: &PutAttachment) -> Result<AttachmentDto, DaemonError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&file.data)
            .map_err(|_| fail("invalid_params", "The attachment is not base64."))?;
        if bytes.len() > MAX_BYTES {
            return Err(fail(
                "attachment_too_large",
                "Attachments are at most 10 MiB.",
            ));
        }
        let id = Uuid::now_v7();
        let name = model::safe_name(&file.name);
        let folder = self.root.join(id.to_string());
        let path = folder.join(&name);
        let written = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&path, &bytes));
        if let Err(error) = written {
            return Err(fail(
                "attachment_unwritable",
                format!("The attachment cannot be saved: {error}."),
            ));
        }
        Ok(AttachmentDto {
            id,
            content_type: model::content_type(&name).to_owned(),
            path: path.display().to_string(),
            size: bytes.len() as u64,
            name,
        })
    }

    /// A kept file's bytes.
    pub fn get(&self, file: &GetAttachment) -> Result<Vec<u8>, DaemonError> {
        if model::safe_name(&file.name) != file.name {
            return Err(not_found());
        }
        std::fs::read(self.root.join(file.id.to_string()).join(&file.name)).map_err(|_| not_found())
    }

    /// The files of a message, by ID, for its agent.
    pub fn resolve(&self, ids: &[Uuid]) -> Result<Vec<Attached>, DaemonError> {
        if ids.len() > MAX_PER_MESSAGE {
            return Err(fail(
                "too_many_attachments",
                "Attach at most 10 files to a message.",
            ));
        }
        ids.iter()
            .map(|id| {
                let folder = self.root.join(id.to_string());
                let path = std::fs::read_dir(&folder)
                    .map_err(|_| not_found())?
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| path.is_file())
                    .ok_or_else(not_found)?;
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                Ok(Attached {
                    image: model::image_type(name),
                    path,
                })
            })
            .collect()
    }
}

/// What the agent receives: the text listing every file, and the images.
pub fn input(text: &str, files: &[Attached]) -> stubb::turn::Input {
    let paths: Vec<String> = files
        .iter()
        .map(|file| file.path.display().to_string())
        .collect();
    let images = files
        .iter()
        .filter_map(|file| {
            file.image
                .map(|kind| stubb::turn::Image::new(&file.path, kind))
        })
        .collect();
    stubb::turn::Input::text(model::message(text, &paths)).with_images(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(store: &Attachments, name: &str, bytes: &[u8]) -> AttachmentDto {
        store
            .put(&PutAttachment {
                name: name.into(),
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            })
            .unwrap()
    }

    #[test]
    fn files_are_kept_found_and_given_to_agents() {
        let dir = std::env::temp_dir().join(format!("ahab-attachments-{}", Uuid::now_v7()));
        let store = Attachments::new(&dir);
        let shot = put(&store, "../shot.png", b"png");
        let notes = put(&store, "notes.txt", b"hello");
        assert_eq!(shot.name, "shot.png");
        assert_eq!(shot.size, 3);
        assert!(shot.path.starts_with(&dir.display().to_string()));
        assert_eq!(
            store
                .get(&GetAttachment {
                    id: notes.id,
                    name: "notes.txt".into()
                })
                .unwrap(),
            b"hello"
        );
        assert!(
            store
                .get(&GetAttachment {
                    id: notes.id,
                    name: "../x".into()
                })
                .is_err()
        );

        let files = store.resolve(&[shot.id, notes.id]).unwrap();
        let input = input("look", &files);
        assert_eq!(input.images.len(), 1);
        assert_eq!(input.images[0].mime_type, "image/png");
        assert!(input.text.starts_with("look\n\n[Attached files]\n"));
        assert!(input.text.ends_with("notes.txt"));
        assert_eq!(
            store.resolve(&[Uuid::now_v7()]).unwrap_err().code,
            "attachment_not_found"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
