//! Errors: a rule was broken (`DomainError`), storage failed (`RepoError`),
//! a use case failed (`ServiceError`), or something a daemon was asked to do
//! failed (`DaemonError`).

/// One invalid field. `field` is the name the API uses, with dots for nested
/// fields, such as `folder.path`; `message` is shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: std::borrow::Cow<'static, str>,
    pub message: String,
}

impl FieldError {
    pub fn new(
        field: impl Into<std::borrow::Cow<'static, str>>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// One or more fields are invalid.
    #[error("the input is invalid")]
    Invalid(Vec<FieldError>),
}

/// A storage problem, without the storage library's own types.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("storage unavailable: {0}")]
    Unavailable(String),
    /// Stored data that the model does not accept, such as an unknown status.
    #[error("stored data is invalid: {0}")]
    InvalidData(String),
}

/// Why a use case failed. Interfaces turn it into a response.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("the input is invalid")]
    Invalid(Vec<FieldError>),
    /// The named thing does not exist. `code` is the error code the API uses,
    /// such as `machine_not_found`.
    #[error("{message}")]
    NotFound { code: &'static str, message: String },
    #[error(transparent)]
    Storage(#[from] RepoError),
    /// The request clashes with what exists. `code` is the error code the API
    /// uses.
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    /// Something Ahab depends on cannot be reached right now.
    #[error("{message}")]
    Unavailable { code: &'static str, message: String },
    /// The caller is not who it needs to be.
    #[error("{message}")]
    Unauthorized { code: &'static str, message: String },
    /// The caller may not do this.
    #[error("{message}")]
    Forbidden { code: &'static str, message: String },
    /// Too many tries at a secret from one address (`too_many_attempts`):
    /// wait `retry_after` seconds.
    #[error("{message}")]
    TooManyAttempts { message: String, retry_after: u32 },
    /// A plugin's command failed, with the plugin's own code, such as
    /// `cluster_unreachable` (the API's code stays `command_failed`).
    #[error("{message}")]
    Plugin { code: String, message: String },
    /// Something that should not happen.
    #[error("{0}")]
    Internal(String),
}

impl From<DomainError> for ServiceError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::Invalid(errors) => Self::Invalid(errors),
        }
    }
}

/// Why something a daemon was asked to do failed (Git, files, search, a
/// chat, an attachment), with a stable code the server maps to its own.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct DaemonError {
    pub code: &'static str,
    pub message: String,
}

/// A [`DaemonError`] with its code and message.
pub fn fail(code: &'static str, message: impl Into<String>) -> DaemonError {
    DaemonError {
        code,
        message: message.into(),
    }
}
