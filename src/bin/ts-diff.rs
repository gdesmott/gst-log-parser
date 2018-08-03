use std::collections::HashMap;
use std::fs::File;
use std::process::exit;

extern crate gst_log_parser;
use gst_log_parser::parse;

extern crate gstreamer as gst;
use gst::ClockTime;

extern crate colored;
use colored::*;

extern crate itertools;
use itertools::Itertools;

extern crate structopt;
#[macro_use]
extern crate structopt_derive;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "ts-diff",
    about = "Display the timestamp difference between the previous entry from the thread"
)]
struct Opt {
    #[structopt(help = "Input log file")]
    input: String,
    #[structopt(
        short = "p", help = "Percentage of the longest entries to highlight", default_value = "1"
    )]
    top: usize,
}

struct TsEntry {
    entry: gst_log_parser::Entry,
    diff: ClockTime,
    top: bool,
}

impl TsEntry {
    fn new(entry: gst_log_parser::Entry, diff: ClockTime) -> TsEntry {
        TsEntry {
            entry: entry,
            diff: diff,
            top: false,
        }
    }

    fn new_top(e: TsEntry) -> TsEntry {
        TsEntry {
            entry: e.entry,
            diff: e.diff,
            top: true,
        }
    }
}

fn generate() -> Result<bool, std::io::Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;

    let parsed = parse(input);
    let mut previous: HashMap<String, ClockTime> = HashMap::new();

    // Compute ts diff
    let entries = parsed.map(|entry| {
        let diff = match previous.get(&entry.thread) {
            Some(p) => entry.ts - *p,
            None => ClockTime::from_seconds(0),
        };

        previous.insert(entry.thread.clone(), entry.ts);

        TsEntry::new(entry, diff)
    });

    // Sort by ts diff
    let entries = entries.sorted_by(|a, b| Ord::cmp(&b.diff, &a.diff));

    // Mark the top entries
    let n = entries.len() * opt.top / 100;

    let entries = entries.into_iter().enumerate().map(|(i, e)| {
        if i < n as usize {
            TsEntry::new_top(e)
        } else {
            e
        }
    });

    // Sort by ts
    let entries = entries
        .sorted_by(|a, b| Ord::cmp(&a.entry.ts, &b.entry.ts))
        .into_iter();

    // Display
    for e in entries {
        let diff = {
            if e.top {
                e.diff.to_string().red().to_string()
            } else {
                e.diff.to_string()
            }
        };

        println!(
            "{} ({}) {} {:?} {} {}:{}:{}:<{}> {}",
            e.entry.ts,
            diff,
            e.entry.thread,
            e.entry.level,
            e.entry.category,
            e.entry.file,
            e.entry.line,
            e.entry.function,
            e.entry.object.clone().unwrap_or("".to_string()),
            e.entry.message
        );
    }

    Ok(true)
}

fn main() {
    if generate().is_err() {
        exit(1);
    }
}
