use std::fs::File;

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
}

fn main() {
    let opt = Opt::from_args();
    let f = File::open(opt.input).expect("Failed to open log file");

    let parsed = parse(f);
    for entry in parsed {
        println!("{}", entry);
    }
}
