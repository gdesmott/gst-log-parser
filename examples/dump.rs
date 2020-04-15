// Copyright (C) 2017-2019 Guillaume Desmottes <guillaume@desmottes.be>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fs::File;

extern crate gst_log_parser;
use gst_log_parser::parse;

extern crate structopt;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "dump",
    about = "Parse a GStreamer log file and dump its content. Mostly used for testing"
)]
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
