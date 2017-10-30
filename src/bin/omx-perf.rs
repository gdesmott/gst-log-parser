// Generate input logs with: GST_DEBUG="OMX_PERFORMANCE:8"
use std::env;
use std::fs::File;
use std::io::Write;
use std::process::exit;

extern crate gst_log_parser;
use gst_log_parser::parse;

fn usage () {
    println!("Usage: {} INPUT OUTPUT", env::args().nth(0).unwrap());
}

fn generate() -> Result<bool, std::io::Error> {
    let mut args = env::args();
    let in_path = args.nth(1).expect("Missing input");
    let input = File::open(in_path)?;
    let out_path = args.nth(0).expect("Missing output");
    let mut output = (File::create(&out_path))?;

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

    println!("Generated {}", out_path);
    Ok(true)
}

fn main() {
    if env::args().count() != 3 || generate().is_err() {
        usage();
        exit(1);
    }
}
