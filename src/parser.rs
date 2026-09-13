use crate::datetime::DateTimeValue;
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct ParseError {
    pub pos: Pos,
    pub message: String,
    pub line_text: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "line {}, column {}: {}", self.pos.line, self.pos.col, self.message)?;
        writeln!(f, "  {}", self.line_text)?;
        write!(f, "  {}^", " ".repeat(self.pos.col.saturating_sub(1)))
    }
}

#[derive(Debug, Clone)]
pub struct VEvent {
    pub summary: String,
    pub uid: Option<String>,
    pub start: DateTimeValue,
    pub end: Option<DateTimeValue>,
}

struct RawLine<'a> {
    number: usize,
    text: &'a str,
}

fn split_raw_lines(input: &str) -> Vec<RawLine<'_>> {
    input
        .split('\n')
        .enumerate()
        .map(|(i, raw)| RawLine {
            number: i + 1,
            text: raw.strip_suffix('\r').unwrap_or(raw),
        })
        .collect()
}

struct LogicalLine {
    pos: Pos,
    text: String,
    first_physical_len: usize,
}

// RFC 5545 line folding: a line starting with a space or tab is a continuation
// of the previous line, with that one leading character removed.
fn unfold(raw_lines: &[RawLine<'_>]) -> Vec<LogicalLine> {
    let mut logical: Vec<LogicalLine> = Vec::new();
    for raw in raw_lines {
        let is_continuation =
            (raw.text.starts_with(' ') || raw.text.starts_with('\t')) && !logical.is_empty();
        if is_continuation {
            let last = logical.last_mut().unwrap();
            last.text.push_str(&raw.text[1..]);
        } else {
            logical.push(LogicalLine {
                pos: Pos { line: raw.number, col: 1 },
                text: raw.text.to_string(),
                first_physical_len: raw.text.chars().count(),
            });
        }
    }
    logical
}

// Errors only ever point at the first physical line of a logical (unfolded) line,
// so that's what gets echoed back under the caret.
fn first_physical_line(line: &LogicalLine) -> String {
    line.text.chars().take(line.first_physical_len).collect()
}

struct ContentLine {
    name: String,
    value: String,
    pos: Pos,
    value_col: usize,
}

fn find_value_colon(text: &str) -> Option<(usize, usize)> {
    let mut in_quotes = false;
    for (char_idx, (byte_idx, c)) in text.char_indices().enumerate() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => return Some((byte_idx, char_idx)),
            _ => {}
        }
    }
    None
}

fn clamp_col(col: usize, first_physical_len: usize) -> usize {
    if col <= first_physical_len + 1 {
        col
    } else {
        first_physical_len + 1
    }
}

fn parse_content_line(line: &LogicalLine) -> Result<ContentLine, ParseError> {
    let (byte_idx, char_idx) = find_value_colon(&line.text).ok_or_else(|| ParseError {
        pos: Pos { line: line.pos.line, col: line.text.chars().count() + 1 },
        message: "missing ':' — expected NAME:VALUE or NAME;PARAM=VALUE:VALUE".to_string(),
        line_text: first_physical_line(line),
    })?;

    let head = &line.text[..byte_idx];
    let value = &line.text[byte_idx + 1..];

    let name_end = head.find(';').unwrap_or(head.len());
    let name = &head[..name_end];
    if name.is_empty() {
        return Err(ParseError {
            pos: Pos { line: line.pos.line, col: 1 },
            message: "empty property name before ':'".to_string(),
            line_text: first_physical_line(line),
        });
    }

    Ok(ContentLine {
        name: name.to_ascii_uppercase(),
        value: value.to_string(),
        pos: Pos { line: line.pos.line, col: 1 },
        value_col: clamp_col(char_idx + 2, line.first_physical_len),
    })
}

fn unescape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(';') => out.push(';'),
                Some(',') => out.push(','),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

struct PartialEvent {
    begin_pos: Pos,
    begin_line_text: String,
    summary: Option<String>,
    uid: Option<String>,
    dtstart: Option<DateTimeValue>,
    dtend: Option<DateTimeValue>,
}

impl PartialEvent {
    fn new(line: &LogicalLine) -> Self {
        PartialEvent {
            begin_pos: line.pos,
            begin_line_text: first_physical_line(line),
            summary: None,
            uid: None,
            dtstart: None,
            dtend: None,
        }
    }

    fn set(&mut self, content: &ContentLine, line: &LogicalLine) -> Result<(), ParseError> {
        match content.name.as_str() {
            "SUMMARY" => self.summary = Some(unescape_text(&content.value)),
            "UID" => self.uid = Some(content.value.clone()),
            "DTSTART" => {
                self.dtstart = Some(DateTimeValue::parse(&content.value).map_err(|msg| {
                    ParseError {
                        pos: Pos { line: content.pos.line, col: content.value_col },
                        message: format!("invalid DTSTART: {}", msg),
                        line_text: first_physical_line(line),
                    }
                })?);
            }
            "DTEND" => {
                self.dtend = Some(DateTimeValue::parse(&content.value).map_err(|msg| {
                    ParseError {
                        pos: Pos { line: content.pos.line, col: content.value_col },
                        message: format!("invalid DTEND: {}", msg),
                        line_text: first_physical_line(line),
                    }
                })?);
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Result<VEvent, ParseError> {
        let dtstart = self.dtstart.ok_or_else(|| ParseError {
            pos: self.begin_pos,
            message: "this VEVENT has no DTSTART property".to_string(),
            line_text: self.begin_line_text.clone(),
        })?;
        Ok(VEvent {
            summary: self.summary.unwrap_or_else(|| "(no summary)".to_string()),
            uid: self.uid,
            start: dtstart,
            end: self.dtend,
        })
    }
}

pub fn parse_events(input: &str) -> Result<Vec<VEvent>, ParseError> {
    let raw_lines = split_raw_lines(input);
    let logical_lines = unfold(&raw_lines);

    let mut stack: Vec<String> = Vec::new();
    let mut saw_vcalendar = false;
    let mut events = Vec::new();
    let mut current: Option<PartialEvent> = None;

    for line in &logical_lines {
        if line.text.trim().is_empty() {
            continue;
        }
        let content = parse_content_line(line)?;

        match content.name.as_str() {
            "BEGIN" => {
                let component = content.value.trim().to_ascii_uppercase();
                if component == "VCALENDAR" {
                    saw_vcalendar = true;
                }
                if component == "VEVENT" && stack.last().map(String::as_str) == Some("VCALENDAR") {
                    if current.is_some() {
                        return Err(ParseError {
                            pos: content.pos,
                            message: "nested BEGIN:VEVENT — the previous VEVENT was never closed"
                                .to_string(),
                            line_text: first_physical_line(line),
                        });
                    }
                    current = Some(PartialEvent::new(line));
                }
                stack.push(component);
            }
            "END" => {
                let component = content.value.trim().to_ascii_uppercase();
                match stack.pop() {
                    Some(open) if open == component => {}
                    Some(open) => {
                        return Err(ParseError {
                            pos: content.pos,
                            message: format!(
                                "mismatched END:{} — the innermost open block is BEGIN:{}",
                                component, open
                            ),
                            line_text: first_physical_line(line),
                        });
                    }
                    None => {
                        return Err(ParseError {
                            pos: content.pos,
                            message: format!("END:{} has no matching BEGIN", component),
                            line_text: first_physical_line(line),
                        });
                    }
                }
                if component == "VEVENT" {
                    if let Some(partial) = current.take() {
                        events.push(partial.finish()?);
                    }
                }
            }
            _ => {
                if stack.last().map(String::as_str) == Some("VEVENT") {
                    if let Some(partial) = current.as_mut() {
                        partial.set(&content, line)?;
                    }
                }
            }
        }
    }

    if !saw_vcalendar {
        return Err(ParseError {
            pos: Pos { line: 1, col: 1 },
            message: "expected a BEGIN:VCALENDAR line — this doesn't look like an iCalendar file"
                .to_string(),
            line_text: raw_lines.first().map(|l| l.text.to_string()).unwrap_or_default(),
        });
    }

    if let Some(unclosed) = stack.last() {
        return Err(ParseError {
            pos: Pos { line: raw_lines.last().map(|l| l.number).unwrap_or(1), col: 1 },
            message: format!("reached end of file with BEGIN:{} still open", unclosed),
            line_text: String::new(),
        });
    }

    Ok(events)
}
