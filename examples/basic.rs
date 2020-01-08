use tracing::{info, instrument, span, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry::Registry, Layer};
use tracing_tree::TreeLayer;

fn main() {
    let subscriber = Registry::default().with(TreeLayer::default());
    tracing::subscriber::set_global_default(subscriber).expect("Unable to set a global default");
    call_a("david");
}

#[instrument]
fn call_a(name: &str) {
    info!(name, "got a name");
    call_b(name)
}

#[instrument]
fn call_b(name: &str) {
    info!(name, "got a name");
}
