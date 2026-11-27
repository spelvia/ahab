//! The current time, as Ahab stores it, a clock services can be given, and
//! how times are written in JSON.

use std::sync::{Arc, Mutex};

use time::format_description::BorrowedFormatItem;
use time::{Duration, OffsetDateTime, UtcOffset};

/// The current time in UTC, to the microsecond: what Postgres keeps (SQLite
/// keeps more), so a saved value equals the one a service returns on either.
pub fn now() -> OffsetDateTime {
    to_micros(OffsetDateTime::now_utc())
}

/// A time in UTC, cut to the microsecond.
fn to_micros(time: OffsetDateTime) -> OffsetDateTime {
    let time = time.to_offset(UtcOffset::UTC);
    let nanos = time.nanosecond();
    time.replace_nanosecond(nanos - nanos % 1_000)
        .unwrap_or(time)
}

/// A time as the database keeps it: microseconds since the Unix epoch, in
/// UTC. Integers sort and compare in SQL on SQLite and Postgres alike; any
/// finer fraction of a second is dropped.
pub fn to_unix_micros(time: OffsetDateTime) -> i64 {
    let micros = time.unix_timestamp_nanos().div_euclid(1_000);
    // Every time `time` can hold fits: they span ±9999 years, i64 ±292,000.
    i64::try_from(micros).unwrap_or(if micros < 0 { i64::MIN } else { i64::MAX })
}

/// A time the database keeps ([`to_unix_micros`]), in UTC; `None` when it is
/// beyond what a time can hold.
pub fn from_unix_micros(micros: i64) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(micros) * 1_000).ok()
}

/// A time in UTC written with `format`, for people to read; empty if the
/// time cannot be written that way.
pub fn display(time: OffsetDateTime, format: &[BorrowedFormatItem<'_>]) -> String {
    time.to_offset(UtcOffset::UTC)
        .format(format)
        .unwrap_or_default()
}

/// Where services get the time from, so tests can fix it.
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// The real time.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        now()
    }
}

/// A time that only moves when told to, for tests.
#[derive(Debug, Clone)]
pub struct FixedClock(Arc<Mutex<OffsetDateTime>>);

impl FixedClock {
    pub fn at(time: OffsetDateTime) -> Self {
        Self(Arc::new(Mutex::new(to_micros(time))))
    }

    /// Moves the time forward.
    pub fn advance(&self, by: Duration) {
        if let Ok(mut time) = self.0.lock() {
            *time += by;
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0.lock().map_or_else(|_| now(), |time| *time)
    }
}

/// Times in JSON as RFC 3339 strings in UTC, written as Ahab always has:
/// `Z` for the offset and the fraction of a second in 3, 6 or 9 digits, only
/// as many as it needs (`2026-11-16T16:46:46.718432Z`). Any RFC 3339 time is
/// read, whatever its offset. Use as `#[serde(with = "crate::shared::time::rfc3339")]`.
pub mod rfc3339 {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::format_description::well_known::Rfc3339;
    use time::macros::format_description;
    use time::{OffsetDateTime, UtcOffset};

    /// Writes `time` as Ahab's RFC 3339 string.
    pub fn format(time: OffsetDateTime) -> Result<String, time::error::Format> {
        let time = time.to_offset(UtcOffset::UTC);
        let mut text = time.format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]"
        ))?;
        let nanos = time.nanosecond();
        if nanos.is_multiple_of(1_000_000) {
            if nanos != 0 {
                text.push_str(&format!(".{:03}", nanos / 1_000_000));
            }
        } else if nanos.is_multiple_of(1_000) {
            text.push_str(&format!(".{:06}", nanos / 1_000));
        } else {
            text.push_str(&format!(".{nanos:09}"));
        }
        text.push('Z');
        Ok(text)
    }

    /// Reads an RFC 3339 string, in UTC.
    pub fn parse(text: &str) -> Result<OffsetDateTime, time::error::Parse> {
        Ok(OffsetDateTime::parse(text, &Rfc3339)?.to_offset(UtcOffset::UTC))
    }

    pub fn serialize<S: Serializer>(
        time: &OffsetDateTime,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let text = format(*time).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<OffsetDateTime, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        parse(&text).map_err(serde::de::Error::custom)
    }

    /// The same for an optional time, `null` when there is none. Use with
    /// `#[serde(default, with = "crate::shared::time::rfc3339::option")]`.
    pub mod option {
        use serde::{Deserialize, Deserializer, Serializer};
        use time::OffsetDateTime;
        pub fn serialize<S: Serializer>(
            time: &Option<OffsetDateTime>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match time {
                Some(time) => super::serialize(time, serializer),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<OffsetDateTime>, D::Error> {
            Option::<std::borrow::Cow<'de, str>>::deserialize(deserializer)?
                .map(|text| super::parse(&text).map_err(serde::de::Error::custom))
                .transpose()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use time::macros::datetime;

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct At {
        #[serde(with = "rfc3339")]
        at: OffsetDateTime,
        #[serde(default, with = "rfc3339::option")]
        maybe: Option<OffsetDateTime>,
    }

    #[test]
    fn writes_times_as_ahab_always_has() {
        let cases = [
            (datetime!(2026-11-16 16:46:46 UTC), "2026-11-16T16:46:46Z"),
            (
                datetime!(2026-11-16 16:46:46.5 UTC),
                "2026-11-16T16:46:46.500Z",
            ),
            (
                datetime!(2026-11-16 16:46:46.84426 UTC),
                "2026-11-16T16:46:46.844260Z",
            ),
            (
                datetime!(2026-11-16 16:46:46.718432577 UTC),
                "2026-11-16T16:46:46.718432577Z",
            ),
            (
                datetime!(2026-11-16 17:46:46 +01:00),
                "2026-11-16T16:46:46Z",
            ),
        ];
        for (time, text) in cases {
            assert_eq!(rfc3339::format(time).unwrap(), text);
        }
    }

    #[test]
    fn reads_any_rfc3339_time_in_utc() {
        let read: At = serde_json::from_str(
            r#"{"at": "2026-11-16T17:46:46.844260+01:00", "maybe": "2026-11-16T16:46:46Z"}"#,
        )
        .unwrap();
        assert_eq!(read.at, datetime!(2026-11-16 16:46:46.84426 UTC));
        assert_eq!(read.at.offset(), UtcOffset::UTC);
        assert_eq!(read.maybe, Some(datetime!(2026-11-16 16:46:46 UTC)));

        let missing: At = serde_json::from_str(r#"{"at": "2026-11-16T16:46:46Z"}"#).unwrap();
        assert_eq!(missing.maybe, None);
        let null: At =
            serde_json::from_str(r#"{"at": "2026-11-16T16:46:46Z", "maybe": null}"#).unwrap();
        assert_eq!(null.maybe, None);
        assert_eq!(
            serde_json::to_string(&null).unwrap(),
            r#"{"at":"2026-11-16T16:46:46Z","maybe":null}"#
        );
    }

    #[test]
    fn keeps_times_as_microseconds_since_the_epoch() {
        let time = datetime!(2026-11-16 17:46:46.844260789 +01:00);
        let micros = to_unix_micros(time);
        assert_eq!(micros, 1_794_847_606_844_260);
        assert_eq!(
            from_unix_micros(micros),
            Some(datetime!(2026-11-16 16:46:46.84426 UTC))
        );
        let before_1970 = datetime!(1969-12-31 23:59:59.9999995 UTC);
        assert_eq!(to_unix_micros(before_1970), -1);
        assert_eq!(from_unix_micros(i64::MAX), None);
    }

    #[test]
    fn now_is_to_the_microsecond() {
        assert_eq!(now().nanosecond() % 1_000, 0);
        let clock = FixedClock::at(datetime!(2026-11-13 9:00:00.123456789 UTC));
        assert_eq!(clock.now(), datetime!(2026-11-13 9:00:00.123456 UTC));
        clock.advance(Duration::minutes(16));
        assert_eq!(clock.now(), datetime!(2026-11-13 9:16:00.123456 UTC));
    }
}
