//! Exit codes, which tell a script or an agent what to do after a failure.

use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// Done.
    Success = 0,
    /// Any other error.
    Other = 1,
    /// The command was used wrongly.
    Usage = 2,
    /// The server rejected the change: fix it and try again.
    Rejected = 3,
    /// The caller may not do this: stop trying.
    NotAllowed = 4,
    /// A temporary problem: try again later.
    Temporary = 5,
}

impl Exit {
    /// The exit code for an error response from the API.
    pub fn from_status(status: u16) -> Self {
        match status {
            401 | 403 => Self::NotAllowed,
            409 | 422 => Self::Rejected,
            408 | 429 | 500..=599 => Self::Temporary,
            _ => Self::Other,
        }
    }
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        ExitCode::from(exit as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_error_statuses() {
        assert_eq!(Exit::from_status(401), Exit::NotAllowed);
        assert_eq!(Exit::from_status(403), Exit::NotAllowed);
        assert_eq!(Exit::from_status(409), Exit::Rejected);
        assert_eq!(Exit::from_status(422), Exit::Rejected);
        assert_eq!(Exit::from_status(429), Exit::Temporary);
        assert_eq!(Exit::from_status(503), Exit::Temporary);
        assert_eq!(Exit::from_status(404), Exit::Other);
    }
}
