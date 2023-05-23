# tracing-tree

Instrument your application with [tracing](https://github.com/tokio-rs/tracing)
and get tree-structured summaries of your application activity with timing
information on the console:

https://github.com/davidbarsky/tracing-tree/blob/483cc0a118c3170f4246d6fa4a9f018a00d8f0a9/examples/quiet.stdout#L1-L28

(Format inspired by [slog-term](https://github.com/slog-rs/slog#terminal-output-example))

## Setup

After instrumenting your app with
[tracing](https://github.com/tokio-rs/tracing), add this subscriber like this:

```rust
let subscriber = Registry::default().with(HierarchicalLayer::new(2));
tracing::subscriber::set_global_default(subscriber).unwrap();
```