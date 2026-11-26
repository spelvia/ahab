//! App settings (`settings.json`) and the windows to restore
//! (`windows.json`), in the app's data folder. Neither holds a secret.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::secrets::write_private;

/// What App settings changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Keep the app in the tray when its last window closes. None: on
    /// when Ahab runs on this computer.
    pub keep_in_tray: Option<bool>,
    /// The global shortcut that brings Ahab forward, such as
    /// `CommandOrControl+Shift+A`; none (off) by default.
    pub global_shortcut: Option<String>,
    /// Native notifications for what the web app shows.
    pub notifications: bool,
    /// Check for updates once releases are published; nothing to check until then.
    pub check_for_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keep_in_tray: None,
            global_shortcut: None,
            notifications: true,
            check_for_updates: true,
        }
    }
}

impl Settings {
    /// Whether closing the last window keeps the app in the tray.
    pub fn keeps_in_tray(&self, ahab_runs_here: bool) -> bool {
        self.keep_in_tray.unwrap_or(ahab_runs_here)
    }
}

/// One window to restore: its label (which the window-state plugin keeps
/// the size and position under) and what it showed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowRecord {
    pub label: String,
    pub server_id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Session {
    pub windows: Vec<WindowRecord>,
}

pub struct Files {
    dir: PathBuf,
}

impl Files {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
        }
    }

    fn read<T: Default + for<'de> Deserialize<'de>>(&self, name: &str) -> T {
        std::fs::read_to_string(self.dir.join(name))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) -> std::io::Result<()> {
        let text = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
        write_private(&self.dir.join(name), &text)
    }

    pub fn settings(&self) -> Settings {
        self.read("settings.json")
    }

    pub fn save_settings(&self, settings: &Settings) -> std::io::Result<()> {
        self.write("settings.json", settings)
    }

    pub fn session(&self) -> Session {
        self.read("windows.json")
    }

    pub fn save_session(&self, session: &Session) -> std::io::Result<()> {
        self.write("windows.json", session)
    }

    /// Records what a window shows now.
    pub fn record_window(&self, record: WindowRecord) -> std::io::Result<()> {
        let mut session = self.session();
        if let Some(existing) = session
            .windows
            .iter_mut()
            .find(|window| window.label == record.label)
        {
            if *existing == record {
                return Ok(());
            }
            *existing = record;
        } else {
            session.windows.push(record);
        }
        self.save_session(&session)
    }

    pub fn forget_window(&self, label: &str) -> std::io::Result<()> {
        let mut session = self.session();
        let before = session.windows.len();
        session.windows.retain(|window| window.label != label);
        if session.windows.len() == before {
            return Ok(());
        }
        self.save_session(&session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_in_the_tray_by_default_only_when_ahab_runs_here() {
        let settings = Settings::default();
        assert!(settings.keeps_in_tray(true));
        assert!(!settings.keeps_in_tray(false));
        let off = Settings {
            keep_in_tray: Some(false),
            ..Settings::default()
        };
        assert!(!off.keeps_in_tray(true));
        assert!(settings.global_shortcut.is_none(), "off by default");
    }

    #[test]
    fn remembers_settings_and_windows() {
        let dir = tempfile::tempdir().unwrap();
        let files = Files::new(dir.path());
        assert_eq!(files.settings(), Settings::default());
        let changed = Settings {
            global_shortcut: Some("CommandOrControl+Shift+A".into()),
            ..Settings::default()
        };
        files.save_settings(&changed).unwrap();
        assert_eq!(files.settings(), changed);

        let main = WindowRecord {
            label: "main".into(),
            server_id: Some("s1".into()),
            path: Some("/p/1".into()),
        };
        files.record_window(main.clone()).unwrap();
        files
            .record_window(WindowRecord {
                label: "window-2".into(),
                server_id: Some("s2".into()),
                path: None,
            })
            .unwrap();
        files
            .record_window(WindowRecord {
                path: Some("/p/2".into()),
                ..main
            })
            .unwrap();
        let session = files.session();
        assert_eq!(session.windows.len(), 2);
        assert_eq!(session.windows[0].path.as_deref(), Some("/p/2"));
        files.forget_window("window-2").unwrap();
        assert_eq!(files.session().windows.len(), 1);
    }

    #[test]
    fn reads_damaged_files_as_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), "{not json").unwrap();
        assert_eq!(Files::new(dir.path()).settings(), Settings::default());
    }
}
