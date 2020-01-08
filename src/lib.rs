use quanta::Clock;
use std::fmt::Debug;
use tracing::{
    span::{Attributes, Id, Record},
    Event, Level, Metadata, Subscriber,
};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

#[derive(Debug, Default)]
pub struct TreeLayer {
    clock: Clock,
}

#[derive(Debug)]
struct Data {
    start: u64,
    level: Level,
}

impl<S> Layer<S> for TreeLayer
where
    S: Subscriber + for<'span> LookupSpan<'span> + Debug,
{
    fn new_span(&self, attrs: &Attributes, id: &Id, ctx: Context<S>) {
        let start = self.clock.now();
        let level = attrs.metadata().level();
        dbg!(attrs);
        let data = Data {
            start,
            level: level.clone(),
        };
        let span = ctx.span(id).expect("in new_span but span does not exist");
        span.extensions_mut().insert(data);
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<S>) {
        dbg!(event);
    }

    fn on_close(&self, id: Id, ctx: Context<S>) {
        let end = self.clock.now();
        let span = ctx.span(&id).expect("in on_close but span does not exist");
        let mut ext = span.extensions_mut();
        let data = ext
            .get_mut::<Data>()
            .expect("span does not have metric data");

        let elapsed = self.clock.delta(data.start, end);
    }
}
