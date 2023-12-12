use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{pin_mut, FutureExt};
use tracing::Instrument;
use tracing_subscriber::{layer::SubscriberExt, registry::Registry};
use tracing_tree::HierarchicalLayer;

fn main() {
    let layer = HierarchicalLayer::default()
        .with_writer(std::io::stdout)
        .with_indent_lines(true)
        .with_indent_amount(4)
        .with_thread_names(true)
        .with_thread_ids(true)
        .with_span_retrace(true)
        .with_deferred_spans(false)
        .with_targets(true);

    let subscriber = Registry::default().with(layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
    #[cfg(feature = "tracing-log")]
    tracing_log::LogTracer::init().unwrap();

    let fut_a = spawn_fut("a", a);
    pin_mut!(fut_a);

    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(fut_a.poll_unpin(&mut cx).is_pending());

    let fut_b = spawn_fut("b", b);
    pin_mut!(fut_b);

    assert!(fut_b.poll_unpin(&mut cx).is_pending());

    assert!(fut_a.poll_unpin(&mut cx).is_pending());
    assert!(fut_b.poll_unpin(&mut cx).is_pending());

    assert!(fut_a.poll_unpin(&mut cx).is_ready());
    assert!(fut_b.poll_unpin(&mut cx).is_ready());
}

fn spawn_fut<F: Fn() -> Fut, Fut: Future<Output = ()>>(
    key: &'static str,
    inner: F,
) -> impl Future<Output = ()> {
    let span = tracing::info_span!("spawn_fut", key);

    async move {
        countdown(1).await;

        inner().await;
    }
    .instrument(span)
}

fn a() -> impl Future<Output = ()> {
    let span = tracing::info_span!("a");

    async move {
        countdown(1).await;
        tracing::info!("a");
    }
    .instrument(span)
}

fn b() -> impl Future<Output = ()> {
    let span = tracing::info_span!("b");

    async move {
        countdown(1).await;
        tracing::info!("b");
    }
    .instrument(span)
}

fn countdown(count: u32) -> impl Future<Output = ()> {
    CountdownFuture { count }
}

struct CountdownFuture {
    count: u32,
}

impl Future for CountdownFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.count == 0 {
            Poll::Ready(())
        } else {
            self.count -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
