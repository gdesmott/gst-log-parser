use std::env;
use std::fs::File;

extern crate gst_log_parser;
use gst_log_parser::parse;

fn main() {
    let mut args = env::args();
    let path = args.nth(1).expect("Missing log file");
    let f = File::open(path).expect("Failed to open log file");

    let parsed = parse(f);
    for entry in parsed {
        println!("{}", entry);
    }
}
