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
        ClockTime::from_nseconds(self.total.nseconds().unwrap() / self.n)
    }
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;

    let mut elt_latency: HashMap<String, Count> = HashMap::new();
    let parsed = parse(input)
        .filter(|entry| entry.category == "GST_TRACER" && entry.level == DebugLevel::Trace);

    for entry in parsed {
        let s = entry
            .message_to_struct()
            .expect("Failed to parse structure");

        match s.get_name() {
            "element-latency" => {
                let count = elt_latency
                    .entry(s.get("src").expect("Missing 'src' field"))
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
    for (pad, count) in elt_latency.iter().sorted_by(|(a, _), (b, _)| a.cmp(&b)) {
        println!("  {}: {}", pad, count.mean());
    }

    Ok(())
}
