#![allow(unused_parens)]

use clap::Parser;
use minimp3::{Decoder, Frame};
use std::fs::File;
use std::path::PathBuf;
use tuple_utils::Append;
use wave_art::WaveCharset;

use crate::wave_art::draw_waves;

mod wave_art;

#[derive(Parser)]
struct Cli {
    /// Path to MP3 to examine
    mp3: PathBuf,

    /// Character set to use for rendering
    #[arg(long, short, value_enum, default_value = "blocks")]
    charset: WaveCharset,

    /// Render waveform at double density (if applicable for the character set)
    #[arg(long)]
    hd: bool,

    /// Don't scale the waveform vertically so that it fits the terminal
    #[arg(long)]
    no_scale: bool,
}

fn main() {
    let cli = Cli::parse();

    // Decode all // FIXME: Streaming would be nice but then we can't get a priori length
    let (sample_rate, channels, data) = {
        let mut decoder = Decoder::new(File::open(cli.mp3).expect("Could not open file"));
        let mut sample_rate_channels: Option<(i32, usize)> = Default::default();
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
                        None => sample_rate_channels = Some((sample_rate, channels)),
                        Some(sample_rate_channels) => {
                            std::assert!(sample_rate_channels == (sample_rate, channels), "Sample rate or channels changed? At sample {}, was {sample_rate_channels:?}, now ({sample_rate}, {channels})", float_data.len());
                        }
                    }
                    for sample in data {
                        float_data.push(sample as f32 / std::i16::MAX as f32);
                    }
                }
                Err(minimp3::Error::Eof) => break,
                Err(e) => panic!("{:?}", e),
            }
        }

        sample_rate_channels
            .expect("No frames in mp3?")
            .append(float_data)
    };
    let sample_rate = sample_rate as usize; // Make life easy

    println!("{sample_rate}hz, {channels} channels");

    let frames = data.len() / channels;
    print!(
        "{} min {} sec .{} (msec)\n\n",
        frames / sample_rate / 60,
        (frames / sample_rate) % 60,
        (frames % sample_rate) * 1000 / sample_rate
    );

    let (term_width, term_height) = term_size::dimensions().expect("Unable to get term size");
    let chunk_size = data.len() / (term_width - 1) / if cli.hd { 2 } else { 1 };

    let height_relative = (term_height - 3) / 2; // minus two for metadata, minus one for padding, minus one for prompt, plus one for "omitted" row (see below)
    let mut mags = vec![];
    for chunk in data.chunks(chunk_size) {
        let mut magnitude = 0.0;
        for &sample in chunk {
            magnitude += sample * sample;
        }
        let adjusted_magnitude = (magnitude / (chunk.len() as f32)).sqrt();
        mags.push(adjusted_magnitude);
    }

    draw_waves(height_relative, &mags, !cli.no_scale, cli.charset, cli.hd);
}
