use core::fmt;

/// Timestamp representation used by the logger before and after clock synchronization.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogTimestamp {
    /// Monotonic time since boot, in milliseconds.
    Uptime { ms: u64 },
    /// UTC civil date and time.
    DateTime {
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        millis: u16,
    },
}

impl LogTimestamp {
    /// Converts Unix epoch milliseconds to a UTC civil date and time.
    pub const fn from_epoch_millis(epoch_ms: u64) -> Self {
        let total_seconds = epoch_ms / 1_000;
        let millis = (epoch_ms % 1_000) as u16;
        let second = (total_seconds % 60) as u8;
        let total_minutes = total_seconds / 60;
        let minute = (total_minutes % 60) as u8;
        let total_hours = total_minutes / 60;
        let hour = (total_hours % 24) as u8;
        let days = (total_hours / 24) as i64;
        let (year, month, day) = days_to_civil(days);

        Self::DateTime {
            year: if year > u16::MAX as i64 {
                u16::MAX
            } else {
                year as u16
            },
            month,
            day,
            hour,
            minute,
            second,
            millis,
        }
    }
}

impl fmt::Display for LogTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uptime { ms } => {
                let total_seconds = ms / 1_000;
                let hours = total_seconds / 3_600;
                let minutes = (total_seconds % 3_600) / 60;
                let seconds = total_seconds % 60;
                let millis = ms % 1_000;
                write!(f, "{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
            }
            Self::DateTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
                millis,
            } => write!(
                f,
                "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{millis:03}"
            ),
        }
    }
}

// Proleptic Gregorian calendar conversion from days since 1970-01-01.
const fn days_to_civil(days_since_epoch: i64) -> (i64, u8, u8) {
    let shifted_days = days_since_epoch + 719_468;
    let era = shifted_days / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };

    (year, month as u8, day as u8)
}
