# gst-log-parser [![Build Status](https://travis-ci.org/gdesmott/gst-log-parser.svg?branch=master)](https://travis-ci.org/gdesmott/gst-log-parser)
Simple Rust library to parse GStreamer logs.

See [the examples](https://github.com/gdesmott/gst-log-parser/tree/master/examples) demonstrating how to use it.

## Quick start

- [Install Rust](https://www.rust-lang.org/en-US/install.html) if needed
- `cargo build --release`
- Parsing tools can be executed using `cargo run --release --example` and are also available in `target/release/examples/`

## Tools

`examples` contains a few log parsers. They can be used as examples demonstrating how to use this crate
but also should be useful when debugging specific issues.

### flow

This is a buffer flow analyzer consuming logs generated with `GST_DEBUG="GST_TRACER:7" GST_TRACERS=stats`.
It can be used to:
  - detect decreasing pts/dts
  - detect gap (long period of time without buffers being produced by a pad)
  - plot the pts/dts of produced buffers over time
