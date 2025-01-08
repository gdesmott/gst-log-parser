// Copyright (C) 2025 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::{collections::HashMap, fs::File};

use gst_log_parser::parse;
use gstreamer as gst;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "glcontext",
    about = "Check for how long it takes for functions to be scheduled in the GL thread"
)]
struct Opt {
    #[structopt(help = "Input file")]
    input: String,
}

fn print_stats(times: Vec<gst::ClockTime>) {
    let n = times.len();
    println!("{n} function calls. Schedule times:");
    if n == 0 {
        return;
    }
    let (min_idx, min) = times
        .iter()
        .enumerate()
        .min_by(|(_a_idx, a), (_b_idx, b)| a.cmp(b))
        .unwrap();
    let (max_idx, max) = times
        .iter()
        .enumerate()
        .max_by(|(_a_idx, a), (_b_idx, b)| a.cmp(b))
        .unwrap();
    println!("  min: {min} (call {min_idx})");
    println!("  max: {max} (call {max_idx})");
    let sum: gst::ClockTime = times.into_iter().sum();
    let avg = sum.nseconds() / n as u64;
    let avg = gst::ClockTime::from_nseconds(avg);
    println!("  avg: {avg}");
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let f = File::open(opt.input)?;

    let add_re = regex::Regex::new(r"schedule function:(?<function>.*) data:(?<data>.*)")?;
    let run_re = regex::Regex::new(r"running function:(?<function>.*) data:(?<data>.*)")?;
    let mut pendings = HashMap::new();
    let mut times = vec![];

    let parsed = parse(f);
    for entry in parsed
        .filter(|entry| entry.category == "glcontext" && entry.level == gst::DebugLevel::Trace)
    {
        match entry.function.as_str() {
            "gst_gl_context_thread_add" => {
                let Some(capture) = add_re.captures(&entry.message) else {
                    continue;
                };
                pendings.insert(
                    (
                        (capture["function"]).to_string(),
                        (capture["data"]).to_string(),
                    ),
                    entry.ts,
                );
            }
            "_gst_gl_context_thread_run_generic" => {
                let Some(capture) = run_re.captures(&entry.message) else {
                    continue;
                };
                if let Some(pending) = pendings.remove(&(
                    (capture["function"]).to_string(),
                    (capture["data"]).to_string(),
                )) {
                    times.push(entry.ts - pending);
                }
            }
            _ => {}
        }
    }

    print_stats(times);

    Ok(())
}
