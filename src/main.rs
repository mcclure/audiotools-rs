#![allow(unused_parens)]

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use minimp3::{Frame, Decoder};
use tuple_utils::Append;
use tuple_map::*;
use prisma::FromColor;

const DEBUG_UV:bool = false;

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
    #[arg(short = 's', long = "scale", help = "FFT scale", default_value_t=1.0)]
    fft_scale:f32,
    #[arg(short = 'p', long = "padding", help = "FFT padding count (in vframes)", default_value_t=0)]
    fft_padding: usize,
    #[arg(long = "window", help = "Window FFT? (hanning)")]
    fft_window: bool,
    #[arg(short = 'm', long = "max-power", help = "Color power scale [default frame-max]")]
    fft_max_pwr: Option<f64>,
    #[arg(long = "color-rotate", help = "Rotate FFT color hue [default 0]")]
    fft_color_rotate: Option<f32>,
    #[arg(long = "color-swap", help = "Flip red/green")]
    fft_color_swap: bool,
    #[arg(long = "color-45", help = "Diamond within square")]
    fft_color_45: bool,

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

    let fft_width = vframe_aframes * (1 + cli.fft_padding*2);
    let fft_out_width = fft_width/2;
    let mut fft_planner = realfft::RealFftPlanner::<f64>::new();
    let fft = fft_planner.plan_fft_forward(fft_width);

    let mut fft_in = fft.make_input_vec();
    let mut fft_out : [_;2] =  std::array::from_fn( |_| fft.make_output_vec() );

    let fft_window:Vec<f64> = if cli.fft_window { apodize::hanning_iter(fft_width).collect::<Vec<f64>>() } else { Default::default() };

    let center_square = pixel_width.min(pixel_height);
    let square_log = (center_square as f32).sqrt();

    let fullscreen = cli.fft_color_45; // Later may this may be settable independently

    let mut max_max_pwr = 0.0;

    for vframe_idx in 0..(if !DEBUG_UV { vframes } else { 1 }) {
        let frame_max:usize = pixel_width*pixel_height as usize*3;
        let mut frame: Vec<u8> = (0..frame_max).map(
            |_| 0 // Meaningless
        ).collect::<Vec<_>>();

            let basis = vframe_aframes*vframe_idx;

            // Copy "interesting" section of audio into fft_in

            let mut max_pwr = 0.0;

            for channel_idx in 0..2 {
                for fft_idx in 0..((1+2*cli.fft_padding as isize)*vframe_aframes as isize) {
                    // Remove basis + for a surprise
                    let aframe_idx = basis as isize + fft_idx-cli.fft_padding as isize*vframe_aframes as isize;
                    let sample_idx = aframe_idx*2 + channel_idx as isize;

                    fft_in[fft_idx as usize] = if sample_idx >= 0 && sample_idx < data.len() as isize {
                        data[sample_idx as usize] as f64 * if cli.fft_window { fft_window[fft_idx as usize] } else { 1.0 }
                    } else {
                        0.0
                    }
                }

                fft.process(&mut fft_in, &mut fft_out[channel_idx]).unwrap();

                for fft_idx in 0..fft_out_width {
                    let pwr = fft_out[channel_idx][fft_idx].norm_sqr();
                    //if !pwr.is_finite() { println!("Frame {vframe_idx} fft {fft_idx} = {}", fft_out[channel_idx][fft_idx]); }
                    if pwr > max_pwr { max_pwr = pwr; }
                }
            }

            if max_pwr < 0.0 { println!("Negative power at {vframe_idx}!"); }
            max_pwr = max_pwr.sqrt();
            if max_pwr > max_max_pwr { max_max_pwr = max_pwr }
            if !max_pwr.is_finite() { println!("Infinite power at {vframe_idx}!"); }
            if let Some(pwr) = cli.fft_max_pwr { max_pwr = pwr; }
            //println!("{vframe_idx} max power {max_pwr}");

            let read_frame_raw = |idx| {
                let basis = (basis + idx) * channels;
                if basis+1<data.len() {
                    return [data[basis], data[basis+1]];
                } else {
                    return [0.,0.];
                }
            };
            fn to8(f:f32) -> u8 { (f*127.0 + 127.0) as u8 }
            let torealclamp = |f:realfft::num_complex::Complex<f64>| -> f32 { (f.norm_sqr().log10()/max_pwr).min(1.0).max(0.0) as f32 };
            let _to8realclamp = |f:realfft::num_complex::Complex<f64>| -> u8 { (f.norm_sqr().log10()/max_pwr*255.0).min(255.0).max(0.0) as u8 };

            // FFT read
            for y in 0..(if fullscreen { pixel_height } else { center_square }) {
                let offset_y = (pixel_height as isize-center_square as isize)/2;
                let out_y:isize = y as isize + if fullscreen { 0 } else { offset_y };
                for x in 0..(if fullscreen { pixel_width } else { center_square }) {
                    let offset_x = (pixel_width as isize-center_square as isize)/2;
                    let out_x:isize = x as isize + if fullscreen { 0 } else { offset_x };

                    if out_x >= 0 && out_x < pixel_width as isize && out_y >= 0 && (out_y as isize) < pixel_height as isize {
                        let (out_x, out_y) = (out_x as usize, pixel_height - out_y as usize - 1);

                        let (x,y) = (x,y).map(|v| v as f32);

                        let (x,y) = if cli.fft_color_45 {
                            let half = (center_square as f32)/2.0;
                            //let (x, y) = (x + half, y + half); // Move center of square to 0,0
                            // X let (x,y) = (x*2.0, y*2.0);
                            // Formula: https://www.wolframalpha.com/input?i=rotate+%28360-45%29+degrees
                            let (x,y) = (x+y, y-x).map(|v| v ); // Rotate 45deg (WHY NO SCALE??? WHY ADDITIONAL OFFSET??)
                            let (x,y) = (x - half, y + half); // Move +x in old coordinate system
                            let (x,y) = (x - offset_x as f32, y + offset_x as f32); // Move +x in old coordinate system
                            // HOW TO DO OFFSET_Y ??
                            (x,y)
                        } else { (x,y) };

                        //let (x,y) = if fullscreen { (x + offset_x as f32, y + offset_y as f32) } else { (x,y) };

                        // Note x,y is modified here without "escaping"
                        let color = if !DEBUG_UV {
                            let (x,y) = (x,y).map(|v| ((v.log10().max(0.0)/square_log/cli.fft_scale*fft_out_width as f32) as usize).min(fft_out_width-1));

                            let mut color = [fft_out[0][x], fft_out[0][x]+fft_out[1][y], fft_out[1][y]].map(|p|torealclamp(p));
                            if cli.fft_color_swap {
                                color = [color[2], color[1], color[0]];
                            }
                            if let Some(rotate) = cli.fft_color_rotate {
                                color = {
                                    let color = prisma::Rgb::new(color[0], color[1], color[2]);
                                    let mut color = prisma::Hsv::<f32,angular_units::Deg<_>>::from_color(&color);
                                    color.set_hue( ( color.hue() + angular_units::Deg(rotate) ) % angular_units::Deg(360.0) );
                                    let color = prisma::Rgb::from_color(&color);
                                    [color.red(), color.green(), color.blue()]
                                }
                            }
                            let color = color.map(|x| (x*255.0).min(255.0).max(0.0) as u8);

                            color
                        } else {
                            let top = (center_square-1) as f32;
                            let (r,g) = (x*255.0/center_square as f32, y*255.0/center_square as f32).map(|v| v.min(255.0).max(0.0) as u8);
                            let color = [r,g, if x<=0.0 || y<=0.0 || x >= top || y >= top { 255 } else { 0 }];

                            color
                        };

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

    println!("Max pwr {max_max_pwr}");

    println!("Run:\n{} -r {} -i {}/%08d.png -i \"{}\" -pix_fmt yuv420p -vcodec libx264 -strict experimental -r {} -acodec copy output/test0.mp4", cli.ffmpeg, cli.framerate, cli.outdir.to_string_lossy(), cli.mp3.to_string_lossy(), cli.framerate);
}
