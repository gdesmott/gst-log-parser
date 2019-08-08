// Copyright (C) 2017-2019 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Generate input logs with: GST_DEBUG="GST_TRACER:7" GST_TRACERS=latency

use failure::Error;
use gst_log_parser::parse;
use gstreamer::{DebugLevel, MSECOND_VAL};
use std::fs::File;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug, PartialEq, Copy, Clone)]
#[structopt(name = "command")]
enum Command {
    #[structopt(
        name = "filter-higher",
        about = "Check for latency higher than a value"
    )]
    FilterHigher {
        #[structopt(help = "The minimum latency to display, in ms")]
        min: u64,
    },
}

#[derive(StructOpt, Debug)]
#[structopt(
    name = "check-latency",
    about = "Process logs generated by the 'latency' tracer"
)]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: PathBuf,
    #[structopt(subcommand)]
    command: Command,
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;

    let parsed = parse(input)
        .filter(|entry| entry.category == "GST_TRACER" && entry.level == DebugLevel::Trace);

    for entry in parsed {
        let s = match entry.message_to_struct() {
            None => continue,
            Some(s) => s,
        };

        if s.get_name() != "latency" {
            continue;
        }

        let latency = s.get::<u64>("time").unwrap();
        match opt.command {
            Command::FilterHigher { min } => {
                if latency >= min * MSECOND_VAL {
                    println!("{}", entry);
                }
            }
        }
    }

    Ok(())
}