use ansi_term::{Color, Style};
use chrono::{DateTime, Local};
use std::{fmt, io, io::Write as _};
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
}

struct Data {
    start: DateTime<Local>,
    kvs: Vec<(&'static str, String)>,
}

struct FmtEvent<'a> {
    stdout: io::StdoutLock<'a>,
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
            &mut self.stdout,
            "{comma} ",
            comma = if self.comma { "," } else { "" },
        )
        .unwrap();
        let name = field.name();
        if name == "message" {
            write!(&mut self.stdout, "{:?}", value).unwrap();
            self.comma = true;
        } else {
            write!(&mut self.stdout, "{}={:?}", name, value).unwrap();
            self.comma = true;
        }
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

impl HierarchicalLayer {
    pub fn new(indent_amount: usize) -> Self {
        let ansi = atty::is(atty::Stream::Stdout);
        Self {
            indent_amount,
            stdout: io::stdout(),
            ansi,
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
        writer: &mut impl io::Write,
        kvs: I,
        leading: &str,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str> + 'a,
        V: fmt::Display + 'a,
    {
        let mut kvs = kvs.into_iter();
        if let Some((k, v)) = kvs.next() {
            write!(
                writer,
                "{}{}={}",
                leading,
                // Style::new().fg(Color::Purple).bold().paint(k.as_ref()),
                k.as_ref(),
                v
            )?;
        }
        for (k, v) in kvs {
            write!(
                writer,
                ", {}={}",
                // Style::new().fg(Color::Purple).bold().paint(k.as_ref()),
                k.as_ref(),
                v
            )?;
        }
        Ok(())
    }

    fn print_indent(&self, writer: &mut impl io::Write, indent: usize) -> io::Result<()> {
        for _ in 0..(indent * self.indent_amount) {
            write!(writer, " ")?;
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
        let mut stdout = self.stdout.lock();
        let span = ctx.span(&id).expect("in on_enter but span does not exist");
        let ext = span.extensions();
        let data = ext.get::<Data>().expect("span does not have data");

        let indent = ctx.scope().collect::<Vec<_>>().len() - 1;
        self.print_indent(&mut stdout, indent)
            .expect("Unable to write to stdout");

        write!(
            &mut stdout,
            "{name}",
            name = self.styled(Style::new().fg(Color::Green).bold(), span.metadata().name())
        )
        .unwrap();
        write!(
            &mut stdout,
            "{}",
            self.styled(Style::new().fg(Color::Green).bold(), "{") // Style::new().fg(Color::Green).dimmed().paint("{")
        )
        .unwrap();
        self.print_kvs(&mut stdout, data.kvs.iter().map(|(k, v)| (k, v)), "")
            .unwrap();
        write!(
            &mut stdout,
            "{}",
            self.styled(Style::new().fg(Color::Green).bold(), "}") // Style::new().dimmed().paint("}")
        )
        .unwrap();
        writeln!(&mut stdout).unwrap();
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<S>) {
        let mut stdout = self.stdout.lock();
        // printing the indentation
        if let Some(_) = ctx.current_span().id() {
            // size hint isn't implemented on Scope.
            let indent = ctx.scope().collect::<Vec<_>>().len();
            self.print_indent(&mut stdout, indent)
                .expect("Unable to write to stdout");
        }

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
                &mut stdout,
                "{timestamp}{unit} {level}",
                timestamp = self.styled(
                    Style::new().dimmed(),
                    elapsed.num_milliseconds().to_string()
                ),
                unit = self.styled(Style::new().dimmed(), "ms"),
                level = level,
            )
            .expect("Unable to write to stdout");
        }
        let mut visitor = FmtEvent {
            stdout,
            comma: false,
        };
        event.record(&mut visitor);
        writeln!(&mut visitor.stdout).unwrap();
    }

    fn on_close(&self, _: Id, _: Context<S>) {}
}
