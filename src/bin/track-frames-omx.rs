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
            omx_ts: omx_ts,
            components: HashMap::new(),
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug)]
struct CbTime {
    fill_done_ts: ClockTime,
    fill_done_tot: ClockTime,
    fill_done_max: ClockTime,
    fill_done_n: u64,

    empty_done_ts: ClockTime,
    empty_done_tot: ClockTime,
    empty_done_max: ClockTime,
    empty_done_n: u64,
}

impl CbTime {
    fn new() -> CbTime {
        CbTime {
            fill_done_ts: ClockTime::none(),
            fill_done_tot: ClockTime::from_nseconds(0),
            fill_done_max: ClockTime::from_nseconds(0),
            fill_done_n: 0,

            empty_done_ts: ClockTime::none(),
            empty_done_tot: ClockTime::from_nseconds(0),
            empty_done_max: ClockTime::from_nseconds(0),
            empty_done_n: 0,
        }
    }

    fn fill_done_average(&self) -> ClockTime {
        ClockTime::from_nseconds(self.fill_done_tot.nseconds().unwrap() / self.fill_done_n)
    }

    fn empty_done_average(&self) -> ClockTime {
        ClockTime::from_nseconds(self.empty_done_tot.nseconds().unwrap() / self.empty_done_n)
    }
}

fn generate() -> Result<bool, std::io::Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;
    let parsed = parse(input).filter(|entry| entry.category == "OMX_PERFORMANCE");

    let mut frames: HashMap<u64, Frame> = HashMap::new();
    // comp -> CbTime
    let mut cbs: HashMap<String, CbTime> = HashMap::new();

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
            let cb = cbs.entry(comp_name.to_string()).or_insert(CbTime::new());

            match event {
                // input
                "EmptyThisBuffer" => {
                    comp.empty_ts.push(entry.ts);
                }
                // output
                "FillBufferDone" => {
                    // TODO: skip empty
                    comp.fill_done_ts.push(entry.ts);
                    cb.fill_done_ts = entry.ts;
                }
                "FillBufferDone-FINISHED" => {
                    let diff = entry.ts - cb.fill_done_ts;
                    cb.fill_done_tot += diff;
                    if cb.fill_done_max < diff {
                        cb.fill_done_max = diff;
                    }
                    cb.fill_done_n += 1;
                }
                "EmptyBufferDone" => cb.empty_done_ts = entry.ts,
                "EmptyBufferDone-FINISHED" => {
                    let diff = entry.ts - cb.empty_done_ts;
                    cb.empty_done_tot += diff;
                    if cb.empty_done_max < diff {
                        cb.empty_done_max = diff;
                    }
                    cb.empty_done_n += 1;
                }
                _ => {}
            }
        }
    }

    // Filter out frames still in OMX components
    let frames = frames.values().filter(|f| {
        for c in f.components.values() {
            if c.fill_done_ts.len() == 0 {
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
                .or_insert(ComponentStats::new());
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
                    " lst-in:  {} 𝚫: {}",
                    f.last_buffer_enter_ts(),
                    f.last_buffer_enter_ts() - f.first_buffer_enter_ts()
                );
            }

            if f.n_output() > 1 {
                print!(
                    " fst-out: {} 𝚫: {}",
                    f.first_buffer_left_ts(),
                    f.last_buffer_left_ts() - f.first_buffer_left_ts()
                );
            }

            print!("]");

            comp.tot_processing_time += diff;
            comp.n += 1;

            if comp.ts_first_out.is_none() {
                comp.ts_first_out = f.last_buffer_left_ts();
            }
            comp.ts_last_out = f.last_buffer_left_ts();
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

    println!("");
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
