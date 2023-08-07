pub(crate) mod format;
pub mod time;

use crate::time::FormatTime;
use format::{write_span_mode, Buffers, ColorLevel, Config, FmtEvent, SpanMode};

use is_terminal::IsTerminal;
use nu_ansi_term::{Color, Style};
use std::{
    any::Any,
    fmt::{self, Write as _},
    hint::spin_loop,
    io,
    iter::Fuse,
    mem,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Instant,
};
use tracing_core::{
    field::{Field, Visit},
    span::{Attributes, Id},
    Event, Subscriber,
};
#[cfg(feature = "tracing-log")]
use tracing_log::NormalizeEvent;
use tracing_subscriber::{
    fmt::MakeWriter,
    layer::{Context, Layer},
    registry::{self, LookupSpan, ScopeFromRoot, SpanRef},
};

// Span extension data
pub(crate) struct Data {
    start: Instant,
    kvs: Vec<(&'static str, String)>,
    written: bool,
}

impl Data {
    pub fn new(attrs: &Attributes<'_>, printed: bool) -> Self {
        let mut span = Self {
            start: Instant::now(),
            kvs: Vec::new(),
            written: printed,
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
pub struct HierarchicalLayer<W = fn() -> io::Stderr, FT = ()>
where
    W: for<'writer> MakeWriter<'writer> + 'static,
    FT: FormatTime,
{
    make_writer: W,
    bufs: Mutex<Buffers>,
    config: Config,
    timer: FT,
}

impl Default for HierarchicalLayer {
    fn default() -> Self {
        Self::new(2)
    }
}

impl HierarchicalLayer<fn() -> io::Stderr> {
    pub fn new(indent_amount: usize) -> Self {
        let ansi = io::stderr().is_terminal();
        let config = Config {
            ansi,
            indent_amount,
            ..Default::default()
        };
        Self {
            make_writer: io::stderr,
            bufs: Mutex::new(Buffers::new()),
            config,
            timer: (),
        }
    }
}

impl<W, FT> HierarchicalLayer<W, FT>
where
    W: for<'writer> MakeWriter<'writer> + 'static,
    FT: FormatTime,
{
    /// Enables terminal colors, boldness and italics.
    pub fn with_ansi(self, ansi: bool) -> Self {
        Self {
            config: self.config.with_ansi(ansi),
            ..self
        }
    }

    pub fn with_writer<W2>(self, make_writer: W2) -> HierarchicalLayer<W2, FT>
    where
        W2: for<'writer> MakeWriter<'writer>,
    {
        HierarchicalLayer {
            make_writer,
            config: self.config,
            bufs: self.bufs,
            timer: self.timer,
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

    /// Specifies how to measure and format time at which event has occurred.
    pub fn with_timer<FT2: FormatTime>(self, timer: FT2) -> HierarchicalLayer<W, FT2> {
        HierarchicalLayer {
            make_writer: self.make_writer,
            config: self.config,
            bufs: self.bufs,
            timer,
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

    /// Whether to print the currently active span's message again before entering a new span.
    /// This helps if the entry to the current span was quite a while back (and with scrolling
    /// upwards in logs).
    pub fn with_verbose_entry(self, verbose_entry: bool) -> Self {
        Self {
            config: self.config.with_verbose_entry(verbose_entry),
            ..self
        }
    }

    /// Whether to print the currently active span's message again before dropping it.
    /// This helps if the entry to the current span was quite a while back (and with scrolling
    /// upwards in logs).
    pub fn with_verbose_exit(self, verbose_exit: bool) -> Self {
        Self {
            config: self.config.with_verbose_exit(verbose_exit),
            ..self
        }
    }

    /// Whether to print the currently active span's message again if another span was entered in
    /// the meantime
    /// This helps during concurrent or multi-threaded events where threads are entered, but not
    /// necessarily *exited* before other *divergent* spans are entered and generating events.
    pub fn with_verbose_retrace(self, verbose_retrace: bool) -> Self {
        Self {
            config: self.config.with_verbose_retrace(verbose_retrace),
            ..self
        }
    }

    /// Defers printing span opening until an event is generated within the span.
    ///
    /// Avoids printing empty spans with no generated events.
    pub fn with_deferred_spans(self, enabled: bool) -> Self {
        Self {
            config: self.config.with_deferred_spans(enabled),
            ..self
        }
    }

    /// Prefixes each branch with the event mode, such as `open`, or `close`
    pub fn with_span_modes(self, enabled: bool) -> Self {
        Self {
            config: self.config.with_span_modes(enabled),
            ..self
        }
    }

    /// Whether to print `{}` around the fields when printing a span.
    /// This can help visually distinguish fields from the rest of the message.
    pub fn with_bracketed_fields(self, bracketed_fields: bool) -> Self {
        Self {
            config: self.config.with_bracketed_fields(bracketed_fields),
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

    fn print_kvs<'a, I, V>(&self, buf: &mut impl fmt::Write, kvs: I) -> fmt::Result
    where
        I: IntoIterator<Item = (&'a str, V)>,
        V: fmt::Display + 'a,
    {
        let mut kvs = kvs.into_iter();
        if let Some((k, v)) = kvs.next() {
            if k == "message" {
                write!(buf, "{}", v)?;
            } else {
                write!(buf, "{}={}", k, v)?;
            }
        }
        for (k, v) in kvs {
            write!(buf, ", {}={}", k, v)?;
        }
        Ok(())
    }

    fn write_span_info<S>(
        &self,
        span: &SpanRef<S>,
        bufs: &mut Buffers,
        ctx: &Context<S>,
        style: SpanMode,
    ) where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        // let span = ctx
        //     .span(id)
        //     .expect("in on_enter/on_exit but span does not exist");

        let ext = span.extensions();
        let data = ext.get::<Data>().expect("span does not have data");

        let mut current_buf = &mut bufs.current_buf;

        if self.config.span_modes {
            write_span_mode(current_buf, style)
        }

        let indent = scope_path(span).skip(1).count();

        let should_write = match style {
            SpanMode::Open { .. } | SpanMode::Event => true,
            // Print the parent of a new span again before entering the child
            SpanMode::PreOpen if self.config.verbose_entry => true,
            SpanMode::Close { verbose } => verbose,
            SpanMode::Retrace { verbose } => verbose,
            // Generated if `verbose_exit` is enabled
            SpanMode::PostClose => true,
            _ => false,
        };

        if should_write {
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
            if self.config.bracketed_fields {
                write!(
                    current_buf,
                    "{}",
                    self.styled(Style::new().fg(Color::Green).bold(), "{") // Style::new().fg(Color::Green).dimmed().paint("{")
                )
                .unwrap();
            } else {
                write!(current_buf, " ").unwrap();
            }
            self.print_kvs(&mut current_buf, data.kvs.iter().map(|(k, v)| (*k, v)))
                .unwrap();
            if self.config.bracketed_fields {
                write!(
                    current_buf,
                    "{}",
                    self.styled(Style::new().fg(Color::Green).bold(), "}") // Style::new().dimmed().paint("}")
                )
                .unwrap();
            }
        }

        bufs.indent_current(indent, &self.config, style);
        let writer = self.make_writer.make_writer();
        bufs.flush_current_buf(writer)
    }
}

impl<S, W, FT> Layer<S> for HierarchicalLayer<W, FT>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    W: for<'writer> MakeWriter<'writer> + 'static,
    FT: FormatTime + 'static,
{
    fn on_new_span(&self, attrs: &Attributes, id: &Id, ctx: Context<S>) {
        let span = ctx.span(id).expect("in new_span but span does not exist");

        if span.extensions().get::<Data>().is_none() {
            let data = Data::new(attrs, !self.config.deferred_spans);
            span.extensions_mut().insert(data);
        }

        // Entry will be printed in on_event along with retrace
        if self.config.deferred_spans {
            return;
        }

        let bufs = &mut *self.bufs.lock().unwrap();

        // Store the most recently entered span
        bufs.current_span = Some(span.id());

        if self.config.verbose_entry {
            if let Some(span) = span.parent() {
                self.write_span_info(&span, bufs, &ctx, SpanMode::PreOpen);
            }
        }

        self.write_span_info(
            &span,
            bufs,
            &ctx,
            SpanMode::Open {
                verbose: self.config.verbose_entry,
            },
        );
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<S>) {
        let span = ctx.current_span();
        let span_id = span.id();
        let span = span_id.and_then(|id| ctx.span(id).map(|span| (id, span)));

        let mut guard = self.bufs.lock().unwrap();
        let bufs = &mut *guard;

        if let Some((span_id, current_span)) = &span {
            if self.config.verbose_retrace
                || current_span
                    .extensions()
                    .get::<Data>()
                    .is_some_and(|v| !v.written)
            {
                if let Some(data) = current_span.extensions_mut().get_mut::<Data>() {
                    data.written = true;
                }

                let old_span_id = bufs.current_span.replace((*span_id).clone());

                eprintln!("Old span: {old_span_id:?}");

                if Some(*span_id) != old_span_id.as_ref() {
                    let old_span = old_span_id.as_ref().and_then(|v| ctx.span(v));

                    // eprintln!(
                    //     "concurrent old: {:?}, new: {:?}",
                    //     old_span.as_ref().map(|v| v.metadata().name()),
                    //     current_span.metadata().name()
                    // );

                    let old_path = old_span.as_ref().map(scope_path).into_iter().flatten();
                    let new_path = scope_path(current_span);

                    // Print the path from the common base of the two spans
                    let new_path = DifferenceIter::new(old_path, new_path, |v| v.id());

                    for span in new_path {
                        self.write_span_info(
                            &span,
                            bufs,
                            &ctx,
                            SpanMode::Retrace {
                                verbose: self.config.verbose_retrace,
                            },
                        )
                    }
                }
            }
        }

        let mut event_buf = &mut bufs.current_buf;

        // Time.

        {
            let prev_buffer_len = event_buf.len();

            self.timer
                .format_time(&mut event_buf)
                .expect("Unable to write time to buffer");

            // Something was written to the buffer, pad it with a space.
            if prev_buffer_len < event_buf.len() {
                write!(event_buf, " ").expect("Unable to write to buffer");
            }
        }

        // printing the indentation
        let indent = ctx
            .event_scope(event)
            .map(|scope| scope.count())
            .unwrap_or(0);

        // check if this event occurred in the context of a span.
        // if it has, get the start time of this span.
        let start = match span {
            Some((id, span)) => {
                // if the event is in a span, get the span's starting point.
                let ext = span.extensions();
                let data = ext
                    .get::<Data>()
                    .expect("Data cannot be found in extensions");

                Some(data.start)
            }
            None => None,
        };

        if let Some(start) = start {
            let elapsed = start.elapsed();
            let millis = elapsed.as_millis();
            let secs = elapsed.as_secs();
            let (n, unit) = if millis < 1000 {
                (millis as _, "ms")
            } else if secs < 60 {
                (secs, "s ")
            } else {
                (secs / 60, "m ")
            };
            let n = format!("{n:>3}");
            write!(
                &mut event_buf,
                "{timestamp}{unit} ",
                timestamp = self.styled(Style::new().dimmed(), n),
                unit = self.styled(Style::new().dimmed(), unit),
            )
            .expect("Unable to write to buffer");
        }

        #[cfg(feature = "tracing-log")]
        let normalized_meta = event.normalized_metadata();
        #[cfg(feature = "tracing-log")]
        let metadata = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());
        #[cfg(not(feature = "tracing-log"))]
        let metadata = event.metadata();

        let level = metadata.level();
        let level = if self.config.ansi {
            ColorLevel(level).to_string()
        } else {
            level.to_string()
        };

        write!(&mut event_buf, "{level}", level = level).expect("Unable to write to buffer");

        if self.config.targets {
            let target = metadata.target();
            write!(
                &mut event_buf,
                " {}",
                self.styled(Style::new().dimmed(), target,),
            )
            .expect("Unable to write to buffer");
        }

        let mut visitor = FmtEvent { comma: false, bufs };
        event.record(&mut visitor);
        visitor
            .bufs
            .indent_current(indent, &self.config, SpanMode::Event);
        let writer = self.make_writer.make_writer();
        bufs.flush_current_buf(writer)
    }

    fn on_close(&self, id: Id, ctx: Context<S>) {
        let bufs = &mut *self.bufs.lock().unwrap();

        // Store the most recently entered span
        if bufs.current_span.as_ref() == Some(&id) {
            bufs.current_span = None;
        }

        let span = ctx.span(&id).expect("invalid span in on_close");

        // Span was not printed, so don't print an exit
        if self.config.deferred_spans
            && span.extensions().get::<Data>().map(|v| v.written) != Some(true)
        {}

        self.write_span_info(
            &span,
            bufs,
            &ctx,
            SpanMode::Close {
                verbose: self.config.verbose_exit,
            },
        );

        if self.config.verbose_exit {
            if let Some(parent_span) = span.parent() {
                // Consider parent as entered
                bufs.current_span = Some(parent_span.id());

                self.write_span_info(&parent_span, bufs, &ctx, SpanMode::PostClose);
            }
        }
    }
}

fn scope_path<'a, R: LookupSpan<'a>>(span: &SpanRef<'a, R>) -> ScopeFromRoot<'a, R> {
    span.scope().from_root()
}

/// Runs `A` and `B` side by side and only yields items present in `B`
struct DifferenceIter<L, R, F> {
    left: Fuse<L>,
    right: R,
    compare: F,
}

impl<L: Iterator<Item = T>, R: Iterator<Item = T>, T, U: PartialEq, F: Fn(&T) -> U>
    DifferenceIter<L, R, F>
{
    fn new(left: L, right: R, compare: F) -> Self {
        Self {
            left: left.fuse(),
            right,
            compare,
        }
    }
}

impl<L: Iterator<Item = T>, R: Iterator<Item = T>, T, U: PartialEq, F: Fn(&T) -> U> Iterator
    for DifferenceIter<L, R, F>
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let left = self.left.next();
            let right = self.right.next()?;

            if left.as_ref().map(&self.compare) != Some((self.compare)(&right)) {
                return Some(right);
            }
        }
    }
}
