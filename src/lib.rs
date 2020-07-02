use ansi_term::{Color, Style};
use chrono::{DateTime, Local};
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

const LINE_VERT: &str = "│";
const LINE_HORIZ: &str = "─";
const LINE_BRANCH: &str = "├";

#[derive(Debug)]
struct Config {
    ansi: bool,
    indent_lines: bool,
    indent_amount: usize,
}

impl Config {
    fn with_ansi(self, ansi: bool) -> Self {
        Self { ansi, ..self }
    }
    fn with_indent_lines(self, indent_lines: bool) -> Self {
        Self {
            indent_lines,
            ..self
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ansi: true,
            indent_lines: false,
            indent_amount: 2,
        }
    }
}

#[derive(Debug)]
pub struct HierarchicalLayer {
    stdout: io::Stdout,
    bufs: Mutex<Buffers>,
    config: Config,
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

    fn indent_current(&mut self, indent: usize, config: &Config) {
        indent_block(
            &mut self.current_buf,
            &mut self.indent_buf,
            indent,
            config.indent_amount,
            config.indent_lines,
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
    fn finish(&mut self, indent: usize, config: &Config) {
        self.bufs.current_buf.push('\n');
        self.bufs.indent_current(indent, config);
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

fn indent_block_with_lines(lines: &[&str], buf: &mut String, indent: usize, indent_amount: usize) {
    let indent_spaces = indent * indent_amount;
    if lines.len() == 0 {
        return;
    } else if indent_spaces == 0 {
        for line in lines {
            buf.push_str(line);
            buf.push('\n');
        }
        return;
    }
    let mut s = String::with_capacity(indent_spaces);

    // instead of using all spaces to indent, draw a vertical line at every indent level
    // up until the last indent
    for i in 0..(indent_spaces - indent_amount) {
        if i % indent_amount == 0 {
            s.push_str(LINE_VERT);
        } else {
            s.push(' ');
        }
    }

    // draw branch
    buf.push_str(&s);
    buf.push_str(LINE_BRANCH);

    // add `indent_amount - 1` horizontal lines before the span/event
    for _ in 0..(indent_amount - 1) {
        buf.push_str(LINE_HORIZ);
    }
    buf.push_str(&lines[0]);
    buf.push('\n');

    // add the rest of the indentation, since we don't want to draw horizontal lines
    // for subsequent lines
    for i in 0..indent_amount {
        if i % indent_amount == 0 {
            s.push_str(LINE_VERT);
        } else {
            s.push(' ');
        }
    }

    // add all of the actual content, with each line preceded by the indent string
    for line in &lines[1..] {
        buf.push_str(&s);
        buf.push_str(line);
        buf.push('\n');
    }
}

fn indent_block(
    block: &mut String,
    buf: &mut String,
    indent: usize,
    indent_amount: usize,
    indent_lines: bool,
) {
    let lines: Vec<&str> = block.lines().collect();
    let indent_spaces = indent * indent_amount;
    buf.reserve(block.len() + (lines.len() * indent_spaces));
    if indent_lines {
        indent_block_with_lines(&lines, buf, indent, indent_amount);
    } else {
        let indent_str = String::from(" ").repeat(indent_spaces);
        for line in lines {
            buf.push_str(&indent_str);
            buf.push_str(line);
            buf.push('\n');
        }
    }
}

impl HierarchicalLayer {
    pub fn new(indent_amount: usize) -> Self {
        let ansi = atty::is(atty::Stream::Stdout);
        let config = Config {
            ansi,
            indent_amount,
            ..Default::default()
        };
        Self {
            stdout: io::stdout(),
            bufs: Mutex::new(Buffers::new()),
            config,
        }
    }

    pub fn with_ansi(self, ansi: bool) -> Self {
        Self {
            config: self.config.with_ansi(ansi),
            ..self
        }
    }

    pub fn with_indent_lines(self, indent_lines: bool) -> Self {
        Self {
            config: self.config.with_indent_lines(indent_lines),
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

        let indent = ctx.scope().count().saturating_sub(1);

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

        bufs.indent_current(indent, &self.config);
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
        let level = if self.config.ansi {
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
        visitor.finish(indent, &self.config);
        bufs.flush_current_buf(self.stdout.lock());
    }

    fn on_exit(&self, _id: &Id, _ctx: Context<S>) {}
}
