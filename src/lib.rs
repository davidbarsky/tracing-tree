use ansi_term::{Color, Style};
use chrono::{DateTime, Local};
use std::sync::Mutex;
use std::{
    fmt::{self, Write as _},
    io,
    io::Write as _,
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
    buf: Mutex<String>,
}

struct Data {
    start: DateTime<Local>,
    kvs: Vec<(&'static str, String)>,
}

struct FmtEvent<'a> {
    buf: &'a mut String,
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
        write!(
            self.buf,
            "{comma} ",
            comma = if self.comma { "," } else { "" },
        )
        .unwrap();
        let name = field.name();
        if name == "message" {
            write!(self.buf, "{:?}", value).unwrap();
            self.comma = true;
        } else {
            write!(self.buf, "{}={:?}", name, value).unwrap();
            self.comma = true;
        }
    }
}

impl<'a> FmtEvent<'a> {
    fn print(&mut self, outer_buf: &mut String, indent: usize, indent_amount: usize) {
        let indented = indent_block(&self.buf, indent, indent_amount);
        outer_buf.push_str(&indented);
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

fn indent_block(block: &str, indent: usize, indent_amount: usize) -> String {
    let lines: Vec<_> = block.lines().collect();
    let indent_spaces = indent * indent_amount;
    let mut buf = String::with_capacity(block.len() + (lines.len() * indent_spaces));
    let indent_str = String::from(" ").repeat(indent_spaces);
    for line in lines {
        buf.push_str(&indent_str);
        buf.push_str(line);
        buf.push('\n');
    }
    buf
}

impl HierarchicalLayer {
    pub fn new(indent_amount: usize) -> Self {
        let ansi = atty::is(atty::Stream::Stdout);
        Self {
            indent_amount,
            stdout: io::stdout(),
            ansi,
            buf: Mutex::new(String::new()),
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
            write!(
                buf,
                "{}{}={}",
                leading,
                // Style::new().fg(Color::Purple).bold().paint(k.as_ref()),
                k.as_ref(),
                v
            )?;
        }
        for (k, v) in kvs {
            write!(
                buf,
                ", {}={}",
                // Style::new().fg(Color::Purple).bold().paint(k.as_ref()),
                k.as_ref(),
                v
            )?;
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

        let mut buf = self.buf.lock().unwrap();
        let mut current_buf = String::new();

        let indent = ctx.scope().collect::<Vec<_>>().len() - 1;

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
        writeln!(current_buf).unwrap();
        let indented = indent_block(&current_buf, indent, self.indent_amount);
        buf.push_str(&indented);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<S>) {
        let mut buf = self.buf.lock().unwrap();
        let mut event_buf = String::new();
        // printing the indentation
        let indent = if let Some(_) = ctx.current_span().id() {
            // size hint isn't implemented on Scope.
            ctx.scope().collect::<Vec<_>>().len()
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
            let level = event.metadata().level();
            let level = if self.ansi {
                ColorLevel(level).to_string()
            } else {
                level.to_string()
            };
            write!(
                &mut event_buf,
                "{timestamp}{unit} {level}",
                timestamp = self.styled(
                    Style::new().dimmed(),
                    elapsed.num_milliseconds().to_string()
                ),
                unit = self.styled(Style::new().dimmed(), "ms"),
                level = level,
            )
            .expect("Unable to write to buffer");
        }
        let mut visitor = FmtEvent {
            comma: false,
            buf: &mut event_buf,
        };
        event.record(&mut visitor);
        buf.reserve(visitor.buf.len());
        writeln!(&mut visitor.buf).unwrap();
        visitor.print(&mut buf, indent, self.indent_amount);
    }

    fn on_close(&self, _id: Id, _ctx: Context<S>) {
        let mut stdout = self.stdout.lock();
        let mut buf = self.buf.lock().unwrap();
        write!(stdout, "{}", buf).unwrap();
        buf.clear();
    }
}
