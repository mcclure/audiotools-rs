#![allow(unused_parens)]

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use minimp3::{Frame, Decoder};
use tuple_utils::Append;
use tuple_map::*;

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
    #[arg(short = 'p', long = "padding", help = "FFT padding count (in vframes)", default_value_t=0)]
    padding: usize,
    #[arg(long = "window", help = "Window FFT? (hanning)")]
    window: bool,

    #[arg(long="ffmpeg", help="Path to ffmpeg (for concenience)", default_value="ffmpeg")]
    ffmpeg: String
}

fn main() {
    let cli = Cli::parse();

    // Decode all // FIXME: Streaming would be nice but then we can't get a priori length
    let (sample_rate, channels, data) = {
        let mut decoder =
            Decoder::new(File::open(cli.mp3.clone()).expect("Could not open file"));
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

    let fft_width = vframe_aframes * (1 + cli.padding*2);
    let fft_out_width = fft_width/2;
    let mut fft_planner = realfft::RealFftPlanner::<f64>::new();
    let fft = fft_planner.plan_fft_forward(fft_width);

    let mut fft_in = fft.make_input_vec();
    let mut fft_out : [_;2] =  std::array::from_fn( |_| fft.make_output_vec() );

    let fft_window:Vec<f64> = if cli.window { apodize::hanning_iter(fft_width).collect::<Vec<f64>>() } else { Default::default() };

    let center_square = pixel_width.min(pixel_height);
    let square_log = (center_square as f32).log10();

    for vframe_idx in 0..vframes {
        let max:usize = pixel_width*pixel_height as usize*3;
        let mut frame: Vec<u8> = (0..max).map(
            |_| 0 // Meaningless
        ).collect::<Vec<_>>();

            let basis = vframe_aframes*vframe_idx;

            // Copy "interesting" section of audio into fft_in

            let mut max_pwr = 0.0;

            for channel_idx in 0..2 {
                for fft_idx in 0..((1+cli.padding as isize)*vframe_aframes as isize) {
                    // Remove basis + for a surprise
                    let aframe_idx = basis as isize + fft_idx-cli.padding as isize*vframe_aframes as isize;
                    let sample_idx = aframe_idx*2 + channel_idx as isize;

                    fft_in[fft_idx as usize] = if sample_idx >= 0 && sample_idx < data.len() as isize {
                        data[sample_idx as usize] as f64 * if cli.window { fft_window[fft_idx as usize] } else { 1.0 }
                    } else {
                        0.0
                    }
                }

                fft.process(&mut fft_in, &mut fft_out[channel_idx]).unwrap();

                for fft_idx in 0..fft_out_width {
                    let pwr = fft_out[channel_idx][fft_idx].norm_sqr();
                    if !pwr.is_finite() { println!("Frame {vframe_idx} fft {fft_idx} = {}", fft_out[channel_idx][fft_idx]); }
                    if pwr > max_pwr { max_pwr = pwr; }
                }
            }

            if max_pwr < 0.0 { println!("Negative power at {vframe_idx}!"); }
            max_pwr = max_pwr.log10();
            if !max_pwr.is_finite() { println!("Infinite power at {vframe_idx}!"); }

            let read_frame_raw = |idx| {
                let basis = (basis + idx) * channels;
                if basis+1<data.len() {
                    return [data[basis], data[basis+1]];
                } else {
                    return [0.,0.];
                }
            };
            fn to8(f:f32) -> u8 { (f*127.0 + 127.0) as u8 }
            let to8realclamp = |f:realfft::num_complex::Complex<f64>| -> u8 { (f.norm_sqr().log10()/max_pwr*128.0 + 128.0).min(255.0).max(0.0) as u8 };

            // FFT read
            for y in 0..center_square {
                let out_y:isize = y as isize + (pixel_height as isize-center_square as isize)/2;
                for x in 0..center_square {
                    let out_x:isize = x as isize + (pixel_width as isize-center_square as isize)/2;

                    if out_x >= 0 && out_x < pixel_width as isize && out_y >= 0 && (out_y as isize) < pixel_height as isize {
                        let (out_x, out_y) = (out_x as usize, pixel_height - out_y as usize - 1); 
                        let (x,y) = (x,y).map(|v| (((v as f32).log10().max(0.0)/square_log*fft_out_width as f32) as usize).min(fft_width-1));

                        let color = [fft_out[0][x], fft_out[0][x]+fft_out[1][y], fft_out[1][y]].map(|p|to8realclamp(p));
                        let frame_basis = (out_x + out_y*pixel_width)*3;
                        for comp_idx in 0..3 {
                            frame[frame_basis+comp_idx] = color[comp_idx];
                        }
                    }
                }
            }

            // Raw/diagonal read
            if false {
                for x in 0..vframe_aframes {
                    let x = x + (pixel_width-vframe_aframes)/2;
                    for y in 0..pixel_height {
                        let aframe = read_frame_raw(x+y);
                        // Also consider: ((aframe[0]*aframe[1]).sqrt()*255.0) as u8
                        let color = [to8(aframe[0]), to8((aframe[0] + aframe[1])/2.0), to8(aframe[1])];
                        for comp_idx in 0..3 {
                            frame[(x + y*pixel_width)*3+comp_idx] = color[comp_idx];
                        }
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

    println!("Run:\n{} -r {} -i {}/%08d.png -i \"{}\" -pix_fmt yuv420p -vcodec libx264 -strict experimental -r {} -acodec copy output/test0.mp4", cli.ffmpeg, cli.framerate, cli.outdir.to_string_lossy(), cli.mp3.to_string_lossy(), cli.framerate);
}
