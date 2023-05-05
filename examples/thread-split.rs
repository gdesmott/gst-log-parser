use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::Write,
    path::PathBuf,
};

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "thread-split",
    about = "Split a GStreamer log file to one file per thread"
)]
struct Opt {
    #[structopt(help = "Input log file")]
    input: PathBuf,
    #[structopt(long, help = "Directory where to store splitted files")]
    output_dir: Option<PathBuf>,
    #[structopt(
        long,
        default_value = "0",
        help = "Last lines of each thread to be displayed"
    )]
    tail: usize,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let input = File::open(opt.input)?;
    let parsed = gst_log_parser::parse(input);
    let mut threads = HashMap::new();
    let mut tails = HashMap::new();

    if let Some(output_dir) = opt.output_dir.as_ref() {
        std::fs::create_dir_all(output_dir)?;
    }

    for entry in parsed {
        if let Some(output_dir) = opt.output_dir.as_ref() {
            let output = threads
                .entry(entry.thread.clone())
                .or_insert_with_key(move |thd| {
                    let mut path = output_dir.clone();
                    path.push(format!("{thd}.log"));
                    File::create(path).unwrap()
                });

            writeln!(output, "{entry}")?;
        }

        if opt.tail > 0 {
            let tail = tails
                .entry(entry.thread.clone())
                .or_insert_with(VecDeque::new);

            tail.push_back(entry);
            if tail.len() > opt.tail {
                tail.pop_front();
            }
        }
    }

    for (thread, entries) in tails.into_iter() {
        println!("{thread}");
        for entry in entries {
            println!("{entry}");
        }
        println!();
    }

    Ok(())
}
