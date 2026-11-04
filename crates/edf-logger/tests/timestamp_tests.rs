use edf_logger::LogTimestamp;

#[test]
fn converts_unix_epoch_and_matter_epoch() {
    assert_eq!(
        LogTimestamp::from_epoch_millis(0),
        LogTimestamp::DateTime {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            millis: 0,
        }
    );
    assert_eq!(
        LogTimestamp::from_epoch_millis(946_684_800_000),
        LogTimestamp::DateTime {
            year: 2000,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            millis: 0,
        }
    );
}

#[test]
fn converts_leap_days_and_non_leap_century() {
    for (epoch_ms, year, month, day) in [
        (951_782_400_000, 2000, 2, 29),
        (1_709_164_800_000, 2024, 2, 29),
        (4_107_456_000_000, 2100, 2, 28),
    ] {
        assert_eq!(
            LogTimestamp::from_epoch_millis(epoch_ms),
            LogTimestamp::DateTime {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
                second: 0,
                millis: 0,
            }
        );
    }
}

#[test]
fn converts_recent_timestamp_with_milliseconds() {
    assert_eq!(
        LogTimestamp::from_epoch_millis(1_793_730_600_123),
        LogTimestamp::DateTime {
            year: 2026,
            month: 11,
            day: 3,
            hour: 18,
            minute: 30,
            second: 0,
            millis: 123,
        }
    );
}

#[test]
fn formats_uptime_and_datetime_timestamps() {
    assert_eq!(
        LogTimestamp::Uptime { ms: 3_723_004 }.to_string(),
        "01:02:03.004"
    );
    assert_eq!(
        LogTimestamp::DateTime {
            year: 2026,
            month: 11,
            day: 3,
            hour: 18,
            minute: 30,
            second: 0,
            millis: 123,
        }
        .to_string(),
        "2026-11-03 18:30:00.123"
    );
}
