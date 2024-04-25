#![allow(unused_parens)]

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use minimp3::{Frame, Decoder};
use tuple_utils::Append;

#[derive(Parser)]
struct Cli {
    mp3: PathBuf,
    outdir: PathBuf,

    #[arg(short = 'r', long = "framerate", default_value_t=60)]
    framerate: usize,
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

    let aframes = data.len()/channels;
    println!("{} min {} sec .{} (msec)", aframes/sample_rate/60, (aframes/sample_rate)%60, (aframes%sample_rate)*1000/sample_rate);

    assert!(0==sample_rate%cli.framerate, "{} FPS doesn't divide into sample rate cleanly", cli.framerate);

    std::fs::create_dir_all(cli.outdir).expect("Could not create output dir");

    let vframe_aframes = sample_rate/cli.framerate;
    let vframes = data.len().div_ceil(channels*vframe_aframes);

    println!("{vframes} video frames, {vframe_aframes} aframes per vframe");
}
