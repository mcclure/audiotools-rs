#![allow(unused_parens)]

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use minimp3::{Frame, Decoder};
use tuple_utils::Append;

#[derive(Parser)]
#[clap(disable_help_flag = true)]
struct Cli {
    #[clap(long, action = clap::ArgAction::HelpLong)]
    help: Option<bool>,

    mp3: PathBuf,
    outdir: PathBuf,

    #[arg(short = 'w', long = "width", help = "Pixel width [default: 1920]")]
    pixel_width: Option<u32>,
    #[arg(short = 'h', long = "height", help = "Pixel height [default: 1080]")]
    pixel_height: Option<u32>,
    #[arg(short = 'r', long = "framerate", help = "Frames per second", default_value_t=60)]
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

    std::fs::create_dir_all(cli.outdir.clone()).expect("Could not create output dir");

    let vframe_aframes = sample_rate/cli.framerate;
    let vframes = data.len().div_ceil(channels*vframe_aframes);

    println!("{vframes} video frames, {vframe_aframes} aframes per vframe");

    let (pixel_width, pixel_height) = match ((cli.pixel_width, cli.pixel_height)) {
        (None, None) => (1920, 1080),
        (Some(w), None) => (w, w*9/16),
        (None, Some(h)) => (h*16/9, h),
        (Some(w), Some(h)) => (w, h)
    };

    let mut png_header = mtpng::Header::new();
    png_header.set_size(pixel_width, pixel_height).expect("Couldn't set png size");
    png_header.set_color(mtpng::ColorType::Truecolor, 8).expect("Couldn't set png color depth");
    let (pixel_width, pixel_height) = (pixel_width as usize, pixel_height as usize);

    assert!(channels == 2, "This script assumes stereo");

    for vframe_idx in 0..vframes {
        let max:usize = pixel_width*pixel_height as usize*3;
        let mut frame: Vec<u8> = (0..max).map(
            |_| 0 // Meaningless
        ).collect::<Vec<_>>();

            let basis = vframe_aframes*vframe_idx;
            let read = |idx| {
                let basis = (basis + idx) * channels;
                if basis+1<data.len() {
                    return [data[basis], data[basis+1]];
                } else {
                    return [0.,0.];
                }
            };
            for x in 0..vframe_aframes {
                let x = x + (pixel_width-vframe_aframes)/2;
                for y in 0..pixel_height {
                    let aframe = read(x+y);
                    let color = [(aframe[0]*127.0 + 127.0) as u8, (aframe[1]*127.0 + 127.0) as u8, ((aframe[0]*aframe[1]).sqrt()*255.0) as u8];
                    for comp_idx in 0..3 {
                        frame[(x + y*pixel_width)*3+comp_idx] = color[comp_idx];
                    }
                }
            }

        let png_writer = File::create(cli.outdir.clone().join(format!("{:08}.png", vframe_idx+1))).expect("Couldn't create file");

        let options = mtpng::encoder::Options::new();

        let mut encoder = mtpng::encoder::Encoder::new(png_writer, &options);

        encoder.write_header(&png_header).expect("Couldn't write header");
        encoder.write_image_rows(&frame).expect("Couldn't write png");
        encoder.finish().expect("Couldn't complete png");

        { // Status update
            let vframe_idx = vframe_idx + 1;
            if vframe_idx % 10 == 0 || vframe_idx == vframes {
                println!("{vframe_idx}/{vframes}");
            }
        }
    }
}
