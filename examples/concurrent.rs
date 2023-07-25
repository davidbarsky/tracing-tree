use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::FutureExt;
use tracing::{debug, debug_span, error, info, instrument, span, warn, Instrument, Level};
use tracing_subscriber::{layer::SubscriberExt, registry::Registry};
use tracing_tree::HierarchicalLayer;

fn main() {
    let layer = HierarchicalLayer::default()
        .with_writer(std::io::stdout)
        .with_indent_lines(true)
        .with_indent_amount(2)
        .with_thread_names(true)
        .with_thread_ids(true)
        .with_verbose_exit(true)
        .with_verbose_entry(true)
        .with_verbose_retrace(true)
        .with_targets(true);

    let subscriber = Registry::default().with(layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
    #[cfg(feature = "tracing-log")]
    tracing_log::LogTracer::init().unwrap();

    let app_span = span!(Level::TRACE, "hierarchical-example", version = %0.1);

    let _e = app_span.enter();

    let server_span = span!(Level::TRACE, "server", host = "localhost", port = 8080);

    let _e2 = server_span.enter();
    info!("starting");

    std::thread::sleep(std::time::Duration::from_millis(1000));

    info!("listening");

    let peer1 = span!(Level::TRACE, "conn", peer_addr = "82.9.9.9", port = 42381);

    debug!("starting countdowns");
    debug_span!("countdowns").in_scope(|| {
        let mut countdown_a = CountdownFuture {
            label: "a",
            count: 3,
        }
        .instrument(span!(Level::DEBUG, "countdown_a"))
        .fuse();

        let mut countdown_b = CountdownFuture {
            label: "b",
            count: 5,
        }
        .instrument(span!(Level::DEBUG, "countdown_b"))
        .fuse();

        // We don't care if the futures are ready, as we poll manually
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let _ = countdown_a.poll_unpin(&mut cx);
        let _ = countdown_b.poll_unpin(&mut cx);

        std::thread::sleep(std::time::Duration::from_millis(300));

        let _ = countdown_b.poll_unpin(&mut cx);
    });

    tracing::info!("finished countdowns");

    info!("exit")
}

struct CountdownFuture {
    label: &'static str,
    count: u32,
}

impl Future for CountdownFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        debug!(label=?self.label, count=?self.count, "polling countdown");
        self.count -= 1;

        if self.count == 0 {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
