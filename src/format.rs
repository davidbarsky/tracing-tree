use nu_ansi_term::Color;
use std::{
    fmt::{self, Write as _},
    io,
};
use tracing_core::{
    field::{Field, Visit},
    span, Level,
};

pub(crate) const LINE_VERT: &str = "│";
const LINE_HORIZ: &str = "─";
pub(crate) const LINE_BRANCH: &str = "├";
pub(crate) const LINE_CLOSE: &str = "┘";
pub(crate) const LINE_CLOSE2: char = '┌';
pub(crate) const LINE_OPEN: &str = "┐";
pub(crate) const LINE_OPEN2: char = '└';

#[derive(Debug, Copy, Clone)]
pub(crate) enum SpanMode {
    /// Executed on the parent before entering a child span
    PreOpen,
    Open {
        verbose: bool,
    },
    Close {
        verbose: bool,
    },
    /// A span has been entered but another *different* span has been entered in the meantime.
    Retrace {
        verbose: bool,
    },
    PostClose,
    Event,
}

#[derive(Debug)]
pub struct Config {
    /// Whether to use colors.
    pub ansi: bool,
    /// Whether an ascii art tree is used or (if false) whether to just use whitespace indent
    pub indent_lines: bool,
    /// The amount of chars to indent.
    pub indent_amount: usize,
    /// Whether to show the module paths.
    pub targets: bool,
    /// Whether to show thread ids.
    pub render_thread_ids: bool,
    /// Whether to show thread names.
    pub render_thread_names: bool,
    /// Specifies after how many indentation levels we will wrap back around to zero
    pub wraparound: usize,
    /// Whether to print the current span before activating a new one
    pub verbose_entry: bool,
    /// Whether to print the current span before exiting it.
    pub verbose_exit: bool,
    /// Print the path leading up to a span if a different span was entered concurrently
    pub span_retrace: bool,
    /// Whether to print squiggly brackets (`{}`) around the list of fields in a span.
    pub bracketed_fields: bool,
    /// Defer printing a span until an event is generated inside of it
    pub deferred_spans: bool,
    /// Print a label of the span mode (open/close etc).
    pub span_modes: bool,
}

impl Config {
    pub fn with_ansi(self, ansi: bool) -> Self {
        Self { ansi, ..self }
    }

    pub fn with_indent_lines(self, indent_lines: bool) -> Self {
        Self {
            indent_lines,
            ..self
        }
    }

    pub fn with_targets(self, targets: bool) -> Self {
        Self { targets, ..self }
    }

    pub fn with_thread_ids(self, render_thread_ids: bool) -> Self {
        Self {
            render_thread_ids,
            ..self
        }
    }

    pub fn with_thread_names(self, render_thread_names: bool) -> Self {
        Self {
            render_thread_names,
            ..self
        }
    }

    pub fn with_wraparound(self, wraparound: usize) -> Self {
        Self { wraparound, ..self }
    }

    pub fn with_verbose_entry(self, verbose_entry: bool) -> Self {
        Self {
            verbose_entry,
            ..self
        }
    }

    pub fn with_verbose_exit(self, verbose_exit: bool) -> Self {
        Self {
            verbose_exit,
            ..self
        }
    }

    pub fn with_span_retrace(self, enabled: bool) -> Self {
        Self {
            span_retrace: enabled,
            ..self
        }
    }

    pub fn with_deferred_spans(self, enable: bool) -> Self {
        Self {
            deferred_spans: enable,
            ..self
        }
    }

    pub fn with_span_modes(self, enable: bool) -> Self {
        Self {
            span_modes: enable,
            ..self
        }
    }

    pub fn with_bracketed_fields(self, bracketed_fields: bool) -> Self {
        Self {
            bracketed_fields,
            ..self
        }
    }

    pub(crate) fn prefix(&self) -> String {
        let mut buf = String::new();
        if self.render_thread_ids {
            write!(buf, "{:?}", std::thread::current().id()).unwrap();
            if buf.ends_with(')') {
                buf.truncate(buf.len() - 1);
            }
            if buf.starts_with("ThreadId(") {
                buf.drain(0.."ThreadId(".len());
            }
        }
        if self.render_thread_names {
            if let Some(name) = std::thread::current().name() {
                if self.render_thread_ids {
                    buf.push(':');
                }
                buf.push_str(name);
            }
        }
        buf
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ansi: true,
            indent_lines: false,
            indent_amount: 2,
            targets: false,
            render_thread_ids: false,
            render_thread_names: false,
            wraparound: usize::MAX,
            verbose_entry: false,
            verbose_exit: false,
            span_retrace: false,
            bracketed_fields: false,
            deferred_spans: false,
            span_modes: false,
        }
    }
}

#[derive(Debug)]
pub struct Buffers {
    pub current_buf: String,
    pub indent_buf: String,

    /// The last seen span of this layer
    ///
    /// This serves to serialize spans as two events can be generated in different spans
    /// without the spans entering and exiting beforehand. This happens for multithreaded code
    /// and instrumented futures
    pub current_span: Option<span::Id>,
}

impl Buffers {
    pub fn new() -> Self {
        Self {
            current_buf: String::new(),
            indent_buf: String::new(),
            current_span: None,
        }
    }

    pub fn flush_current_buf(&mut self, mut writer: impl io::Write) {
        write!(writer, "{}", &self.current_buf).unwrap();
        self.current_buf.clear();
    }

    pub fn flush_indent_buf(&mut self) {
        self.current_buf.push_str(&self.indent_buf);
        self.indent_buf.clear();
    }

    pub(crate) fn indent_current(&mut self, indent: usize, config: &Config, style: SpanMode) {
        let prefix = config.prefix();

        // Render something when wraparound occurs so the user is aware of it
        if config.indent_lines {
            self.current_buf.push('\n');

            match style {
                SpanMode::Close { .. } | SpanMode::PostClose => {
                    if indent > 0 && (indent + 1) % config.wraparound == 0 {
                        self.indent_buf.push_str(&prefix);
                        for _ in 0..(indent % config.wraparound * config.indent_amount) {
                            self.indent_buf.push_str(LINE_HORIZ);
                        }
                        self.indent_buf.push_str(LINE_OPEN);
                        self.indent_buf.push('\n');
                    }
                }
                _ => {}
            }
        }

        indent_block(
            &self.current_buf,
            &mut self.indent_buf,
            indent % config.wraparound,
            config.indent_amount,
            config.indent_lines,
            &prefix,
            style,
        );

        self.current_buf.clear();
        self.flush_indent_buf();

        // Render something when wraparound occurs so the user is aware of it
        if config.indent_lines {
            match style {
                SpanMode::PreOpen | SpanMode::Open { .. } => {
                    if indent > 0 && (indent + 1) % config.wraparound == 0 {
                        self.current_buf.push_str(&prefix);
                        for _ in 0..(indent % config.wraparound * config.indent_amount) {
                            self.current_buf.push_str(LINE_HORIZ);
                        }
                        self.current_buf.push_str(LINE_CLOSE);
                        self.current_buf.push('\n');
                    }
                }
                _ => {}
            }
        }
    }
}

pub struct FmtEvent<'a> {
    pub bufs: &'a mut Buffers,
    pub comma: bool,
}

impl<'a> Visit for FmtEvent<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let buf = &mut self.bufs.current_buf;
        let comma = if self.comma { "," } else { "" };
        match field.name() {
            "message" => {
                write!(buf, "{} {:?}", comma, value).unwrap();
                self.comma = true;
            }
            // Skip fields that are actually log metadata that have already been handled
            #[cfg(feature = "tracing-log")]
            name if name.starts_with("log.") => {}
            name => {
                write!(buf, "{} {}={:?}", comma, name, value).unwrap();
                self.comma = true;
            }
        }
    }
}

pub struct ColorLevel<'a>(pub &'a Level);

impl<'a> fmt::Display for ColorLevel<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self.0 {
            Level::TRACE => Color::Purple.bold().paint("TRACE"),
            Level::DEBUG => Color::Blue.bold().paint("DEBUG"),
            Level::INFO => Color::Green.bold().paint(" INFO"),
            Level::WARN => Color::Rgb(252, 234, 160).bold().paint(" WARN"), // orange
            Level::ERROR => Color::Red.bold().paint("ERROR"),
        }
        .fmt(f)
    }
}

pub(crate) fn write_span_mode(buf: &mut String, style: SpanMode) {
    match style {
        SpanMode::Open { verbose: true } => buf.push_str("open(v)"),
        SpanMode::Open { verbose: false } => buf.push_str("open"),
        SpanMode::Retrace { verbose: false } => buf.push_str("retrace"),
        SpanMode::Retrace { verbose: true } => buf.push_str("retrace(v)"),
        SpanMode::Close { verbose: true } => buf.push_str("close(v)"),
        SpanMode::Close { verbose: false } => buf.push_str("close"),
        SpanMode::PreOpen => buf.push_str("pre_open"),
        SpanMode::PostClose => buf.push_str("post_close"),
        SpanMode::Event => buf.push_str("event"),
    }

    buf.push_str(": ")
}

fn indent_block_with_lines(
    lines: &[&str],
    buf: &mut String,
    indent: usize,
    // width of one level of indent
    indent_amount: usize,
    prefix: &str,
    style: SpanMode,
) {
    let indent_spaces = indent * indent_amount;

    if lines.is_empty() {
        return;
    } else if indent_spaces == 0 {
        for line in lines {
            buf.push_str(prefix);
            // The first indent is special, we only need to print open/close and nothing else
            if indent == 0 {
                match style {
                    SpanMode::Open { .. } => buf.push_str(LINE_OPEN),
                    SpanMode::Retrace { .. } => buf.push_str(LINE_OPEN),
                    SpanMode::Close { .. } => buf.push_str(LINE_CLOSE),
                    SpanMode::PreOpen | SpanMode::PostClose => {}
                    SpanMode::Event => {}
                }
            }
            buf.push_str(line);
            buf.push('\n');
        }
        return;
    }

    let mut s = String::with_capacity(indent_spaces + prefix.len());
    s.push_str(prefix);

    for _ in 0..(indent_spaces - indent_amount) {
        s.push(' ');
    }

    // draw branch
    buf.push_str(&s);

    match style {
        SpanMode::PreOpen => {
            buf.push(LINE_OPEN2);
            for _ in 1..(indent_amount / 2) {
                buf.push_str(LINE_HORIZ);
            }
            buf.push_str(LINE_OPEN);
        }
        SpanMode::Open { verbose: false } | SpanMode::Retrace { verbose: false } => {
            buf.push(LINE_OPEN2);
            for _ in 1..indent_amount {
                buf.push_str(LINE_HORIZ);
            }
            buf.push_str(LINE_OPEN);
        }
        SpanMode::Open { verbose: true } | SpanMode::Retrace { verbose: true } => {
            buf.push(' ');
            for _ in 1..(indent_amount / 2) {
                buf.push(' ');
            }
            // We don't have the space for fancy rendering at single space indent.
            if indent_amount > 1 {
                buf.push(LINE_OPEN2);
            }
            for _ in (indent_amount / 2)..(indent_amount - 1) {
                buf.push_str(LINE_HORIZ);
            }
            // We don't have the space for fancy rendering at single space indent.
            if indent_amount > 1 {
                buf.push_str(LINE_OPEN);
            } else {
                buf.push(' ');
            }
        }
        SpanMode::Close { verbose: false } => {
            buf.push(LINE_CLOSE2);
            for _ in 1..indent_amount {
                buf.push_str(LINE_HORIZ);
            }
            buf.push_str(LINE_CLOSE);
        }
        SpanMode::Close { verbose: true } => {
            buf.push(' ');
            for _ in 1..(indent_amount / 2) {
                buf.push(' ');
            }
            // We don't have the space for fancy rendering at single space indent.
            if indent_amount > 1 {
                buf.push(LINE_CLOSE2);
            }
            for _ in (indent_amount / 2)..(indent_amount - 1) {
                buf.push_str(LINE_HORIZ);
            }
            // We don't have the space for fancy rendering at single space indent.
            if indent_amount > 1 {
                buf.push_str(LINE_CLOSE);
            } else {
                buf.push(' ');
            }
        }
        SpanMode::PostClose => {
            buf.push(LINE_CLOSE2);
            for _ in 1..(indent_amount / 2) {
                buf.push_str(LINE_HORIZ);
            }
            buf.push_str(LINE_CLOSE);
        }
        SpanMode::Event => {
            buf.push_str(LINE_BRANCH);

            // add `indent_amount - 1` horizontal lines before the span/event
            for _ in 0..(indent_amount - 1) {
                buf.push_str(LINE_HORIZ);
            }
        }
    }
    buf.push_str(lines[0]);
    buf.push('\n');

    // add the rest of the indentation, since we don't want to draw horizontal lines
    // for subsequent lines
    match style {
        SpanMode::Open { .. } | SpanMode::Retrace { .. } => s.push_str("  "),
        SpanMode::Close { .. } => s.push(' '),
        _ => {}
    }
    s.push_str(LINE_VERT);
    for _ in 1..=indent_amount {
        s.push(' ');
    }

    // add all of the actual content, with each line preceded by the indent string
    for line in &lines[1..] {
        buf.push_str(&s);
        buf.push_str(line);
        buf.push('\n');
    }
}

fn indent_block(
    block: &str,
    buf: &mut String,
    mut indent: usize,
    indent_amount: usize,
    indent_lines: bool,
    prefix: &str,
    style: SpanMode,
) {
    let lines: Vec<&str> = block.lines().collect();
    let indent_spaces = indent * indent_amount;
    buf.reserve(block.len() + (lines.len() * indent_spaces));

    // The PreOpen and PostClose need to match up with the indent of the entered child span one more indent
    // deep
    match style {
        SpanMode::PreOpen | SpanMode::PostClose => {
            indent += 1;
        }
        _ => (),
    }

    if indent_lines {
        indent_block_with_lines(&lines, buf, indent, indent_amount, prefix, style);
    } else {
        let mut indent_str = " ".repeat(indent_spaces);
        let mut first_line = true;
        for line in lines {
            buf.push_str(prefix);
            buf.push(' ');
            buf.push_str(&indent_str);
            if first_line {
                first_line = false;
                indent_str.push_str("  ");
            }
            buf.push_str(line);
            buf.push('\n');
        }
    }
}
