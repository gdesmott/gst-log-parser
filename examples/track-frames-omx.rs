// Copyright (C) 2017-2019 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fs::File;
use std::process::exit;

use anyhow::Result;
use gst_log_parser::parse;
use gstreamer::ClockTime;
use itertools::Itertools;
use std::collections::HashMap;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "omx-ts",
    about = "Track progress of frames across OMX components"
)]
struct Opt {
    #[structopt(help = "Input file, generated with GST_DEBUG=\"OMX_API_TRACE:7\"")]
    input: String,
}

#[derive(Debug)]
struct FrameInComponent {
    name: String,

    // enters component (EmptyThisBuffer)
    empty_ts: Vec<ClockTime>,
    // leaves component (FillBufferDone)
    fill_done_ts: Vec<ClockTime>,
}

impl FrameInComponent {
    fn new(name: &str) -> FrameInComponent {
        FrameInComponent {
            name: name.to_string(),
            empty_ts: Vec::new(),
            fill_done_ts: Vec::new(),
        }
    }

    // ts when first buffer of the frame entered the OMX component
    fn first_buffer_enter_ts(&self) -> ClockTime {
        self.empty_ts[0]
    }

    // ts when last buffer of the frame entered the OMX component
    fn last_buffer_enter_ts(&self) -> ClockTime {
        *self.empty_ts.last().unwrap()
    }

    // ts when first buffer of the frame left the OMX component
    fn first_buffer_left_ts(&self) -> ClockTime {
        self.fill_done_ts[0]
    }

    // ts when last buffer of the frame left the OMX component
    fn last_buffer_left_ts(&self) -> ClockTime {
        *self.fill_done_ts.last().unwrap()
    }

    fn total_time(&self) -> ClockTime {
        self.last_buffer_left_ts() - self.first_buffer_enter_ts()
    }

    fn n_input(&self) -> usize {
        self.empty_ts.len()
    }

    fn n_output(&self) -> usize {
        self.fill_done_ts.len()
    }
}

#[derive(Debug)]
struct Frame {
    omx_ts: u64,
    components: HashMap<String, FrameInComponent>,
}

impl Frame {
    fn new(omx_ts: u64) -> Frame {
        Frame {
            omx_ts,
            components: HashMap::new(),
        }
    }
}

#[derive(Debug)]
struct ComponentStats {
    n: u64,
    tot_processing_time: ClockTime,
    // when the first/last buffer has been produced
    ts_first_out: Option<ClockTime>,
    ts_last_out: Option<ClockTime>,
}

impl ComponentStats {
    fn new() -> ComponentStats {
        ComponentStats {
            n: 0,
            tot_processing_time: ClockTime::from_nseconds(0),
            ts_first_out: None,
            ts_last_out: None,
        }
    }

    fn average_processing_time(&self) -> ClockTime {
        ClockTime::from_nseconds(self.tot_processing_time.nseconds() / self.n)
    }
}

#[derive(Debug)]
struct CbTime {
    fill_done_ts: Option<ClockTime>,
    fill_done_tot: ClockTime,
    fill_done_max: ClockTime,
    fill_done_n: u64,

    empty_done_ts: Option<ClockTime>,
    empty_done_tot: ClockTime,
    empty_done_max: ClockTime,
    empty_done_n: u64,
}

impl CbTime {
    fn new() -> CbTime {
        CbTime {
            fill_done_ts: None,
            fill_done_tot: ClockTime::from_nseconds(0),
            fill_done_max: ClockTime::from_nseconds(0),
            fill_done_n: 0,

            empty_done_ts: None,
            empty_done_tot: ClockTime::from_nseconds(0),
            empty_done_max: ClockTime::from_nseconds(0),
            empty_done_n: 0,
        }
    }

    fn fill_done_average(&self) -> ClockTime {
        ClockTime::from_nseconds(self.fill_done_tot.nseconds() / self.fill_done_n)
    }

    fn empty_done_average(&self) -> ClockTime {
        ClockTime::from_nseconds(self.empty_done_tot.nseconds() / self.empty_done_n)
    }
}

fn generate() -> Result<bool> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;
    let parsed = parse(input).filter(|entry| entry.category == "OMX_API_TRACE");

    let mut frames: HashMap<u64, Frame> = HashMap::new();
    // comp -> CbTime
    let mut cbs: HashMap<String, CbTime> = HashMap::new();

    for entry in parsed {
        let s = entry.message_to_struct().expect("Failed to parse struct");
        let object = entry.object.unwrap();
        // Extract the component name by taking the 4th last chars of the gst object name
        if let Some((i, _)) = object.char_indices().rev().nth(3) {
            let comp_name = &object[i..];

            let omx_ts = s.get::<Option<ClockTime>>("TimeStamp")?;
            if omx_ts.is_none() {
                continue;
            }
            let omx_ts = omx_ts.unwrap().nseconds();

            let event = s.name();

            let frame = frames.entry(omx_ts).or_insert_with(|| Frame::new(omx_ts));
            let comp = frame
                .components
                .entry(comp_name.to_string())
                .or_insert_with(|| FrameInComponent::new(comp_name));
            let cb = cbs.entry(comp_name.to_string()).or_insert_with(CbTime::new);

            match event {
                // input
                "EmptyThisBuffer" => {
                    comp.empty_ts.push(entry.ts);
                }
                // output
                "FillBufferDone" => {
                    // Ignore empty output buffers
                    let filled = s.get::<u32>("FilledLen")?;
                    if filled == 0 {
                        continue;
                    }
                    comp.fill_done_ts.push(entry.ts);
                    cb.fill_done_ts = Some(entry.ts);
                }
                "FillBufferDone-FINISHED" => {
                    if let Some(fill_done_ts) = cb.fill_done_ts {
                        let diff = entry.ts - fill_done_ts;
                        cb.fill_done_tot += diff;
                        if cb.fill_done_max < diff {
                            cb.fill_done_max = diff;
                        }
                        cb.fill_done_n += 1;
                    }
                }
                "EmptyBufferDone" => cb.empty_done_ts = Some(entry.ts),
                "EmptyBufferDone-FINISHED" => {
                    if let Some(empty_done_ts) = cb.empty_done_ts {
                        let diff = entry.ts - empty_done_ts;
                        cb.empty_done_tot += diff;
                        if cb.empty_done_max < diff {
                            cb.empty_done_max = diff;
                        }
                        cb.empty_done_n += 1;
                    }
                }
                _ => {}
            }
        }
    }

    // Filter out frames still in OMX components
    let frames = frames.values().filter(|f| {
        for c in f.components.values() {
            if c.fill_done_ts.is_empty() {
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
            .sorted_by(|a, b| a.empty_ts[0].cmp(&b.empty_ts[0]));

        print!("Frame: {} ", ClockTime::from_useconds(frame.omx_ts));
        for f in fic {
            let comp = components
                .entry(f.name.to_string())
                .or_insert_with(ComponentStats::new);
            let diff = f.total_time();

            print!(
                "\n\t[{} fst-in: {} lst-out: {} 𝚫: {}",
                f.name,
                f.first_buffer_enter_ts(),
                f.last_buffer_left_ts(),
                diff,
            );

            if f.n_input() > 1 {
                print!(
                    " lst-in:  {} 𝚫(fst-in): {}",
                    f.last_buffer_enter_ts(),
                    f.last_buffer_enter_ts() - f.first_buffer_enter_ts()
                );
            }

            if f.n_output() > 1 {
                print!(
                    " fst-out: {} 𝚫(lst-out): {} 𝚫(fst-in): {}",
                    f.first_buffer_left_ts(),
                    f.last_buffer_left_ts() - f.first_buffer_left_ts(),
                    f.first_buffer_left_ts() - f.first_buffer_enter_ts()
                );
            }

            print!("]");

            comp.tot_processing_time += diff;
            comp.n += 1;

            if comp.ts_first_out.is_none() {
                comp.ts_first_out = Some(f.last_buffer_left_ts());
            }
            comp.ts_last_out = Some(f.last_buffer_left_ts());
        }
        println!();
    }

    println!();
    for (name, comp) in components {
        let avg = comp.average_processing_time();
        if let Some(ts_last_out) = comp.ts_last_out {
            if let Some(ts_first_out) = comp.ts_first_out {
                let interval = ts_last_out - ts_first_out;
                let rate = comp.n as f64 / interval.seconds() as f64;

                println!(
                    "{} : nb-frames: {} avg-time: {} rate: {:.2} fps",
                    name, comp.n, avg, rate
                );
            }
        }
    }

    println!();
    for (name, cb) in cbs {
        if cb.empty_done_n > 0 {
            println!(
                "{} EmptyBufferDone n: {} tot: {} avg: {} max: {}",
                name,
                cb.empty_done_n,
                cb.empty_done_tot,
                cb.empty_done_average(),
                cb.empty_done_max
            );
        }
        if cb.fill_done_n > 0 {
            println!(
                "{} FillBufferDone n: {} tot: {} avg: {} max: {}",
                name,
                cb.fill_done_n,
                cb.fill_done_tot,
                cb.fill_done_average(),
                cb.fill_done_max
            );
        }
    }

    Ok(true)
}

fn main() {
    if generate().is_err() {
        println!("Failed");
        exit(1);
    }
}
