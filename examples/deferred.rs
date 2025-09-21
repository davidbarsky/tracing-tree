use tracing::{
    debug, error, info, instrument, level_filters::LevelFilter, span, trace, warn, Level,
};
use tracing_subscriber::{layer::SubscriberExt, registry::Registry, Layer};
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
        .with_deferred_spans(true)
        .with_targets(true)
        .with_span_modes(true)
        .with_filter(LevelFilter::DEBUG);

    let subscriber = Registry::default().with(layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
    #[cfg(feature = "tracing-log")]
    tracing_log::LogTracer::init().unwrap();

    let app_span = span!(Level::DEBUG, "hierarchical-example", version = %0.1);
    let _e = app_span.enter();

    let server_span = span!(Level::DEBUG, "server", host = "localhost", port = 8080);

    println!("-> This prints before the span open message");

    let _e2 = server_span.enter();
    info!("starting");
    std::thread::sleep(std::time::Duration::from_millis(1000));

    span!(Level::INFO, "empty-span").in_scope(|| {
        // empty span
    });

    info!("listening");
    // Defer two levels of spans
    println!("-> Deferring two levels of spans");
    span!(Level::INFO, "connections").in_scope(|| {
        let peer1 = span!(Level::DEBUG, "conn", peer_addr = "82.9.9.9", port = 42381);
        peer1.in_scope(|| {
            debug!(peer = "peer1", "connected");
            std::thread::sleep(std::time::Duration::from_millis(300));
            debug!(length = 2, "message received");
        });

        drop(peer1);
        let peer2 = span!(Level::DEBUG, "conn", peer_addr = "82.9.9.9", port = 61548);

        // This span will not be printed at all since no event in it will pass the filter
        peer2.in_scope(|| {
            trace!(peer = "peer2", "connected");
            std::thread::sleep(std::time::Duration::from_millis(300));
            trace!(length = 2, "message received");
        });
        drop(peer2);
        let peer3 = span!(Level::DEBUG, "conn", peer_addr = "8.8.8.8", port = 18230);
        peer3.in_scope(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            debug!(peer = "peer3", "connected");
        });
        drop(peer3);
        let peer4 = span!(
            Level::DEBUG,
            "foomp",
            normal_var = 43,
            "{} <- format string",
            42
        );
        peer4.in_scope(|| {
            error!("hello");
        });
        drop(peer4);
        let peer1 = span!(Level::DEBUG, "conn", peer_addr = "82.9.9.9", port = 42381);
        peer1.in_scope(|| {
            warn!(algo = "xor", "weak encryption requested");
            std::thread::sleep(std::time::Duration::from_millis(300));
            debug!(length = 8, "response sent");
            debug!("disconnected");
        });
        drop(peer1);
        let peer2 = span!(Level::DEBUG, "conn", peer_addr = "8.8.8.8", port = 18230);
        peer2.in_scope(|| {
            debug!(length = 5, "message received");
            std::thread::sleep(std::time::Duration::from_millis(300));
            debug!(length = 8, "response sent");
            debug!("disconnected");
        });
        drop(peer2);
    });

    warn!("internal error");
    log::error!("this is a log message");
    info!("exit");
}

#[allow(dead_code)]
#[instrument]
fn call_a(name: &str) {
    info!(name, "got a name");
    call_b(name)
}

#[instrument]
fn call_b(name: &str) {
    info!(name, "got a name");
}
