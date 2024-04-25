#![allow(unused_parens)]

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use minimp3::{Frame, Decoder};
use tuple_utils::Append;

#[derive(Parser)]
struct Cli {
    mp3: PathBuf,
    //output: PathBuf,
}

fn main() {
    let cli = Cli::parse();

    // Decode all // FIXME: Streaming would be nice but then we can't get a priori length
    let (sample_rate, channels, data) = {
        let mut decoder =
            Decoder::new(File::open(cli.mp3).expect("Could not open file"));
        let mut sample_rate_channels : Option<(i32, usize)> = Default::default();
        let mut float_data: Vec<f32> = Default::default();

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
                            std::assert!(sample_rate_channels == (sample_rate, channels), "Sample rate or channels changed? At sample {}, was {sample_rate_channels:?}, now ({sample_rate}, {channels})", float_data.len());
                        }
                    }
                    for sample in data {
                        float_data.push(sample as f32/std::i16::MAX as f32);
                    }
                },
                Err(minimp3::Error::Eof) => break,
                Err(e) => panic!("{:?}", e),
            }
        }

        sample_rate_channels.expect("No frames in mp3?").append(float_data)
    };
    let sample_rate = sample_rate as usize; // Make life easy

    println!("{sample_rate}hz, {channels} channels");

    let frames = data.len()/channels;
    print!("{} min {} sec .{} (msec)\n\n", frames/sample_rate/60, (frames/sample_rate)%60, (frames%sample_rate)*1000/sample_rate);

    let (term_width, term_height) = term_size::dimensions().expect("Unable to get term size");
    let pixel_frames = frames / term_width;
    let pixel_count = frames / pixel_frames;
    let height_relative = (term_height-4)/2; 
    let mut heights:Vec<usize> = Default::default();
    for pixel in 0..pixel_count {
        let mut magnitude = 0.0;
        let offset = pixel*pixel_frames*channels;
        let samples = channels*if (pixel < pixel_count-1) { pixel_frames } else { pixel_frames + frames % pixel_frames };
        for idx in offset..(offset+samples) {
            let sample = data[idx as usize];
            magnitude += sample*sample;
        }
        let height = (magnitude/samples as f32).sqrt()*height_relative as f32;
        let height_floorplus:usize = height.floor() as usize + 1;
        heights.push(if height == 0.0 { 0 } else if height_floorplus > height_relative { height_relative } else { height_floorplus });
    }

    let printhalf = |up| {
        for line_idx in 0..height_relative {
            let line_idx = if up { line_idx + 1 } else { height_relative - line_idx };
            for height in &heights {
                print!("{}", if *height >= line_idx { '#' } else { ' ' } );
            }
            println!("");
        }
    };
    printhalf(false);
    printhalf(true);
}
