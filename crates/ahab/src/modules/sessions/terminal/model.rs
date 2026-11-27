//! A terminal: a process with a PTY in a project's folder, as the server
//! records it. Its output is never stored: only the terminal host
//! on its machine keeps it.

use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalState {
    Running,
    /// Its process ended (or the machine restarted); kept until archived.
    Exited,
}

impl TerminalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Exited => "exited",
        }
    }

    pub fn parse(text: &str) -> Self {
        if text == "running" {
            Self::Running
        } else {
            Self::Exited
        }
    }
}

/// What a new terminal is to be.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewTerminal {
    pub title: Option<String>,
    /// A command for the login shell to run; the shell itself otherwise.
    pub command: Option<String>,
    /// The project's folder to file it in.
    pub folder: Option<Uuid>,
    pub cols: u16,
    pub rows: u16,
    /// The plugin starting it.
    pub plugin: Option<String>,
    /// What it is for, by plugin (`{"docker":{"container":"web"}}`): the
    /// `terminals.create` hook reads it.
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terminal {
    pub id: Uuid,
    /// The session it is in (one of a session's chats and terminals).
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub machine_id: Uuid,
    /// What it runs; the login shell when absent.
    pub command: Option<String>,
    pub state: TerminalState,
    /// How its process ended: its code; absent when it was not seen ending
    /// (the machine restarted).
    pub exit_code: Option<i32>,
    pub cols: u16,
    pub rows: u16,
    /// The title its shell set (the command running, the folder), if any.
    pub title: Option<String>,
    /// The name you gave it, if any (`terminal_labels`), over its title.
    pub label: Option<String>,
    /// The plugin that started it, if one did.
    pub plugin: Option<String>,
    /// What it was started for, by plugin.
    pub context: Option<serde_json::Value>,
    /// The plugins whose `terminals.create` hook changed how it started.
    pub hooked_by: Vec<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Terminal {
    /// It ended, with its code when known.
    pub fn exited(&mut self, code: Option<i32>, now: OffsetDateTime) {
        self.state = TerminalState::Exited;
        self.exit_code = code;
        self.updated_at = now;
    }

    /// It runs again.
    pub fn restarted(&mut self, now: OffsetDateTime) {
        self.state = TerminalState::Running;
        self.exit_code = None;
        self.updated_at = now;
    }
}

/// A size a terminal can have: at least 2 columns and 1 row, at most 1000
/// of either.
pub fn size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(2, 1000), rows.clamp(1, 1000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_stay_usable() {
        assert_eq!(size(0, 0), (2, 1));
        assert_eq!(size(120, 40), (120, 40));
        assert_eq!(size(5000, 5000), (1000, 1000));
    }
}
