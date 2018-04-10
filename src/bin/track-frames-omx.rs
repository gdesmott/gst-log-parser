use std::fs::File;
use std::process::exit;

extern crate gst_log_parser;
use gst_log_parser::parse;

extern crate gstreamer as gst;
use gst::ClockTime;

use std::collections::HashMap;
extern crate itertools;
use itertools::Itertools;

extern crate structopt;
#[macro_use]
extern crate structopt_derive;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "omx-ts", about = "Track progress of frames across OMX components")]
struct Opt {
    #[structopt(help = "Input file, generated with GST_DEBUG=\"OMX_PERFORMANCE:7\"")]
    input: String,
}

struct FrameInComponent {
    name: String,
    in_ts: ClockTime,
    out_ts: ClockTime,
}

impl FrameInComponent {
    fn new(name: &str) -> FrameInComponent {
        FrameInComponent {
            name: name.to_string(),
            in_ts: ClockTime::none(),
            out_ts: ClockTime::none(),
        }
    }
}

struct Frame {
    omx_ts: u64,
    components: HashMap<String, FrameInComponent>,
}

impl Frame {
    fn new(omx_ts: u64) -> Frame {
        Frame {
            omx_ts: omx_ts,
            components: HashMap::new(),
        }
    }
}

struct ComponentStats {
    n: u64,
    tot_processing_time: ClockTime,
    // when the first/last buffer has been produced
    ts_first_out: ClockTime,
    ts_last_out: ClockTime,
}

impl ComponentStats {
    fn new() -> ComponentStats {
        ComponentStats {
            n: 0,
            tot_processing_time: ClockTime::from_nseconds(0),
            ts_first_out: ClockTime::none(),
            ts_last_out: ClockTime::none(),
        }
    }

    fn average_processing_time(&self) -> ClockTime {
        ClockTime::from_nseconds(self.tot_processing_time.nseconds().unwrap() / self.n)
    }
}

fn generate() -> Result<bool, std::io::Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;
    let parsed = parse(input).filter(|entry| entry.category == "OMX_PERFORMANCE");

    let mut frames: HashMap<u64, Frame> = HashMap::new();

    for entry in parsed {
        let s = entry.message_to_struct().expect("Failed to parse struct");
        let object = entry.object.unwrap();
        // Extract the component name by taking the 4th last chars of the gst object name
        if let Some((i, _)) = object.char_indices().rev().nth(3) {
            let comp_name = &object[i..];

            let omx_ts: u64 = s.get("TimeStamp").unwrap();
            let event = s.get_name();

            let frame = frames.entry(omx_ts).or_insert(Frame::new(omx_ts));
            let comp = frame
                .components
                .entry(comp_name.to_string())
                .or_insert(FrameInComponent::new(comp_name));

            match event {
                // input: take the ts of the first buffer
                "EmptyThisBuffer" => if comp.in_ts.is_none() {
                    comp.in_ts = entry.ts
                },
                // output: take the ts of the latest buffer
                "FillBufferDone" => comp.out_ts = entry.ts,
                _ => {}
            }
        }
    }

    // Filter out frames still in OMX components
    let frames = frames.values().filter(|f| {
        for c in f.components.values() {
            if c.out_ts.is_none() {
                return false;
            }
        }
        true
    });

    // Sort by ts
    let frames = frames.sorted_by(|a, b| a.omx_ts.cmp(&b.omx_ts));
    let mut components: HashMap<String, ComponentStats> = HashMap::new();

    for frame in frames {
        let fic = frame
            .components
            .values()
            .sorted_by(|a, b| a.in_ts.cmp(&b.in_ts));

        print!("Frame: {} ", ClockTime::from_useconds(frame.omx_ts));
        for f in fic {
            let comp = components
                .entry(f.name.to_string())
                .or_insert(ComponentStats::new());
            let diff = f.out_ts - f.in_ts;

            print!(
                "[{} in: {} out: {} 𝚫: {}] ",
                f.name, f.in_ts, f.out_ts, diff
            );

            comp.tot_processing_time += diff;
            comp.n += 1;

            if comp.ts_first_out.is_none() {
                comp.ts_first_out = f.out_ts;
            }
            comp.ts_last_out = f.out_ts;
        }
        print!("\n");
    }

    println!("");
    for (name, comp) in components {
        let avg = comp.average_processing_time();
        let interval = comp.ts_last_out - comp.ts_first_out;
        let rate = comp.n as f64 / interval.seconds().unwrap() as f64;

        println!(
            "{} : nb-frames: {} avg-time: {} rate: {:.2} fps",
            name, comp.n, avg, rate
        );
    }

    Ok(true)
}

fn main() {
    if generate().is_err() {
        println!("Failed");
        exit(1);
    }
}
