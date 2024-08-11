#![allow(unused_parens)]

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use minimp3::{Frame, Decoder};
use tuple_utils::Append;
use hound;


#[derive(Parser)]
struct Cli {
    mp3: PathBuf,

    #[arg(short = 'a', long = "ascii", help = "Don't use unicode drawing")]
    ascii: bool,

    #[arg(short = 'x', long = "hex", help = "Hex integer rather than float")]
    hex: bool,

    #[arg(short = 'd', long = "decimal", help = "Decimal integer rather than float")]
    dec: bool,

    #[arg(short = 'o', long = "offset", help = "Start at this sample", default_value_t = 0)]
    offset: usize,

    #[arg(short = 'c', long = "count", help = "Samples to print (0 for all)", default_value_t = 0)]
    count: usize
}

fn main() {
    let cli = Cli::parse();

    let i2f = |sample| sample as f32/std::i16::MAX as f32;

    // Decode all // FIXME: Streaming would be nice but then we can't get a priori length
    let (sample_rate, channels, data) = {
        let mut decoder =
            Decoder::new(File::open(cli.mp3.clone()).expect("Could not open file"));
        let mut sample_rate_channels : Option<(i32, usize)> = Default::default();
        let mut all_data: Vec<i16> = Default::default();

        assert!(!(cli.hex && cli.dec), "Can't pass --dec and --hex at once.");

        loop {
            match decoder.next_frame() {
                Ok(Frame {
                    data,
                    sample_rate,
                    channels,
                    ..
                }) => {
                    match (sample_rate_channels) {
                        None => {
                            sample_rate_channels = Some((sample_rate, channels))
                        }
                        Some(sample_rate_channels) => {
                            std::assert!(sample_rate_channels == (sample_rate, channels), "Sample rate or channels changed? At sample {}, was {sample_rate_channels:?}, now ({sample_rate}, {channels})", all_data.len());
                        }
                    }
                    for sample in data {
                        all_data.push(sample);
                    }
                },
                Err(minimp3::Error::Eof) => break,
                Err(e) => panic!("{:?}", e),
            }
        }

        if let Some(tuple) = sample_rate_channels {
            tuple.append(all_data)
        } else {
            // Not an mp3?
            let mut reader = hound::WavReader::open(cli.mp3)
                .map_err(|_| ()).expect("File is not a readable mp3 or wav");
            let spec = reader.spec();
            let samples = reader.samples::<i16>();

            (spec.sample_rate as i32, spec.channels as usize,
                samples.map(|x| x.expect("Invalid sample in wav")).collect()
            )
        }
    };
    let sample_rate = sample_rate as usize; // Make life easy

    println!("{sample_rate}hz, {channels} channels per sample");

    let frames = data.len()/channels;
    println!("{} min {} sec .{} (msec)", frames/sample_rate/60, (frames/sample_rate)%60, (frames%sample_rate)*1000/sample_rate);

    print!("{} samples, left channel only:\n\n", frames);

    // Unicode, ASCII
    let blocks = [
        [' ', '▌', '█'],
        [' ', ' ', '#']
    ];


    let (term_width, term_height) = term_size::dimensions().expect("Unable to get term size");

    let count = if (cli.count > 0) { cli.count } else { frames };

    for c in 0..count {
        let idx = cli.offset + c;
        if idx > frames { break; }
        let idx = idx * channels;
        let sample = data[idx];
        print!("{c:>10}: ");
        let mut chars = 12;
        if cli.dec {
            print!("{sample:>6}");
            chars += 6;
        } else if cli.hex {
            print!("{sample:05x}");
            chars += 5;
        } else {
            print!("{:.06}", i2f(sample));
            chars += 8;
        }
        println!("");
    }
}
