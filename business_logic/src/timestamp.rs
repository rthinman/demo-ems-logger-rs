use arrayvec::ArrayString;
use core::fmt::Write;

use crate::constants::{AGGREGATION_PERIOD, SAMPLE_PERIOD};

// TODO: implement Format for Timestamp

/// Errors that can occur when working with timestamps.
#[derive(Debug)]
pub enum TimestampError {
    InvalidFormat,
    OutOfRange,
    OutOfOrder,
}

/// Represents a timestamp in seconds since the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timestamp {
    pub seconds: u32,
}

impl Timestamp {
    /// Create an an ISO 8601 Duration string.
    pub fn create_iso8601_str(&self) -> ArrayString<32> {
        let (days, hours, minutes, remaining_seconds) = self.to_dhms();
        let mut result = ArrayString::<32>::new();
        if days > 0 {
            write!(&mut result, "P{}D", days).expect("can't write");
        } else {
            result.push_str("P0D");
        }
        if hours > 0 || minutes > 0 || remaining_seconds > 0 {
            write!(&mut result, "T{}H{}M{}S", hours, minutes, remaining_seconds).expect("can't write");
        } else {
            result.push_str("T0S");
        }
        result
    }

    /// Converts seconds since the epoch to days, hours, minutes, and seconds.
    pub fn to_dhms(&self) -> (u32, u32, u32, u32) {
        let days = self.seconds / 86400;
        let seconds_of_day = self.seconds - days * 86400;
        let hours = seconds_of_day / 3600;
        let remaining_seconds = seconds_of_day - hours * 3600;
        let minutes = remaining_seconds / 60;
        let remaining_seconds = remaining_seconds - minutes * 60;
        (days, hours, minutes, remaining_seconds)
    }

    /// Parses an ISO 8601 Duration string and returns the number of days, hours, minutes, and seconds.
    pub fn parse_duration(input: &str) -> Option<(u32, u32, u32, u32)> {
        if input.starts_with("P0DT0S") {
            return Some((0, 0, 0, 0));
        }
        let input = input.strip_prefix('P')?;
        let (days_str, rest) = input.split_once("DT")?;
        let (hours_str, rest) = rest.split_once('H')?;
        let (minutes_str, rest) = rest.split_once('M')?;
        let seconds_str = rest.strip_suffix('S')?;
        Some((
            days_str.parse().ok()?,
            hours_str.parse().ok()?,
            minutes_str.parse().ok()?,
            seconds_str.parse().ok()?
        ))
    }

    /// Get the last short sample period end timestamp.
    pub fn get_last_short_sample_end(&self) -> Timestamp {
        Timestamp {
            seconds: (self.seconds / SAMPLE_PERIOD) * SAMPLE_PERIOD,
        }
    }

    /// Get the last long sample period end timestamp.
    pub fn get_last_long_sample_end(&self) -> Timestamp {
        Timestamp {
            seconds: (self.seconds / AGGREGATION_PERIOD) * AGGREGATION_PERIOD,
        }
    }

    pub fn get_next_aggregation_start(&self) -> Timestamp {
        Timestamp {
            seconds: (self.seconds / AGGREGATION_PERIOD + 1) * AGGREGATION_PERIOD,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_dhms() {
        let ts = Timestamp { seconds: 93784 };
        assert_eq!(ts.to_dhms(), (1, 2, 3, 4));
    }

    #[test]
    fn test_create_iso8601_str() {
        let ts = Timestamp { seconds: 93784 };
        assert_eq!(ts.create_iso8601_str().as_str(), "P1DT2H3M4S");
        let ts = Timestamp { seconds: 0 };
        assert_eq!(ts.create_iso8601_str().as_str(), "P0DT0S");
    }

    #[test]
    fn test_parse_duration() {
        let parsed = Timestamp::parse_duration("P1DT2H3M4S");
        assert_eq!(parsed, Some((1, 2, 3, 4)));
        let parsed = Timestamp::parse_duration("P0DT0S");
        assert_eq!(parsed, Some((0, 0, 0, 0)));
        let parsed = Timestamp::parse_duration("P2DT3H4M5");
        assert_eq!(parsed, None); // Invalid format, missing seconds.
    }

    #[test]
    fn test_get_last_short_sample_end() {
        // 93784 seconds: 93784 / 900 = 104 remainder 184, so prev period end is (93784/900)*900 = 93600
        let ts = Timestamp { seconds: 93784 };
        let end = ts.get_last_short_sample_end();
        assert_eq!(end.seconds, 93600);

        // Exactly on a period boundary
        let ts = Timestamp { seconds: 900 };
        let end = ts.get_last_short_sample_end();
        assert_eq!(end.seconds, 900);

        // At zero
        let ts = Timestamp { seconds: 0 };
        let end = ts.get_last_short_sample_end();
        assert_eq!(end.seconds, 0);
    }

    #[test]
    fn test_get_last_long_sample_end() {
        // 93784 seconds: 8*3600 = 28800, 93784 / 28800 = 3, so 3*28800 = 86400
        let ts = Timestamp { seconds: 93784 };
        let end = ts.get_last_long_sample_end();
        let start = ts.get_next_aggregation_start();
        assert_eq!(end.seconds, 86400);
        assert_eq!(start.seconds, 115200); // 86400 + 28800

        // Exactly on a period boundary
        let ts = Timestamp { seconds: 28800 };
        let end = ts.get_last_long_sample_end();
        let start = ts.get_next_aggregation_start();
        assert_eq!(end.seconds, 28800);
        assert_eq!(start.seconds, 57600); // 28800 + 28800


        // At zero
        let ts = Timestamp { seconds: 0 };
        let end = ts.get_last_long_sample_end();
        let start = ts.get_next_aggregation_start();
        assert_eq!(end.seconds, 0);
        assert_eq!(start.seconds, 28800); // 0 + 28800
    }

    #[test]
    fn test_long_sample_end_never_after_short_sample_end() {
        // Test a range of timestamps, including boundaries and random values
        for &seconds in &[0, 1, 899, 900, 901, 28799, 28800, 28801, 93784, 100000, 123456] {
            let ts = Timestamp { seconds };
            let short_end = ts.get_last_short_sample_end().seconds;
            let long_end = ts.get_last_long_sample_end().seconds;
            let start = ts.get_next_aggregation_start().seconds;
            assert!(long_end <= short_end, "long_end={} > short_end={} for seconds={}", long_end, short_end, seconds);
            assert!(long_end <= start, "long_end={} > start{} for seconds={}", long_end, start, seconds);
        }
    }


}
