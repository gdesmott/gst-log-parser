// Copyright (C) 2017-2019 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

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
#[macro_use]
extern crate failure;

use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum TimestampField {
    Hour,
    Minute,
    Second,
    SubSecond,
}

#[derive(Debug, PartialEq)]
pub enum Token {
    Timestamp { field: Option<TimestampField> },
    PID,
    Thread,
    Level,
    Category,
    File,
    LineNumber,
    Function,
    Message,
    Object,
}

#[derive(Debug, Fail, PartialEq)]
pub enum ParsingError {
    #[fail(display = "invalid debug level: {}", name)]
    InvalidDebugLevel { name: String },
    #[fail(display = "invalid timestamp: {} : {:?}", ts, field)]
    InvalidTimestamp { ts: String, field: TimestampField },
    #[fail(display = "missing token: {:?}", t)]
    MissingToken { t: Token },
    #[fail(display = "invalid PID: {}", pid)]
    InvalidPID { pid: String },
    #[fail(display = "missing location")]
    MissingLocation,
    #[fail(display = "invalid line number: {}", line)]
    InvalidLineNumber { line: String },
}

#[derive(Debug)]
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
        _ => Err(ParsingError::InvalidDebugLevel {
            name: s.to_string(),
        }),
    }
}

fn parse_time(ts: &str) -> Result<ClockTime, ParsingError> {
    let mut split = ts.splitn(3, ':');
    let h: u64 = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken {
            t: Token::Timestamp {
                field: Some(TimestampField::Hour),
            },
        })?
        .parse()
        .map_err(|_e| ParsingError::InvalidTimestamp {
            ts: ts.to_string(),
            field: TimestampField::Hour,
        })?;

    let m: u64 = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken {
            t: Token::Timestamp {
                field: Some(TimestampField::Minute),
            },
        })?
        .parse()
        .map_err(|_e| ParsingError::InvalidTimestamp {
            ts: ts.to_string(),
            field: TimestampField::Minute,
        })?;

    split = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken {
            t: Token::Timestamp {
                field: Some(TimestampField::Second),
            },
        })?
        .splitn(2, '.');
    let secs: u64 = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken {
            t: Token::Timestamp {
                field: Some(TimestampField::Second),
            },
        })?
        .parse()
        .map_err(|_e| ParsingError::InvalidTimestamp {
            ts: ts.to_string(),
            field: TimestampField::Second,
        })?;

    let subsecs: u64 = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken {
            t: Token::Timestamp {
                field: Some(TimestampField::SubSecond),
            },
        })?
        .parse()
        .map_err(|_e| ParsingError::InvalidTimestamp {
            ts: ts.to_string(),
            field: TimestampField::SubSecond,
        })?;

    Ok(ClockTime::from_seconds(h * 60 * 60 + m * 60 + secs) + ClockTime::from_nseconds(subsecs))
}

fn split_location(location: &str) -> Result<(String, u32, String, Option<String>), ParsingError> {
    let mut split = location.splitn(4, ':');
    let file = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken { t: Token::File })?;
    let line_str = split.next().ok_or_else(|| ParsingError::MissingToken {
        t: Token::LineNumber,
    })?;
    let line = line_str
        .parse()
        .map_err(|_e| ParsingError::InvalidLineNumber {
            line: line_str.to_string(),
        })?;

    let function = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken { t: Token::Function })?;

    let object = split
        .next()
        .ok_or_else(|| ParsingError::MissingToken { t: Token::Object })?;

    let object_name = {
        if !object.is_empty() {
            let object = object
                .to_string()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string();

            Some(object)
        } else {
            None
        }
    };

    Ok((file.to_string(), line, function.to_string(), object_name))
}

impl Entry {
    fn new(line: &str) -> Result<Entry, ParsingError> {
        // Strip color codes
        lazy_static! {
            static ref RE: Regex = Regex::new("\x1b\\[[0-9;]*m").unwrap();
        }
        let line = RE.replace_all(&line, "");

        let mut it = line.split(' ');
        let ts_str = it.next().ok_or_else(|| ParsingError::MissingToken {
            t: Token::Timestamp { field: None },
        })?;
        let ts = parse_time(ts_str)?;

        let mut it = it.skip_while(|x| x.is_empty());
        let pid_str = it
            .next()
            .ok_or_else(|| ParsingError::MissingToken { t: Token::PID })?;
        let pid = pid_str.parse().map_err(|_e| ParsingError::InvalidPID {
            pid: pid_str.to_string(),
        })?;

        let mut it = it.skip_while(|x| x.is_empty());
        let thread = it
            .next()
            .ok_or_else(|| ParsingError::MissingToken { t: Token::Thread })?
            .to_string();

        let mut it = it.skip_while(|x| x.is_empty());
        let level_str = it
            .next()
            .ok_or_else(|| ParsingError::MissingToken { t: Token::Level })?;
        let level = parse_debug_level(level_str)?;

        let mut it = it.skip_while(|x| x.is_empty());
        let category = it
            .next()
            .ok_or_else(|| ParsingError::MissingToken { t: Token::Category })?
            .to_string();

        let mut it = it.skip_while(|x| x.is_empty());
        let location_str = it.next().ok_or_else(|| ParsingError::MissingLocation)?;
        let (file, line, function, object) = split_location(location_str)?;
        let message: String = join(it, " ");

        Ok(Entry {
            ts,
            pid,
            thread,
            level,
            category,
            file,
            line,
            function,
            object,
            message,
        })
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
            self.object.clone().unwrap_or_else(|| "".to_string()),
            self.message
        )
    }
}

pub struct ParserIterator<R: Read> {
    lines: Lines<BufReader<R>>,
}

impl<R: Read> ParserIterator<R> {
    fn new(lines: Lines<BufReader<R>>) -> Self {
        Self { lines }
    }
}

impl<R: Read> Iterator for ParserIterator<R> {
    type Item = Entry;

    fn next(&mut self) -> Option<Entry> {
        match self.lines.next() {
            None => None,
            Some(line) => match Entry::new(&line.unwrap()) {
                Ok(entry) => Some(entry),
                Err(_err) => None,
            },
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

    #[test]
    fn corrupted() {
        let f = File::open("test-logs/corrupted-nocolor.log").expect("Failed to open log file");
        let mut parsed = parse(f);

        assert!(parsed.next().is_none());
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
    fn timestamps() {
        assert!(Entry::new("foo").is_err());

        let e1 = "e:00:00.007773544  8874 0x558951015c00 INFO                GST_INIT gst.c:510:init_pre: Init";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::InvalidTimestamp {
                    ts: "e:00:00.007773544".to_string(),
                    field: TimestampField::Hour,
                }
            ),
        };

        let e1 = ":00:00.007773544  8874 0x558951015c00 INFO                GST_INIT gst.c:510:init_pre: Init";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::InvalidTimestamp {
                    ts: ":00:00.007773544".to_string(),
                    field: TimestampField::Hour,
                }
            ),
        };

        let e1 = "8874 0x558951015c00 INFO                GST_INIT gst.c:510:init_pre: Init";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::MissingToken {
                    t: Token::Timestamp {
                        field: Some(TimestampField::Minute)
                    },
                }
            ),
        };
    }

    #[test]
    fn pid() {
        let e1 = "00:00:00.007773544 ";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingToken { t: Token::PID }),
        };

        let e1 = "00:00:00.007773544  8fuz874 0x558951015c00 INFO                GST_INIT gst.c:510:init_pre: Init";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::InvalidPID {
                    pid: "8fuz874".to_string(),
                }
            ),
        };
    }

    #[test]
    fn thread() {
        let e1 = "00:00:00.007773544  8874 ";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingToken { t: Token::Thread }),
        };
    }

    #[test]
    fn debug_level() {
        let e1 = "00:00:00.007773544  8874 0x558951015c00 ";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingToken { t: Token::Level }),
        };

        let e1 = "00:00:00.007773544  8874 0x558951015c00 FUZZLEVEL                GST_INIT gst.c:510:init_pre: Init";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::InvalidDebugLevel {
                    name: "FUZZLEVEL".to_string(),
                }
            ),
        };
    }

    #[test]
    fn category() {
        let e1 = "00:00:00.007773544  8874 0x558951015c00 INFO";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingToken { t: Token::Category }),
        };
    }

    #[test]
    fn location() {
        let e1 = "00:00:00.007773544  8874 0x558951015c00 INFO GST_INIT";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingLocation {}),
        };

        let e1 = "00:00:00.007773544  8874 0x558951015c00 INFO GST_INIT gst.c";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::MissingToken {
                    t: Token::LineNumber
                }
            ),
        };

        let e1 = "00:00:00.007773544  8874 0x558951015c00 INFO GST_INIT gst.c:fuzz";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(
                e,
                ParsingError::InvalidLineNumber {
                    line: "fuzz".to_string()
                }
            ),
        };

        let e1 = "00:00:00.007773544  8874 0x558951015c00 INFO GST_INIT gst.c:510";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingToken { t: Token::Function }),
        };

        let e1 = "00:00:00.007773544  8874 0x558951015c00 INFO GST_INIT gst.c:510:";
        match Entry::new(e1) {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e, ParsingError::MissingToken { t: Token::Object }),
        };
    }
}
