// Generate input logs with: GST_DEBUG="OMX_PERFORMANCE:8"
use std::fs::File;
use std::io::Write;
use std::process::exit;

extern crate gst_log_parser;
use gst_log_parser::parse;

extern crate structopt;
#[macro_use]
extern crate structopt_derive;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "dump", about = "Parse a GStreamer log file and dump its content. Mostly used for testing")]
struct Opt {
    #[structopt(help = "Input file")]
    input: String,
    #[structopt(help = "Output file")]
    output: String,
}


fn generate() -> Result<bool, std::io::Error> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;
    let mut output = (File::create(&opt.output))?;

    let parsed = parse(input).filter(|entry| {
        entry.category == "OMX_PERFORMANCE"
    });

    for entry in parsed {
        let object = entry.object.unwrap();
        // Extract the component name by taking the 4th last chars of the gst object name
        if let Some((i, _)) = object.char_indices().rev().nth(3) {
            let comp_name = &object[i..];
            write!(output, "{}_{} 1 {}\n", comp_name, entry.message, entry.ts)?;
            write!(output, "{}_{} 0 {}\n", comp_name, entry.message, entry.ts + 1)?;
        }
    }

    println!("Generated {}", opt.output);
    Ok(true)
}

fn main() {
    if generate().is_err() {
        exit(1);
    }
}
