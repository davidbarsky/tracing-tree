use std::{io, str, sync::Mutex};

use tracing::subscriber::set_global_default;
use tracing_subscriber::{layer::SubscriberExt, registry};

use tracing_tree::HierarchicalLayer;

struct RecursiveWriter(Mutex<Vec<u8>>);

impl io::Write for &RecursiveWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend(buf);

        tracing::error!("Nobody expects the Spanish Inquisition");

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        tracing::error!("Nobody expects the Spanish Inquisition");
        Ok(())
    }
}

/// This test checks that if `tracing` events happen during processing of
/// `on_event`, the library does not deadlock.
#[test]
fn recursive_event() {
    static WRITER: RecursiveWriter = RecursiveWriter(Mutex::new(Vec::new()));

    let subscriber = registry().with(HierarchicalLayer::new(2).with_writer(|| &WRITER));
    // This has to be its own integration test because we can't just set a
    // global default like this otherwise and not expect everything else to
    // break.
    set_global_default(subscriber).unwrap();

    tracing::error!("We can never expect the unexpected.");

    let output = WRITER.0.lock().unwrap();
    let output = str::from_utf8(&output).unwrap();

    // If this test finished we're happy. Let's just also check that we did
    // in fact log _something_ and that the logs from within the writer did
    // not actually go through.
    assert!(output.contains("We can never expect the unexpected."));
    assert!(!output.contains("Nobody expects the Spanish Inquisition"));
}
