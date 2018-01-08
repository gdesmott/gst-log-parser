use std::fs::File;
use std::process::exit;
use std::collections::HashMap;

extern crate gst_log_parser;
use gst_log_parser::parse;

extern crate gstreamer as gst;
use gst::ClockTime;

extern crate structopt;
#[macro_use]
extern crate structopt_derive;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "ts-diff",
            about = "Display the timestamp difference between the previous entry from the thread")]
struct Opt {
    #[structopt(help = "Input log file")]
    input: String,
}

fn generate() -> Result<bool, std::io::Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;

    let parsed = parse(input);
    let mut previous: HashMap<String, ClockTime> = HashMap::new();

    for entry in parsed {
        let diff = match previous.get(&entry.thread) {
            Some(p) => entry.ts - *p,
            None => ClockTime::from_seconds(0),
        };
        println!(
            "{} ({}) {} {:?} {} {}:{}:{}:<{}> {}",
            entry.ts,
            diff,
            entry.thread,
            entry.level,
            entry.category,
            entry.file,
            entry.line,
            entry.function,
            entry.object.clone().unwrap_or("".to_string()),
            entry.message
        );

        previous.insert(entry.thread, entry.ts);
    }

    Ok(true)
}

fn main() {
    if generate().is_err() {
        exit(1);
    }
}
