use std::{fmt::Write, time::Duration};

use nu_ansi_term::Style;

use crate::styled;

/// A type that can measure and format the current time.
///
/// This trait is used by [HierarchicalLayer] to include a timestamp with each
/// [Event] when it is logged.
///
/// Notable default implementations of this trait are [LocalDateTime] and `()`.
/// The former prints the current time as reported by [time's OffsetDateTime]
/// (note that it requires a `time` feature to be enabled and may panic!
/// make sure to check out the docs for the [LocalDateTime]),
/// and the latter does not print the current time at all.
///
/// Inspired by the [FormatTime] trait from [tracing-subscriber].
///
/// [HierarchicalLayer]: crate::HierarchicalLayer
/// [Event]: tracing_core::Event
/// [time's OffsetDateTime]: time::OffsetDateTime
/// [FormatTime]: tracing_subscriber::fmt::time::FormatTime
/// [tracing-subscriber]: tracing_subscriber
// NB:
//   We can't use `tracing_subscriber::fmt::format::Writer`
//   since it doesn't have a public constructor.
pub trait FormatTime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result;
    fn style_timestamp(
        &self,
        ansi: bool,
        elapsed: Duration,
        w: &mut impl std::fmt::Write,
    ) -> std::fmt::Result;
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Default do-nothing time formatter.
impl FormatTime for () {
    fn format_time(&self, _w: &mut impl std::fmt::Write) -> std::fmt::Result {
        Ok(())
    }
    fn style_timestamp(
        &self,
        _ansi: bool,
        _elapsed: Duration,
        _w: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Retrieve and print the current wall-clock time in UTC timezone.
#[cfg(feature = "time")]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct UtcDateTime {
    /// Whether to print the time with higher precision.
    pub higher_precision: bool,
}

#[cfg(feature = "time")]
impl FormatTime for UtcDateTime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        let time = time::OffsetDateTime::now_utc();
        write!(w, "{} {}", time.date(), time.time())
    }

    fn style_timestamp(
        &self,
        ansi: bool,
        elapsed: Duration,
        w: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        style_timestamp(ansi, self.higher_precision, elapsed, w)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Retrieve and print the current wall-clock time.
///
/// # Panics
///
/// Panics if [time crate] cannot determine the local UTC offset.
///
/// [time crate]: time
// NB:
//   Can't use `tracing_subscriber::fmt::time::SystemTime` since it uses
//   private `datetime` module to format the actual time.
#[cfg(feature = "time")]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct LocalDateTime {
    /// Whether to print the time with higher precision.
    pub higher_precision: bool,
}

#[cfg(feature = "time")]
impl FormatTime for LocalDateTime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        let time = time::OffsetDateTime::now_local().expect("time offset cannot be determined");
        write!(w, "{}", time)
    }
    fn style_timestamp(
        &self,
        ansi: bool,
        elapsed: Duration,
        w: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        style_timestamp(ansi, self.higher_precision, elapsed, w)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Retrieve and print the relative elapsed wall-clock time since an epoch.
///
/// The `Default` implementation for `Uptime` makes the epoch the current time.
// NB: Copy-pasted from `tracing-subscriber::fmt::time::Uptime`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Uptime {
    epoch: std::time::Instant,
    /// Whether to print the time with higher precision.
    pub higher_precision: bool,
}

impl Default for Uptime {
    fn default() -> Self {
        Uptime::from(std::time::Instant::now())
    }
}

impl From<std::time::Instant> for Uptime {
    fn from(epoch: std::time::Instant) -> Self {
        Uptime {
            epoch,
            higher_precision: false,
        }
    }
}

impl FormatTime for Uptime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        let e = self.epoch.elapsed();
        write!(w, "{:4}.{:06}s", e.as_secs(), e.subsec_micros())
    }
    fn style_timestamp(
        &self,
        ansi: bool,
        elapsed: Duration,
        w: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        style_timestamp(ansi, self.higher_precision, elapsed, w)
    }
}

fn style_timestamp(
    ansi: bool,
    higher_precision: bool,
    elapsed: Duration,
    w: &mut impl Write,
) -> std::fmt::Result {
    if higher_precision {
        format_timestamp_with_decimals(ansi, elapsed, w)
    } else {
        format_timestamp(ansi, elapsed, w)
    }
}

fn format_timestamp(ansi: bool, elapsed: Duration, w: &mut impl Write) -> std::fmt::Result {
    let millis = elapsed.as_millis();
    let secs = elapsed.as_secs();

    // Convert elapsed time to appropriate units: ms, s, or m.
    // - Less than 1s : use ms
    // - Less than 1m : use s
    // - 1m and above : use m
    let (n, unit) = if millis < 1000 {
        (millis as _, "ms")
    } else if secs < 60 {
        (secs, "s ")
    } else {
        (secs / 60, "m ")
    };

    let timestamp = format!("{n:>3}");
    write_style_timestamp(ansi, timestamp, unit, w)
}

fn format_timestamp_with_decimals(
    ansi: bool,
    elapsed: Duration,
    w: &mut impl Write,
) -> std::fmt::Result {
    let secs = elapsed.as_secs_f64();

    // Convert elapsed time to appropriate units: μs, ms, or s.
    // - Less than 1ms: use μs
    // - Less than 1s : use ms
    // - 1s and above : use s
    let (n, unit) = if secs < 0.001 {
        (secs * 1_000_000.0, "μs")
    } else if secs < 1.0 {
        (secs * 1_000.0, "ms")
    } else {
        (secs, "s ")
    };

    let timestamp = format!(" {n:.2}");
    write_style_timestamp(ansi, timestamp, unit, w)
}

fn write_style_timestamp(
    ansi: bool,
    timestamp: String,
    unit: &str,
    w: &mut impl Write,
) -> std::fmt::Result {
    write!(
        w,
        "{timestamp}{unit}",
        timestamp = styled(ansi, Style::new().dimmed(), timestamp),
        unit = styled(ansi, Style::new().dimmed(), unit),
    )
}

////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'a, F> FormatTime for &'a F
where
    F: FormatTime,
{
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        F::format_time(self, w)
    }
    fn style_timestamp(
        &self,
        ansi: bool,
        duration: Duration,
        w: &mut impl std::fmt::Write,
    ) -> std::fmt::Result {
        F::style_timestamp(self, ansi, duration, w)
    }
}

// NB:
//   Can't impl for `fn(&mut impl std::fmt::Write)` since impl trait is not allowed
//   outside of function and inherent method return types for now.
