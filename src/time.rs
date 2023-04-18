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
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Default do-nothing time formatter.
impl FormatTime for () {
    fn format_time(&self, _w: &mut impl std::fmt::Write) -> std::fmt::Result {
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Retrieve and print the current wall-clock time in UTC timezone.
#[cfg(feature = "time")]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct UtcDateTime;

#[cfg(feature = "time")]
impl FormatTime for UtcDateTime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        let time = time::OffsetDateTime::now_utc();
        write!(w, "{} {}", time.date(), time.time())
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
pub struct LocalDateTime;

#[cfg(feature = "time")]
impl FormatTime for LocalDateTime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        let time = time::OffsetDateTime::now_local().expect("time offset cannot be determined");
        write!(w, "{}", time)
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
}

impl Default for Uptime {
    fn default() -> Self {
        Uptime {
            epoch: std::time::Instant::now(),
        }
    }
}

impl From<std::time::Instant> for Uptime {
    fn from(epoch: std::time::Instant) -> Self {
        Uptime { epoch }
    }
}

impl FormatTime for Uptime {
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        let e = self.epoch.elapsed();
        write!(w, "{:4}.{:06}s", e.as_secs(), e.subsec_micros())
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'a, F> FormatTime for &'a F
where
    F: FormatTime,
{
    fn format_time(&self, w: &mut impl std::fmt::Write) -> std::fmt::Result {
        (*self).format_time(w)
    }
}

// NB:
//   Can't impl for `fn(&mut impl std::fmt::Write)` since impl trait is not allowed
//   outside of function and inherent method return types for now.
