use std::sync::mpsc::Sender;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{InputCallbackInfo, SampleFormat, StreamInstant};
use rustfft::{FftPlanner, num_complex::Complex};

mod space_filling_curves;

// Audio state to pass to UI thread
pub struct AudioState {
    pub quaternion: [f32; 4],
    pub instant: StreamInstant
}

// Create new audio stream from the default audio-out device
pub fn create_default_loopback(tx: Sender<AudioState>) -> cpal::Stream {
    // Create CPAL default instance
    let audio_host = cpal::default_host();

    // Get the default audio out device
    let default_audio_out = audio_host.default_output_device().expect("There must be at least one output device");
    println!("Default audio out: {:?}", default_audio_out.name().unwrap_or(String::from("Unnamed device")));

    // Search deevice for a supported Float32 compatible format
    let audio_config = match default_audio_out.supported_output_configs().unwrap()
        .find(|c| c.sample_format() == SampleFormat::F32) {
        Some(config) => {
            println!("Default config from output device: {:?}", config);
            let sample_rate = config.min_sample_rate();
            config.with_sample_rate(sample_rate)
        }
        None => panic!("Could not find a supported audio format meeting our requirements")
    };
    let sample_rate = audio_config.sample_rate().0 as f32;

    // Create shared FFT factory for increased speed. However, exact buffer size is not known at this time.
    // This limits the possible speed-up because we cannot tell the planner the buffer size in advance
    let mut planner = FftPlanner::<f32>::new();

    match default_audio_out.build_input_stream(
        &audio_config.config(),
        move |data: &[f32], info: &InputCallbackInfo| {
            let size = data.len();
            if size > 0 {
                // Plan FFT based on size
                let fft = planner.plan_fft_forward(size);

                // Map data to mutable complex array
                let mut complex: Vec<Complex<f32>> = data.iter().map(|x| Complex::<f32>::new(*x, 0.)).collect();

                // Perform FFT on data in-place
                fft.process(&mut complex);

                let full_fsize = size as f32;
                let frequency_resolution = sample_rate / full_fsize;

                // FFT result is symmetric in magnituge and antisymmetric in phase about the center.
                // We can drop the latter half and retain all information
                let size = size / 2;
                complex.truncate(size);

                // Scale to smaller array for displaying
                const FFT_SIZE: usize = 64;
                let mut arr: [f32; FFT_SIZE] = [0.; FFT_SIZE];
                let r = size / FFT_SIZE;
                let scale = 1. / (size as f32).sqrt(); // Rescale elements by 1/sqrt(n), but also divide by range size to get average volume within range
                let mut _volume: f32 = 0.;
                let mut max_volume: (usize, f32) = (0, 0.);
                for i in 1..(FFT_SIZE-1) { // Remove bounds as they are always over represented?
                    let mut t = 0.;
                    let index = i*r;
                    for j in 0..r {
                        let v = complex[index + j].norm();
                        t += v;

                        // Basics of determining largest frequency bins
                        if v > max_volume.1 {
                            max_volume = (index + j, v)
                        }
                    }

                    let v = scale * t;
                    arr[i] = v;
                    _volume += v;
                }

                // Display simple audio spectrum
                let mut string_to_print =  String::new();
                string_to_print = arr.into_iter().fold(string_to_print, |acc, x| {
                    acc +
                        if x > 0.2 {
                            "#"
                        } else if x > 0.1 {
                            "*"
                        } else if x > 0.01 {
                            "_"
                        } else {
                            " "
                        }
                });
                let x = max_volume.0 as f32;
                println!("{} Freq: {:05}Hz: {:?}", string_to_print, x * frequency_resolution, space_filling_curves::default_curve_to_cube(x / full_fsize));

                match tx.send(AudioState {
                    quaternion: [0.; 4],
                    instant: info.timestamp().capture
                }) {
                    Ok(()) => {}
                    Err(_) => println!("Receiver disconnected..")
                }
            }
        },
        |e| panic!("Error on audio input stream: {:?}", e),
    ) {
        // Stream was created successfully 
        Ok(stream) => {
            // Ensure loopback capture starts
            stream.play().expect("Failed to initiate loopback stream");
            stream
        }

        // Panic application if thread cannot capture audio-out
        Err(e) => panic!("Error capturing audio stream: {:?}", e)
    }
}