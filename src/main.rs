use rustfft::{FftPlanner, num_complex::Complex};

use lyon_path::iterator::*;
use lyon_path::math::{point, vector};
use lyon_path::geom::BezierSegment;
use lyon_path::{Path, PathEvent};
use lyon_svg::path_utils::build_path;

mod fft_drawer;
mod visualizer;

// Visualizer
use visualizer::Visualizer;
use visualizer::html_visualizer::HTMLVisualizer;

fn compute_path_length(path: &Path) -> f32 {
    // A simple std::iter::Iterator<PathEvent>,
    let simple_iter = path.iter();

    // Make it an iterator over simpler primitives flattened events,
    // which do not contain any curve. To do so we approximate each curve
    // linear segments according to a tolerance threshold which controls
    // the tradeoff between fidelity of the approximation and amount of
    // generated events. Let's use a tolerance threshold of 0.01.
    // The beauty of this approach is that the flattening happens lazily
    // while iterating without allocating memory for the path.
    let flattened_iter = path.iter().flattened(0.01);

    let mut total_length: f32 = 0.0;
    for evt in flattened_iter {
        match evt {
            PathEvent::Begin { at } => {}
            PathEvent::Line { from, to } => { total_length += (to - from).length(); }
            PathEvent::End { last, first, close } => {
                if close {
                    // Add the closed path
                    total_length += (first - last).length();
                }
            }
            _ => { panic!() }
        }
    }
    total_length
}

fn construct_sample_points(path: &Path, total_length: f32, n_sample: usize) -> Vec<Complex<f32>> {
    let mut samples = Vec::new();

    // A simple std::iter::Iterator<PathEvent>,
    let simple_iter = path.iter();

    // Make it an iterator over simpler primitives flattened events,
    // which do not contain any curve. To do so we approximate each curve
    // linear segments according to a tolerance threshold which controls
    // the tradeoff between fidelity of the approximation and amount of
    // generated events. Let's use a tolerance threshold of 0.01.
    // The beauty of this approach is that the flattening happens lazily
    // while iterating without allocating memory for the path.
    let flattened_iter = path.iter().flattened(0.01);

    let mut itered_length: f32 = 0.0;
    let mut itered_index: u32 = 0;
    let sample_length = total_length / (n_sample as f32);
    for evt in flattened_iter {
        match evt {
            PathEvent::Begin { at } => {
                // Add as the first one
                samples.push(Complex{ re: at.x, im: at.y });
                // println!("Add sample point {:?} at {:?} for begin", itered_index, at);
                itered_index += 1;
            }
            PathEvent::Line { from, to } => {
                let next_sample_length = sample_length * (itered_index as f32);
                let current_line_length = (to - from).length();
                let mut last_added_sample_on_this_segment: f32 = 0.0;
                if (itered_length < next_sample_length) {
                    if itered_length + current_line_length >= next_sample_length {
                        last_added_sample_on_this_segment = sample_length
                            - (itered_length - sample_length * ((itered_index - 1) as f32));
                        // Add a sample point on the segment
                        let sample = from + (to - from) * 
                            ((last_added_sample_on_this_segment) / current_line_length);
                        samples.push(Complex{ re: sample.x, im: sample.y });
                        // println!("Add sample point {:?} at {:?}", itered_index, sample);
                        // Ready to find the next sample point
                        itered_index += 1;
                    }
                }
                // println!("last_added_sample_on_this_segment {:?}", last_added_sample_on_this_segment);

                // Compensation
                let mut compensation_counter = 0;
                while sample_length * (itered_index as f32) <= itered_length + current_line_length {
                    // Add a sample point for compensation
                    let sample = from + (to -from) * (sample_length * compensation_counter as f32) / current_line_length +
                        (to - from) * (last_added_sample_on_this_segment + sample_length) / current_line_length;
                    samples.push(Complex{ re: sample.x, im: sample.y });
                    // println!("Add sample point {:?} at {:?} for compensation", itered_index, sample);
                    // Ready to find the next sample point
                    itered_index += 1;
                    compensation_counter += 1;
                }

                // Accumulate the iterated length
                itered_length += current_line_length;
            }
            PathEvent::End { last, first, close } => {
                if close {
                    // Alias them
                    let from = last;
                    let to = first;

                    let next_sample_length = sample_length * (itered_index as f32);
                    let current_line_length = (to - from).length();
                    let mut last_added_sample_on_this_segment: f32 = 0.0;
                    if (itered_length < next_sample_length) {
                        if itered_length + current_line_length >= next_sample_length {
                            last_added_sample_on_this_segment = sample_length
                                - (itered_length - sample_length * ((itered_index - 1) as f32));
                            // Add a sample point on the segment
                            let sample = from + (to - from) * 
                                ((last_added_sample_on_this_segment) / current_line_length);
                            samples.push(Complex{ re: sample.x, im: sample.y });
                            // println!("Add sample point {:?} at {:?}", itered_index, sample);
                            // Ready to find the next sample point
                            itered_index += 1;
                        }
                    }
                    // println!("last_added_sample_on_this_segment {:?}", last_added_sample_on_this_segment);

                    // Compensation
                    let mut compensation_counter = 0;
                    while sample_length * (itered_index as f32) < itered_length + current_line_length {
                        // Add a sample point for compensation
                        let sample = from + (to -from) * (sample_length * compensation_counter as f32) / current_line_length +
                            (to - from) * (last_added_sample_on_this_segment + sample_length) / current_line_length;
                        samples.push(Complex{ re: sample.x, im: sample.y });
                        // println!("Add sample point {:?} at {:?} for compensation", itered_index, sample);
                        // Ready to find the next sample point
                        itered_index += 1;
                        compensation_counter += 1;
                    }
                }
            }
            _ => { panic!() }
        }
    }
    samples
}

fn path_to_fft(path: Path, n_sample: usize) -> Vec<Complex<f32>> {
    let path_length = compute_path_length(&path);
    let mut samples = construct_sample_points(&path, path_length, n_sample);

    while samples.len() > n_sample {
        samples.remove(n_sample);
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_sample);

    fft.process(&mut samples);

    for i in 0..samples.len() {
        samples[i] = samples[i] / samples.len() as f32;
    }
    samples
}

fn build_path_from_svg(svg_commands: &str) -> Path {
    let svg_builder = Path::builder().with_svg();
    match build_path(svg_builder, svg_commands) {
        Ok (path) => {
            return path;
        }
        _ => {
            panic!();
        }
    }
}

use clap::{Arg, App, SubCommand};

fn main() {
    // Add param
    let app = App::new("Fourier SVG Drawer")
        .version("1.0.0")
        .author("Inoki <veyx.shaw@gmail.com>")
        .about("Draw a path in SVG format using Fourier Transform")
        .arg(Arg::with_name("SVG Path")
            .short("p")
            .long("path")
            .help("Draw an SVG path in string")
            .takes_value(true))
        .arg(Arg::with_name("SVG file")
            .short("f")
            .long("file")
            .help("Draw the first SVG path in file")
            .takes_value(true))
        .arg(Arg::with_name("Number of sample points")
            .short("s")
            .long("sample")
            .help("Use how many sample points to draw the path")
            .takes_value(true))
        .arg(Arg::with_name("Number of waves")
            .short("w")
            .long("wave")
            .help("Use how many waves to draw the path")
            .takes_value(true));
    let matches = app.get_matches();

    // SVG source args
    let arg_path = matches.value_of("SVG Path").unwrap_or("");
    let arg_svg_file = matches.value_of("SVG file").unwrap_or("");

    // FFT config args
    let arg_sample = matches.value_of("Number of sample points").unwrap_or("10240");
    let arg_wave = matches.value_of("Number of waves").unwrap_or("201");

    // Retrieve svg from web or local file
    let mut svg_string: &str;
    if arg_svg_file.len() > 0 {
        // TODO: Read path from svg file
        return;
    } else if (arg_path.len() > 0) {
        // Read path from svg path string
        svg_string = arg_path;
    } else {
        println!("No SVG path provided.");
        return;
    }

    let num_sample = arg_wave.parse::<usize>().unwrap_or(10240);
    let mut num_wave = arg_wave.parse::<usize>().unwrap_or(201);

    // Make sure num_sample >= num_wave
    if num_sample < num_wave {
        num_wave = num_sample;
    }

    let path = build_path_from_svg(svg_string);

    let fft_size = num_sample;
    let mut fft_result = path_to_fft(path, fft_size);

    // Temporally output to json
    let mut data = Vec::new();
    data.push(fft_drawer::DrawData::new_from_complex(0 as f32, fft_result[0]));
    // Can change from param
    for i in 1..(num_wave / 2) {
        data.push(fft_drawer::DrawData::new_from_complex(i as f32, fft_result[i]));
        data.push(fft_drawer::DrawData::new_from_complex((0 - i as i32) as f32, fft_result[fft_size - i]));
    }

    // TODO: Add an option to choose a different visualizer
    let html_visualizer = HTMLVisualizer::new("output.html".to_string());
    html_visualizer.render(data);
}
