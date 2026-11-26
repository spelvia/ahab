//! A repository's or worktree's use, and the rule for being active.

use time::{Duration, OffsetDateTime};

/// One repository or worktree of a project, as kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    /// The repository's folder, relative to the project (`""` for the
    /// project's own).
    pub repository: String,
    /// The worktree's folder, when it is one.
    pub worktree: Option<String>,
    pub last_active_at: Option<OffsetDateTime>,
    pub pinned: bool,
}

impl Activity {
    /// Pinned, or used within `days` of `now`.
    pub fn is_active(&self, now: OffsetDateTime, days: u32) -> bool {
        self.pinned
            || self
                .last_active_at
                .is_some_and(|at| now - at <= Duration::days(i64::from(days)))
    }

    /// The folder it stands for: the worktree's, else the repository's.
    pub fn folder(&self) -> &str {
        self.worktree.as_deref().unwrap_or(&self.repository)
    }
}

/// A folder named by a client: relative, without `..`.
pub fn valid_folder(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path.len() <= 4096
        && !path.split('/').any(|part| part == "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_when_pinned_or_used_lately() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(100);
        let used = |days_ago: i64, pinned: bool| Activity {
            repository: "app".into(),
            worktree: None,
            last_active_at: Some(now - Duration::days(days_ago)),
            pinned,
        };
        assert!(used(3, false).is_active(now, 14));
        assert!(used(14, false).is_active(now, 14));
        assert!(!used(15, false).is_active(now, 14));
        assert!(used(400, true).is_active(now, 14));
        let never = Activity {
            last_active_at: None,
            ..used(0, false)
        };
        assert!(!never.is_active(now, 14));
        assert!(valid_folder("app/main") && valid_folder(""));
        assert!(!valid_folder("../x") && !valid_folder("/etc"));
    }
}
