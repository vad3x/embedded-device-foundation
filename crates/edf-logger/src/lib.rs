#![no_std]
//! A configurable uptime logger with fixed-capacity in-memory retention.
//!
//! Device composition provides a millisecond clock and an output callback.

mod filter;
mod ring;

use core::cell::RefCell;
use core::fmt::{self, Write};
use critical_section::Mutex;

pub use filter::{DEFAULT_SUPPRESSED_DEBUG_EXACT, DEFAULT_SUPPRESSED_DEBUG_PREFIXES};
pub use ring::LogRingBuffer;

/// A no-allocation logger with one retained byte ring per instance.
pub struct UptimeLogger<const N: usize> {
    clock_ms: fn() -> u64,
    output: for<'a> fn(log::Level, fmt::Arguments<'a>),
    suppressed_debug_exact: &'static [&'static str],
    suppressed_debug_prefixes: &'static [&'static str],
    ring: Mutex<RefCell<LogRingBuffer<N>>>,
}

impl<const N: usize> UptimeLogger<N> {
    /// Creates a logger with callbacks supplied by the device composition root.
    ///
    /// `output` receives a complete timestamped record without a line ending.
    /// The output callback owns any transport-specific line ending or decoration.
    pub const fn new(
        clock_ms: fn() -> u64,
        output: for<'a> fn(log::Level, fmt::Arguments<'a>),
        suppressed_debug_exact: &'static [&'static str],
        suppressed_debug_prefixes: &'static [&'static str],
    ) -> Self {
        Self {
            clock_ms,
            output,
            suppressed_debug_exact,
            suppressed_debug_prefixes,
            ring: Mutex::new(RefCell::new(LogRingBuffer::new())),
        }
    }

    /// Installs this static instance as the process logger and sets its maximum level.
    pub fn init(&'static self, level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
        log::set_logger(self)?;
        log::set_max_level(level);
        Ok(())
    }

    /// Copies the newest retained bytes that fit in `buf` in chronological order.
    pub fn get_buffered_logs(&self, buf: &mut [u8]) -> usize {
        critical_section::with(|cs| self.ring.borrow(cs).borrow().read_into(buf))
    }
}

impl<const N: usize> log::Log for UptimeLogger<N> {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        filter::enabled(
            metadata,
            self.suppressed_debug_exact,
            self.suppressed_debug_prefixes,
        )
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let ms = (self.clock_ms)();
        let total_secs = ms / 1000;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        let millis = ms % 1000;

        critical_section::with(|cs| {
            let mut ring = self.ring.borrow(cs).borrow_mut();
            let _ = writeln!(
                &mut *ring,
                "{hours:02}:{minutes:02}:{seconds:02}.{millis:03} {:<5} {} - {}",
                record.level(),
                record.target(),
                record.args(),
            );
        });
        (self.output)(
            record.level(),
            format_args!(
                "{hours:02}:{minutes:02}:{seconds:02}.{millis:03} {:<5} {} - {}",
                record.level(),
                record.target(),
                record.args(),
            ),
        );
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::fmt::Write;
    use log::{Level, Log, Metadata, Record};
    use std::format;
    use std::string::{String, ToString};
    use std::sync::Mutex;

    static OUTPUT: Mutex<String> = Mutex::new(String::new());
    static OUTPUT_RECORDS: Mutex<std::vec::Vec<(Level, String)>> = Mutex::new(std::vec::Vec::new());
    static TEST_SERIAL: Mutex<()> = Mutex::new(());
    static REGISTERED_LOGGER: UptimeLogger<8> = UptimeLogger::new(clock, output, &[], &[]);

    fn clock() -> u64 {
        3_723_004
    }

    fn output(_level: Level, args: core::fmt::Arguments<'_>) {
        OUTPUT.lock().unwrap().write_fmt(args).unwrap();
    }

    fn output_with_level(level: Level, args: core::fmt::Arguments<'_>) {
        let mut record = String::new();
        record.write_fmt(args).unwrap();
        OUTPUT_RECORDS.lock().unwrap().push((level, record));
    }

    #[test]
    fn init_registers_once_and_keeps_the_first_level_on_duplicate_attempt() {
        assert!(REGISTERED_LOGGER.init(log::LevelFilter::Info).is_ok());
        assert_eq!(log::max_level(), log::LevelFilter::Info);

        assert!(REGISTERED_LOGGER.init(log::LevelFilter::Debug).is_err());
        assert_eq!(log::max_level(), log::LevelFilter::Info);
    }

    #[test]
    fn logger_formats_uptime_and_writes_to_output_and_ring() {
        let _serial = TEST_SERIAL.lock().unwrap();
        OUTPUT.lock().unwrap().clear();
        let logger = UptimeLogger::<128>::new(clock, output, &[], &[]);
        logger.log(
            &Record::builder()
                .args(format_args!("door opened"))
                .level(Level::Info)
                .target("entry")
                .build(),
        );

        let expected = "01:02:03.004 INFO  entry - door opened";
        assert_eq!(&*OUTPUT.lock().unwrap(), expected);
        let mut bytes = [0; 128];
        let count = logger.get_buffered_logs(&mut bytes);
        assert_eq!(&bytes[..count], format!("{expected}\n").as_bytes());
    }

    #[test]
    fn output_callback_receives_level_while_ring_records_stay_uncolored() {
        let _serial = TEST_SERIAL.lock().unwrap();
        OUTPUT_RECORDS.lock().unwrap().clear();
        let logger = UptimeLogger::<128>::new(clock, output_with_level, &[], &[]);

        for level in [Level::Error, Level::Warn] {
            logger.log(
                &Record::builder()
                    .args(format_args!("door alert"))
                    .level(level)
                    .target("entry")
                    .build(),
            );
        }

        let expected_error = "01:02:03.004 ERROR entry - door alert";
        let expected_warn = "01:02:03.004 WARN  entry - door alert";
        assert_eq!(
            &*OUTPUT_RECORDS.lock().unwrap(),
            &[
                (Level::Error, expected_error.to_string()),
                (Level::Warn, expected_warn.to_string()),
            ]
        );

        let mut retained = [0; 128];
        let retained_len = logger.get_buffered_logs(&mut retained);
        assert_eq!(
            &retained[..retained_len],
            format!("{expected_error}\n{expected_warn}\n").as_bytes()
        );
    }

    #[test]
    fn logger_filters_debug_prefix_but_keeps_info() {
        let _serial = TEST_SERIAL.lock().unwrap();
        OUTPUT.lock().unwrap().clear();
        let logger = UptimeLogger::<128>::new(clock, output, &[], &["noisy::"]);
        let debug = Metadata::builder()
            .level(Level::Debug)
            .target("noisy::transport")
            .build();
        let info = Metadata::builder()
            .level(Level::Info)
            .target("noisy::transport")
            .build();
        assert!(!logger.enabled(&debug));
        assert!(logger.enabled(&info));
        logger.log(
            &Record::builder()
                .args(format_args!("discarded"))
                .level(Level::Debug)
                .target("noisy::transport")
                .build(),
        );
        assert!(OUTPUT.lock().unwrap().is_empty());
        logger.log(
            &Record::builder()
                .args(format_args!("visible"))
                .level(Level::Info)
                .target("noisy::transport")
                .build(),
        );
        assert!(OUTPUT
            .lock()
            .unwrap()
            .contains("noisy::transport - visible"));
    }

    #[test]
    fn logger_filters_only_exact_target_when_configured() {
        let logger = UptimeLogger::<8>::new(clock, output, &["noisy"], &[]);
        let exact = Metadata::builder()
            .level(Level::Trace)
            .target("noisy")
            .build();
        let child = Metadata::builder()
            .level(Level::Trace)
            .target("noisy::child")
            .build();
        assert!(!logger.enabled(&exact));
        assert!(logger.enabled(&child));
    }

    #[test]
    fn logger_instances_retain_separate_records() {
        let _serial = TEST_SERIAL.lock().unwrap();
        let first = UptimeLogger::<64>::new(clock, output, &[], &[]);
        let second = UptimeLogger::<64>::new(clock, output, &[], &[]);
        first.log(
            &Record::builder()
                .args(format_args!("first"))
                .level(Level::Info)
                .target("test")
                .build(),
        );

        let mut bytes = [0; 64];
        assert_eq!(second.get_buffered_logs(&mut bytes), 0);
        assert_ne!(first.get_buffered_logs(&mut bytes), 0);
    }
}
