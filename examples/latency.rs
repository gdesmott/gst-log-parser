// Copyright (C) 2017-2019 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Generate input logs with: GST_DEBUG="GST_TRACER:7" GST_TRACERS=latency\(flags="pipeline+element+reported"\)

use failure::Error;
use gst_log_parser::parse;
use gstreamer::{ClockTime, DebugLevel};
use itertools::Itertools;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "latency")]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: PathBuf,
}

#[derive(Debug)]
struct Count {
    n: u64,
    total: ClockTime,
}

impl Count {
    fn new() -> Self {
        Self {
            n: 0,
            total: ClockTime::from_nseconds(0),
        }
    }

    fn mean(&self) -> ClockTime {
        ClockTime::from_nseconds(self.total.nseconds() / self.n)
    }
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;

    let mut elt_latency: HashMap<String, Count> = HashMap::new();
    let parsed = parse(input)
        .filter(|entry| entry.category == "GST_TRACER" && entry.level == DebugLevel::Trace);

    for entry in parsed {
        let s = match entry.message_to_struct() {
            None => continue,
            Some(s) => s,
        };
        match s.name() {
            "element-latency" => {
                let count = elt_latency
                    .entry(s.get::<String>("src").expect("Missing 'src' field"))
                    .or_insert_with(Count::new);

                count.n += 1;
                let time: u64 = s.get("time").expect("Missing 'time' field");
                count.total += ClockTime::from_nseconds(time);
            }
            "latency" => { /* TODO */ }
            "element-reported-latency" => { /* TODO */ }
            _ => {}
        };
    }

    println!("Mean latency:");
    // Sort by pad name so we can easily compare results
    for (pad, count) in elt_latency.iter().sorted_by(|(a, _), (b, _)| a.cmp(b)) {
        println!("  {}: {}", pad, count.mean());
    }

    Ok(())
}
