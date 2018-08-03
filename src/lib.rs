use std::fmt;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Lines;
use std::io::Read;
use std::str;
extern crate itertools;
use itertools::join;

extern crate gstreamer as gst;
use gst::{ClockTime, DebugLevel, Structure};

#[macro_use]
extern crate lazy_static;
extern crate regex;
use regex::Regex;

#[derive(Debug)]
pub struct ParsingError;

pub struct Entry {
    pub ts: ClockTime,
    pub pid: u32,
    pub thread: String,
    pub level: DebugLevel,
    pub category: String,
    pub file: String,
    pub line: u32,
    pub function: String,
    pub message: String,
    pub object: Option<String>,
}

fn parse_debug_level(s: &str) -> Result<DebugLevel, ParsingError> {
    match s {
        "ERROR" => Ok(DebugLevel::Error),
        "WARN" => Ok(DebugLevel::Warning),
        "FIXME" => Ok(DebugLevel::Fixme),
        "INFO" => Ok(DebugLevel::Info),
        "DEBUG" => Ok(DebugLevel::Debug),
        "LOG" => Ok(DebugLevel::Log),
        "TRACE" => Ok(DebugLevel::Trace),
        "MEMDUMP" => Ok(DebugLevel::Memdump),
        _ => Err(ParsingError),
    }
}

fn parse_time(ts: &str) -> ClockTime {
    let mut split = ts.splitn(3, ':');
    let h: u64 = split
        .next()
        .expect("missing hour")
        .parse()
        .expect("invalid hour");
    let m: u64 = split
        .next()
        .expect("missing minute")
        .parse()
        .expect("invalid minute");
    split = split.next().expect("missing second").splitn(2, '.');
    let secs: u64 = split
        .next()
        .expect("missing second")
        .parse()
        .expect("invalid second");
    let subsecs: u64 = split
        .next()
        .expect("missing sub second")
        .parse()
        .expect("invalid sub second");

    ClockTime::from_seconds(h * 60 * 60 + m * 60 + secs) + ClockTime::from_nseconds(subsecs)
}

fn split_location(location: &str) -> (String, u32, String, Option<String>) {
    let mut split = location.splitn(4, ":");
    let file = split.next().expect("missing file");
    let line = split
        .next()
        .expect("missing line")
        .parse()
        .expect("invalid line");
    let function = split.next().expect("missing function");
    let object = split.next().expect("missing object delimiter");
    let object_name = {
        if object.len() > 0 {
            let object = object
                .to_string()
                .trim_left_matches("<")
                .trim_right_matches(">")
                .to_string();

            Some(object)
        } else {
            None
        }
    };

    (file.to_string(), line, function.to_string(), object_name)
}

impl Entry {
    fn new(line: String) -> Entry {
        // Strip color codes
        lazy_static! {
            static ref RE: Regex = Regex::new("\x1b\\[[0-9;]*m").unwrap();
        }
        let line = RE.replace_all(&line, "");

        let mut it = line.split(" ");
        let ts = parse_time(it.next().expect("Missing ts"));
        let mut it = it.skip_while(|x| x.is_empty());
        let pid = it
            .next()
            .expect("Missing PID")
            .parse()
            .expect("Failed to parse PID");
        let mut it = it.skip_while(|x| x.is_empty());
        let thread = it.next().expect("Missing thread").to_string();
        let mut it = it.skip_while(|x| x.is_empty());
        let level = parse_debug_level(it.next().expect("Missing level")).expect("Invalid level");
        let mut it = it.skip_while(|x| x.is_empty());
        let category = it.next().expect("Missing Category").to_string();
        let mut it = it.skip_while(|x| x.is_empty());
        let (file, line, function, object) = split_location(it.next().expect("Missing location"));
        let message: String = join(it, " ");

        Entry {
            ts: ts,
            pid: pid,
            thread: thread,
            level: level,
            category: category,
            file: file,
            line: line,
            function: function,
            object: object,
            message: message,
        }
    }

    pub fn message_to_struct(&self) -> Option<Structure> {
        Structure::from_string(&self.message)
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}  {} {} {:?} {} {}:{}:{}:<{}> {}",
            self.ts,
            self.pid,
            self.thread,
            self.level,
            self.category,
            self.file,
            self.line,
            self.function,
            self.object.clone().unwrap_or("".to_string()),
            self.message
        )
    }
}

pub struct ParserIterator<R: Read> {
    lines: Lines<BufReader<R>>,
}

impl<R: Read> ParserIterator<R> {
    fn new(lines: Lines<BufReader<R>>) -> Self {
        Self { lines: lines }
    }
}

impl<R: Read> Iterator for ParserIterator<R> {
    type Item = Entry;

    fn next(&mut self) -> Option<Entry> {
        match self.lines.next() {
            None => None,
            Some(line) => Some(Entry::new(line.unwrap())),
        }
    }
}

pub fn parse<R: Read>(r: R) -> ParserIterator<R> {
    gst::init().expect("Failed to initialize gst");

    let file = BufReader::new(r);

    ParserIterator::new(file.lines())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn no_color() {
        let f = File::open("test-logs/nocolor.log").expect("Failed to open log file");
        let mut parsed = parse(f);

        let entry = parsed.next().expect("First entry missing");
        assert_eq!(entry.ts.nanoseconds().unwrap(), 7773544);
        assert_eq!(format!("{}", entry.ts), "00:00:00.007773544");
        assert_eq!(entry.pid, 8874);
        assert_eq!(entry.thread, "0x558951015c00");
        assert_eq!(entry.level, DebugLevel::Info);
        assert_eq!(entry.category, "GST_INIT");
        assert_eq!(entry.file, "gst.c");
        assert_eq!(entry.line, 510);
        assert_eq!(entry.function, "init_pre");
        assert_eq!(
            entry.message,
            "Initializing GStreamer Core Library version 1.10.4"
        );

        let entry = parsed.nth(3).expect("3th entry missing");
        assert_eq!(entry.message, "0x55895101d040 ref 1->2");
        assert_eq!(entry.object, Some("allocatorsysmem0".to_string()));
    }

    #[test]
    fn color() {
        let f = File::open("test-logs/color.log").expect("Failed to open log file");
        let mut parsed = parse(f);

        let entry = parsed.next().expect("First entry missing");
        assert_eq!(entry.ts.nanoseconds().unwrap(), 208614);
        assert_eq!(format!("{}", entry.ts), "00:00:00.000208614");
        assert_eq!(entry.pid, 17267);
        assert_eq!(entry.thread, "0x2192200");
        assert_eq!(entry.level, DebugLevel::Info);
        assert_eq!(entry.category, "GST_INIT");
        assert_eq!(entry.file, "gst.c");
        assert_eq!(entry.line, 584);
        assert_eq!(entry.function, "init_pre");
        assert_eq!(
            entry.message,
            "Initializing GStreamer Core Library version 1.13.0.1"
        );

        assert_eq!(parsed.count(), 14);
    }
}
