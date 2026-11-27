//! What stores share to reach the database: one sqlx `Any` pool, on SQLite or
//! Postgres, and reading rows into Ahab's types.
//!
//! The `Any` driver knows only booleans, integers, floats, text and bytes, so
//! the stores convert at the edge: UUIDs are lowercase hyphenated text, times
//! are microseconds since the Unix epoch ([`to_unix_micros`](crate::shared::time::to_unix_micros)), JSON is
//! text. SQL uses `$1, $2…` placeholders, which both backends accept.
//!
//! A `BOOLEAN` column is selected as `CAST(column AS INTEGER) AS column` and
//! read with [`RowExt::flag`]: the `Any` driver cannot read a SQLite column
//! declared `BOOLEAN`, but reads the integer an expression gives.
//!
//! Each JSON column has a companion `<column>_format` integer: the shape its
//! JSON was written in. A store writes its current format (a constant next to
//! it) and reads [`RowExt::format`] before the JSON. A format it does not
//! know, written by a newer Ahab, is never misread: a cache or a layout is
//! treated as absent, a chat event is kept as it is and marked unreadable,
//! and anything else is an error ([`unknown_format`]).

use serde::de::DeserializeOwned;
use sqlx::any::AnyRow;
use sqlx::{Any, Decode, Row, Type};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::shared::error::RepoError;
use crate::shared::time::from_unix_micros;

/// The database: a pool of connections to SQLite or Postgres.
pub type Db = sqlx::AnyPool;

impl From<sqlx::Error> for RepoError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::ColumnDecode { .. }
            | sqlx::Error::Decode(_)
            | sqlx::Error::ColumnNotFound(_) => RepoError::InvalidData(error.to_string()),
            _ => RepoError::Unavailable(error.to_string()),
        }
    }
}

/// Fails unless a statement changed a row: an update of a row that is not
/// there. `what` names the row, such as `machine 0196…`.
pub fn changed_one(rows_affected: u64, what: impl std::fmt::Display) -> Result<(), RepoError> {
    if rows_affected == 0 {
        return Err(RepoError::Unavailable(format!("{what} is not stored")));
    }
    Ok(())
}

/// Reading a column of a row in Ahab's types, with the column named in the
/// error when its value does not fit.
pub trait RowExt {
    /// A value the driver reads as it is: text, an integer, bytes, or an
    /// option of one.
    fn value<'r, T: Decode<'r, Any> + Type<Any>>(&'r self, column: &str) -> Result<T, RepoError>;
    fn uuid(&self, column: &str) -> Result<Uuid, RepoError>;
    fn opt_uuid(&self, column: &str) -> Result<Option<Uuid>, RepoError>;
    fn time(&self, column: &str) -> Result<OffsetDateTime, RepoError>;
    fn opt_time(&self, column: &str) -> Result<Option<OffsetDateTime>, RepoError>;
    /// A `BOOLEAN` column, selected as an integer (see the module).
    fn flag(&self, column: &str) -> Result<bool, RepoError>;
    /// A JSON text column.
    fn json<T: DeserializeOwned>(&self, column: &str) -> Result<T, RepoError>;
    /// The format a JSON column's value was written in: its
    /// `<column>_format` (see the module).
    fn format(&self, column: &str) -> Result<i32, RepoError>;
}

fn invalid(column: &str, error: impl std::fmt::Display) -> RepoError {
    RepoError::InvalidData(format!("{column}: {error}"))
}

impl RowExt for AnyRow {
    fn value<'r, T: Decode<'r, Any> + Type<Any>>(&'r self, column: &str) -> Result<T, RepoError> {
        self.try_get(column).map_err(|error| invalid(column, error))
    }

    fn uuid(&self, column: &str) -> Result<Uuid, RepoError> {
        let text: String = self.value(column)?;
        Uuid::parse_str(&text).map_err(|error| invalid(column, error))
    }

    fn opt_uuid(&self, column: &str) -> Result<Option<Uuid>, RepoError> {
        self.value::<Option<String>>(column)?
            .map(|text| Uuid::parse_str(&text).map_err(|error| invalid(column, error)))
            .transpose()
    }

    fn time(&self, column: &str) -> Result<OffsetDateTime, RepoError> {
        let micros: i64 = self.value(column)?;
        from_unix_micros(micros).ok_or_else(|| invalid(column, "time out of range"))
    }

    fn opt_time(&self, column: &str) -> Result<Option<OffsetDateTime>, RepoError> {
        self.value::<Option<i64>>(column)?
            .map(|micros| {
                from_unix_micros(micros).ok_or_else(|| invalid(column, "time out of range"))
            })
            .transpose()
    }

    fn flag(&self, column: &str) -> Result<bool, RepoError> {
        match self.try_get::<i64, _>(column) {
            Ok(value) => Ok(value != 0),
            Err(_) => self.value(column),
        }
    }

    fn json<T: DeserializeOwned>(&self, column: &str) -> Result<T, RepoError> {
        let text: String = self.value(column)?;
        serde_json::from_str(&text).map_err(|error| invalid(column, error))
    }

    fn format(&self, column: &str) -> Result<i32, RepoError> {
        self.value(&format!("{column}_format"))
    }
}

/// A JSON column written in a format this Ahab does not know, by a newer one.
pub fn unknown_format(column: &str, format: i32) -> RepoError {
    invalid(
        column,
        format_args!("format {format} is newer than this Ahab reads"),
    )
}

/// SQL a store puts together from its own constant pieces (column lists,
/// filters, orders), never from input: values are always bound.
pub fn assembled(sql: String) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(sql)
}

/// JSON as stored in a text column.
pub fn json_text(value: &impl serde::Serialize) -> Result<String, RepoError> {
    serde_json::to_string(value).map_err(|error| RepoError::InvalidData(error.to_string()))
}
