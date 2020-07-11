use ansi_term::Color;
use std::{
    fmt::{self, Write as _},
    io,
};
use tracing::{
    field::{Field, Visit},
    Level,
};

const LINE_VERT: &str = "│";
const LINE_HORIZ: &str = "─";
const LINE_BRANCH: &str = "├";

#[derive(Debug)]
pub struct Config {
    pub ansi: bool,
    pub indent_lines: bool,
    pub indent_amount: usize,
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
pub struct Buffers {
    pub current_buf: String,
    pub indent_buf: String,
}

impl Buffers {
    pub fn new() -> Self {
        Self {
            current_buf: String::new(),
            indent_buf: String::new(),
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

    pub fn indent_current(&mut self, indent: usize, config: &Config) {
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

pub struct FmtEvent<'a> {
    pub bufs: &'a mut Buffers,
    pub comma: bool,
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
    pub fn finish(&mut self, indent: usize, config: &Config) {
        self.bufs.current_buf.push('\n');
        self.bufs.indent_current(indent, config);
        self.bufs.flush_indent_buf();
    }
}

pub struct ColorLevel<'a>(pub &'a Level);

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
