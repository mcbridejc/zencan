//! Data types for TimeOfDay and TimeDifference fields

use chrono::{Datelike, NaiveDate, NaiveTime, TimeDelta, Timelike};
use core::time::Duration;
use snafu::Snafu;

const MILLIS_PER_DAY: u64 = 86_400_000;

#[derive(Clone, Copy, Debug, Snafu)]
pub enum TimeCreateError {
    /// The provided time is before the epoch and cannot be represented
    PreEpoch,
    /// The provided is too far into the future to be represented by TimeOfDay
    OutOfRange,
    /// The provided date is invalid
    ///
    /// This likely means that the date you specified does not exist or is outside the range which
    /// can be be represented by chrono::NaiveDate
    InvalidDate,
}

/// Represents a time in 48-bits, as stored in TimeOfDay objects
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeOfDay(TimeDifference);

impl TimeOfDay {
    /// The size of a TimeOfDay object in bytes as stored in the object dict
    pub const SIZE: usize = 6;

    /// A zero-inintialized TimeOfDay corresponding to the TimeOfDay epoch of 1984-01-01
    pub const EPOCH: TimeOfDay = TimeOfDay(TimeDifference { ms: 0, days: 0 });
    // TimeOfDay objects are encoded with reference to 1984-01-01
    const CHRONO_EPOCH: NaiveDate = NaiveDate::from_ymd_opt(1984, 1, 1).unwrap();

    /// Create a new TimeOfDay
    ///
    /// # Arguments
    /// - `days`: The number of days since January 1, 1984
    /// - `ms`: The number of milliseconds after midnight
    pub fn new(days: u16, ms: u32) -> Self {
        Self(TimeDifference::new(days, ms))
    }

    /// Create a TimeOfDay corresponding to the provided date and time
    pub fn from_ymd_hms_ms(
        year: u32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
        sec: u32,
        milli: u32,
    ) -> Result<Self, TimeCreateError> {
        let chrono_date = NaiveDate::from_ymd_opt(year as i32, month, day)
            .ok_or(InvalidDateSnafu.build())?
            .and_hms_milli_opt(hour, min, sec, milli)
            .ok_or(InvalidDateSnafu.build())?;

        let delta = chrono_date - const { Self::CHRONO_EPOCH.and_hms_opt(0, 0, 0).unwrap() };
        let days = delta.num_days();
        let ms = (delta - TimeDelta::days(days)).num_milliseconds();
        if days < 0 {
            PreEpochSnafu.fail()
        } else if days > u16::MAX as i64 {
            OutOfRangeSnafu.fail()
        } else {
            Ok(Self::new(days as u16, ms as u32))
        }
    }

    /// Create a TimeOfDay from little endian bytes
    pub fn from_le_bytes(bytes: [u8; 6]) -> Self {
        Self(TimeDifference::from_le_bytes(bytes))
    }

    /// Get the little endian byte representation of the time of day
    pub fn to_le_bytes(&self) -> [u8; 6] {
        self.0.to_le_bytes()
    }

    /// Get the date represented
    ///
    /// Returns (year, month, day)
    pub fn date_ymd(&self) -> (u32, u32, u32) {
        let date = Self::CHRONO_EPOCH + self.0.as_chrono_delta();
        (date.year() as u32, date.month(), date.day())
    }

    /// Get the date as number of days since 1984-01-01
    pub fn days(&self) -> u16 {
        self.0.days
    }

    /// Get the time of day as (hour, min, sec, millis)
    pub fn time_hmsm(&self) -> (u32, u32, u32, u32) {
        let sec = self.0.ms / 1000;
        let nanos = (self.0.ms % 1000) * 1000;
        let t = NaiveTime::from_num_seconds_from_midnight_opt(sec, nanos).unwrap();
        (t.hour(), t.minute(), t.second(), t.nanosecond() / 1000)
    }

    /// Get the time of day as the number of milliseconds since midnight
    pub fn time_millis(&self) -> u32 {
        self.0.ms
    }

    /// Get the time of of day as a [`Duration`] since midnight
    pub fn time_duration(&self) -> Duration {
        Duration::from_millis(self.time_millis() as u64)
    }

    /// Get the total number of milliseconds since 1984-01-01
    pub fn total_millis(&self) -> u64 {
        self.0.total_millis()
    }

    /// Get the time represented as a SystemTime
    #[cfg(feature = "std")]
    #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
    pub fn as_system_time(&self) -> std::time::SystemTime {
        use std::time::SystemTime;

        // System time is relative to the UNIX_EPOCH of 1970-01-01
        const UNIX_EPOCH: NaiveDate = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let epoch_delta_millis = (Self::CHRONO_EPOCH - UNIX_EPOCH).num_milliseconds() as u64;
        SystemTime::UNIX_EPOCH + self.0.as_duration() + Duration::from_millis(epoch_delta_millis)
    }
}

/// Represents a duration of time in 48-bits, as stored in TimeDifference objects
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeDifference {
    ms: u32,
    days: u16,
}

impl TimeDifference {
    /// The size of a TimeDifference object in bytes as stored in the object dict
    pub const SIZE: usize = 6;

    /// A zero time difference
    pub const ZERO: TimeDifference = TimeDifference { ms: 0, days: 0 };

    /// Create a new time difference from the raw u32 value
    pub const fn new(days: u16, ms: u32) -> Self {
        Self { ms, days }
    }

    /// Create a TimeOfDay from little endian bytes
    pub fn from_le_bytes(bytes: [u8; 6]) -> Self {
        let ms = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let days = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        Self::new(days, ms)
    }

    /// Return the little endian byte representation of the TimeDifference
    pub fn to_le_bytes(&self) -> [u8; 6] {
        let mut bytes = [0; 6];
        bytes[0..4].copy_from_slice(&self.ms.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.days.to_le_bytes());
        bytes
    }

    /// Get the time duration as milliseconds
    pub fn total_millis(&self) -> u64 {
        self.days as u64 * MILLIS_PER_DAY + self.ms as u64
    }

    /// Convert to a [`core::time::Duration`]
    pub fn as_duration(&self) -> Duration {
        Duration::from_millis(self.days as u64 * MILLIS_PER_DAY + self.ms as u64)
    }

    pub(crate) fn as_chrono_delta(&self) -> chrono::TimeDelta {
        chrono::TimeDelta::milliseconds(self.days as i64 * MILLIS_PER_DAY as i64 + self.ms as i64)
    }
}
