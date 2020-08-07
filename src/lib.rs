pub(crate) mod format;

use ansi_term::{Color, Style};
use chrono::{DateTime, Local};
use format::{Buffers, ColorLevel, Config, FmtEvent};
use std::{
    fmt::{self, Write as _},
    io,
    sync::Mutex,
};
use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id},
    Event, Subscriber,
};
use tracing_subscriber::{
    fmt::MakeWriter,
    layer::{Context, Layer},
    registry::LookupSpan,
};

pub(crate) struct Data {
    start: DateTime<Local>,
    kvs: Vec<(&'static str, String)>,
}

impl Data {
    pub fn new(attrs: &tracing::span::Attributes<'_>) -> Self {
        let mut span = Self {
            start: Local::now(),
            kvs: Vec::new(),
        };
        attrs.record(&mut span);
        span
    }
}

impl Visit for Data {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.kvs.push((field.name(), format!("{:?}", value)))
    }
}

#[derive(Debug)]
pub struct HierarchicalLayer<W = fn() -> io::Stdout>
where
    W: MakeWriter + 'static,
{
    make_writer: W,
    bufs: Mutex<Buffers>,
    config: Config,
}

impl Default for HierarchicalLayer {
    fn default() -> Self {
        Self::new(2)
    }
}

impl HierarchicalLayer<fn() -> io::Stdout> {
    pub fn new(indent_amount: usize) -> Self {
        let ansi = atty::is(atty::Stream::Stdout);
        let config = Config {
            ansi,
            indent_amount,
            ..Default::default()
        };
        Self {
            make_writer: io::stdout,
            bufs: Mutex::new(Buffers::new()),
            config,
        }
    }
}

impl<W> HierarchicalLayer<W>
where
    W: MakeWriter + 'static,
{
    /// Enables terminal colors, boldness and italics.
    pub fn with_ansi(self, ansi: bool) -> Self {
        Self {
            config: self.config.with_ansi(ansi),
            ..self
        }
    }

    pub fn with_writer<W2>(self, make_writer: W2) -> HierarchicalLayer<W2>
    where
        W2: MakeWriter + 'static,
    {
        HierarchicalLayer {
            make_writer,
            config: self.config,
            bufs: self.bufs,
        }
    }

    pub fn with_indent_amount(self, indent_amount: usize) -> Self {
        let config = Config {
            indent_amount,
            ..self.config
        };
        Self { config, ..self }
    }

    /// Renders an ascii art tree instead of just using whitespace indentation.
    pub fn with_indent_lines(self, indent_lines: bool) -> Self {
        Self {
            config: self.config.with_indent_lines(indent_lines),
            ..self
        }
    }

    /// Whether to render the event and span targets. Usually targets are the module path to the
    /// event/span macro invocation.
    pub fn with_targets(self, targets: bool) -> Self {
        Self {
            config: self.config.with_targets(targets),
            ..self
        }
    }

    /// Whether to render the thread id in the beginning of every line. This is helpful to
    /// untangle the tracing statements emitted by each thread.
    pub fn with_thread_ids(self, thread_ids: bool) -> Self {
        Self {
            config: self.config.with_thread_ids(thread_ids),
            ..self
        }
    }

    /// Whether to render the thread name in the beginning of every line. Not all threads have
    /// names, but if they do, this may be more helpful than the generic thread ids.
    pub fn with_thread_names(self, thread_names: bool) -> Self {
        Self {
            config: self.config.with_thread_names(thread_names),
            ..self
        }
    }

    /// Resets the indentation to zero after `wraparound` indentation levels.
    /// This is helpful if you expect very deeply nested spans as otherwise the indentation
    /// just runs out of your screen.
    pub fn with_wraparound(self, wraparound: usize) -> Self {
        Self {
            config: self.config.with_wraparound(wraparound),
            ..self
        }
    }

    fn styled(&self, style: Style, text: impl AsRef<str>) -> String {
        if self.config.ansi {
            style.paint(text.as_ref()).to_string()
        } else {
            text.as_ref().to_string()
        }
    }

    fn print_kvs<'a, I, K, V>(
        &self,
        buf: &mut impl fmt::Write,
        kvs: I,
        leading: &str,
    ) -> fmt::Result
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str> + 'a,
        V: fmt::Display + 'a,
    {
        let mut kvs = kvs.into_iter();
        if let Some((k, v)) = kvs.next() {
            write!(buf, "{}{}={}", leading, k.as_ref(), v)?;
        }
        for (k, v) in kvs {
            write!(buf, ", {}={}", k.as_ref(), v)?;
        }
        Ok(())
    }
}

impl<S, W> Layer<S> for HierarchicalLayer<W>
where
    S: Subscriber + for<'span> LookupSpan<'span> + fmt::Debug,
    W: MakeWriter + 'static,
{
    fn new_span(&self, attrs: &Attributes, id: &Id, ctx: Context<S>) {
        let data = Data::new(attrs);
        let span = ctx.span(id).expect("in new_span but span does not exist");
        span.extensions_mut().insert(data);
    }

    fn on_enter(&self, id: &tracing::Id, ctx: Context<S>) {
        let span = ctx.span(&id).expect("in on_enter but span does not exist");
        let ext = span.extensions();
        let data = ext.get::<Data>().expect("span does not have data");

        let mut guard = self.bufs.lock().unwrap();
        let bufs = &mut *guard;
        let mut current_buf = &mut bufs.current_buf;

        let indent = ctx.scope().count().saturating_sub(1);

        if self.config.targets {
            let target = span.metadata().target();
            write!(
                &mut current_buf,
                "{}::",
                self.styled(Style::new().dimmed(), target,),
            )
            .expect("Unable to write to buffer");
        }

        write!(
            current_buf,
            "{name}",
            name = self.styled(Style::new().fg(Color::Green).bold(), span.metadata().name())
        )
        .unwrap();
        write!(
            current_buf,
            "{}",
            self.styled(Style::new().fg(Color::Green).bold(), "{") // Style::new().fg(Color::Green).dimmed().paint("{")
        )
        .unwrap();
        self.print_kvs(&mut current_buf, data.kvs.iter().map(|(k, v)| (k, v)), "")
            .unwrap();
        write!(
            current_buf,
            "{}",
            self.styled(Style::new().fg(Color::Green).bold(), "}") // Style::new().dimmed().paint("}")
        )
        .unwrap();

        bufs.indent_current(indent, &self.config);
        let writer = self.make_writer.make_writer();
        bufs.flush_current_buf(writer)
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<S>) {
        let mut guard = self.bufs.lock().unwrap();
        let mut bufs = &mut *guard;
        let mut event_buf = &mut bufs.current_buf;

        // printing the indentation
        let indent = if ctx.current_span().id().is_some() {
            // size hint isn't implemented on Scope.
            ctx.scope().count()
        } else {
            0
        };

        // check if this event occurred in the context of a span.
        // if it has, get the start time of this span.
        let start = match ctx.current_span().id() {
            Some(id) => match ctx.span(id) {
                // if the event is in a span, get the span's starting point.
                Some(ctx) => {
                    let ext = ctx.extensions();
                    let data = ext
                        .get::<Data>()
                        .expect("Data cannot be found in extensions");
                    Some(data.start)
                }
                None => None,
            },
            None => None,
        };
        let now = Local::now();
        if let Some(start) = start {
            let elapsed = now - start;
            write!(
                &mut event_buf,
                "{timestamp}{unit} ",
                timestamp = self.styled(
                    Style::new().dimmed(),
                    elapsed.num_milliseconds().to_string()
                ),
                unit = self.styled(Style::new().dimmed(), "ms"),
            )
            .expect("Unable to write to buffer");
        }
        let level = event.metadata().level();
        let level = if self.config.ansi {
            ColorLevel(level).to_string()
        } else {
            level.to_string()
        };
        write!(&mut event_buf, "{level}", level = level).expect("Unable to write to buffer");

        if self.config.targets {
            let target = event.metadata().target();
            write!(
                &mut event_buf,
                " {}",
                self.styled(Style::new().dimmed(), target,),
            )
            .expect("Unable to write to buffer");
        }

        let mut visitor = FmtEvent {
            comma: false,
            bufs: &mut bufs,
        };
        event.record(&mut visitor);
        visitor.bufs.indent_current(indent, &self.config);
        let writer = self.make_writer.make_writer();
        bufs.flush_current_buf(writer)
    }

    fn on_exit(&self, _id: &Id, _ctx: Context<S>) {}
}
