use ansi_term::{Color, Style};
use chrono::{DateTime, Local};
use std::ops::DerefMut as _;
use std::sync::Mutex;
use std::{
    fmt::{self, Write as _},
    io,
};

use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id},
    Event, Level, Subscriber,
};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

#[derive(Debug)]
pub struct HierarchicalLayer {
    stdout: io::Stdout,
    indent_amount: usize,
    ansi: bool,
    bufs: Mutex<Buffers>,
}

#[derive(Debug)]
struct Buffers {
    pub current_buf: String,
    pub indent_buf: String,
}

impl Buffers {
    fn new() -> Self {
        Self {
            current_buf: String::new(),
            indent_buf: String::new(),
        }
    }

    fn flush_current_buf(&mut self, mut writer: impl io::Write) {
        write!(writer, "{}", &self.current_buf).unwrap();
        self.current_buf.clear();
    }

    fn flush_indent_buf(&mut self) {
        self.current_buf.push_str(&self.indent_buf);
        self.indent_buf.clear();
    }

    fn indent_current(&mut self, indent: usize, indent_amount: usize) {
        indent_block(
            &mut self.current_buf,
            &mut self.indent_buf,
            indent,
            indent_amount,
        );
        self.current_buf.clear();
    }
}

struct Data {
    start: DateTime<Local>,
    kvs: Vec<(&'static str, String)>,
}

struct FmtEvent<'a> {
    bufs: &'a mut Buffers,
    comma: bool,
}

impl Data {
    fn new(attrs: &tracing::span::Attributes<'_>) -> Self {
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

impl<'a> Visit for FmtEvent<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let buf = &mut self.bufs.current_buf;
        write!(buf, "{comma} ", comma = if self.comma { "," } else { "" },).unwrap();
        let name = field.name();
        if name == "message" {
            write!(buf, "{:?}", value).unwrap();
            self.comma = true;
        } else {
            write!(buf, "{}={:?}", name, value).unwrap();
            self.comma = true;
        }
    }
}

impl<'a> FmtEvent<'a> {
    fn finish(&mut self, indent: usize, indent_amount: usize) {
        self.bufs.current_buf.push('\n');
        self.bufs.indent_current(indent, indent_amount);
        self.bufs.flush_indent_buf();
    }
}

struct ColorLevel<'a>(&'a Level);

impl<'a> fmt::Display for ColorLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.0 {
            Level::TRACE => Color::Purple.bold().paint("TRACE"),
            Level::DEBUG => Color::Blue.bold().paint("DEBUG"),
            Level::INFO => Color::Green.bold().paint(" INFO"),
            Level::WARN => Color::RGB(252, 234, 160).bold().paint(" WARN"), // orange
            Level::ERROR => Color::Red.bold().paint("ERROR"),
        }
        .fmt(f)
    }
}

fn indent_block(block: &mut String, buf: &mut String, indent: usize, indent_amount: usize) {
    let lines: Vec<_> = block.lines().collect();
    let indent_spaces = indent * indent_amount;
    buf.reserve(block.len() + (lines.len() * indent_spaces));
    let indent_str = String::from(" ").repeat(indent_spaces);
    for line in lines {
        buf.push_str(&indent_str);
        buf.push_str(line);
        buf.push('\n');
    }
}

impl HierarchicalLayer {
    pub fn new(indent_amount: usize) -> Self {
        let ansi = atty::is(atty::Stream::Stdout);
        Self {
            indent_amount,
            stdout: io::stdout(),
            ansi,
            bufs: Mutex::new(Buffers::new()),
        }
    }

    pub fn with_ansi(self, ansi: bool) -> Self {
        Self { ansi, ..self }
    }

    fn styled(&self, style: Style, text: impl AsRef<str>) -> String {
        if self.ansi {
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

impl<S> Layer<S> for HierarchicalLayer
where
    S: Subscriber + for<'span> LookupSpan<'span> + fmt::Debug,
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

        let indent = ctx.scope().count() - 1;

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
        writeln!(
            current_buf,
            "{}",
            self.styled(Style::new().fg(Color::Green).bold(), "}") // Style::new().dimmed().paint("}")
        )
        .unwrap();

        bufs.indent_current(indent, self.indent_amount);
        bufs.flush_indent_buf();
        bufs.flush_current_buf(self.stdout.lock());
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
        let level = if self.ansi {
            ColorLevel(level).to_string()
        } else {
            level.to_string()
        };
        write!(&mut event_buf, "{level}", level = level).expect("Unable to write to buffer");
        let mut visitor = FmtEvent {
            comma: false,
            bufs: &mut bufs,
        };
        event.record(&mut visitor);
        visitor.finish(indent, self.indent_amount);
        bufs.flush_current_buf(self.stdout.lock());
    }

    fn on_close(&self, _id: Id, _ctx: Context<S>) {}
}
