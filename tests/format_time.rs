use std::{
    fmt::Write,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use tracing::{span, Level};
use tracing_subscriber::{layer::SubscriberExt, Registry};

use tracing_tree::{time::FormatTime, HierarchicalLayer};

#[derive(Debug)]
struct FormatTimeCounter(Arc<AtomicU64>);

impl FormatTime for FormatTimeCounter {
    fn format_time(&self, _w: &mut impl Write) -> std::fmt::Result {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn format_time_num_calls() {
    let num_called = Arc::new(AtomicU64::new(0));
    let format_time_counter = FormatTimeCounter(Arc::clone(&num_called));

    let layer = HierarchicalLayer::default()
        .with_writer(std::io::stdout)
        .with_indent_lines(true)
        .with_indent_amount(2)
        .with_timer(format_time_counter)
        .with_thread_names(true)
        .with_thread_ids(true)
        .with_verbose_exit(true)
        .with_verbose_entry(true)
        .with_targets(true);

    let subscriber = Registry::default().with(layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let test_span = span!(Level::TRACE, "format-time-num-calls-test", version = %0.1);
    let _e = test_span.enter();

    tracing::info!("first event");
    assert_eq!(num_called.load(Ordering::Relaxed), 1);

    std::thread::sleep(std::time::Duration::from_millis(100));
    tracing::info!("second event");
    assert_eq!(num_called.load(Ordering::Relaxed), 2);

    let nested_span = span!(Level::TRACE, "nested-span");
    nested_span.in_scope(|| {
        tracing::debug!("nested event");
        assert_eq!(num_called.load(Ordering::Relaxed), 3);

        tracing::info!("important nested event");
        assert_eq!(num_called.load(Ordering::Relaxed), 4);
    });
    drop(nested_span);

    instrumented_function();
    assert_eq!(num_called.load(Ordering::Relaxed), 6);

    tracing::info!("exiting");
    assert_eq!(num_called.load(Ordering::Relaxed), 7);
}

#[tracing::instrument]
fn instrumented_function() {
    tracing::info!("instrumented function");
    nested_instrumented_function();
}

#[tracing::instrument]
fn nested_instrumented_function() {
    tracing::warn!("nested instrumented function");
}
