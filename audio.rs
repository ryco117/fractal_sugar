use std::sync::mpsc::Sender;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use rustfft::{FftPlanner, num_complex::Complex};

// Audio state to pass to UI thread
pub struct AudioState {
    pub quaternion: [f32; 4]
}

// Create new audio stream from the default audio-out device
pub fn create_default_loopback(tx: Sender<AudioState>) -> cpal::Stream {
    // Create CPAL default instance
    let audio_host = cpal::default_host();

    // Get the default audio out device
    let default_audio_out = audio_host.default_output_device().expect("There must be at least one output device");
    println!("Default audio out: {:?}", default_audio_out.name().unwrap_or(String::from("Unnamed device")));

    // Search deevice for a supported Float32 compatible format
    let audio_config = match default_audio_out.supported_output_configs().unwrap().find(|c| c.sample_format() == SampleFormat::F32) {
        Some(config) => {
            println!("Default config from output device: {:?}", config);
            let sample_rate = config.min_sample_rate();
            config.with_sample_rate(sample_rate)
        }
        None => panic!("Could not find a supported audio format meeting our requirements")
    };

    // Create shared FFT factory for increased speed. However, exact buffer size is not known at this time.
    // This limits the possible speed-up because we cannot tell the planner the buffer size in advance
    let mut planner = FftPlanner::<f32>::new();

    match default_audio_out.build_input_stream(
        &audio_config.config(),
        move |data: &[f32], _: &_| {
            let size = data.len();
            if size > 0 {
                // Plan FFT based on size
                let fft = planner.plan_fft_forward(size);

                // Map data to mutable complex array
                let mut complex: Vec<Complex<f32>> = data.iter().map(|x| Complex::<f32>::new(*x, 0.)).collect();

                // Perform FFT on data in-place
                fft.process(&mut complex);

                // FFT result is symmetric in magnituge and antisymmetric in phase about the center.
                // We can drop the latter half and retain all information
                let size = size / 2;
                complex.truncate(size);

                // Scale to smaller array for displaying
                const FFT_SIZE: usize = 128;
                let mut arr: [f32; FFT_SIZE] = [0.; FFT_SIZE];
                let r = size / FFT_SIZE;
                let scale = 1. / ((size as f32).sqrt() * r as f32); // Rescale elements by 1/sqrt(n), but also divide by range size to get average volume within range
                for i in 0..FFT_SIZE {
                    let mut t = 0.;
                    let index = i*r;
                    for j in 0..r {
                        t += complex[index + j].norm()
                    }

                    arr[i] = scale * t
                }

                // Display spectrum
                for &x in &arr[1..] {
                    if x > 0.2 {
                        print!("#")
                    } else if x > 0.02 {
                        print!("_")
                    } else {
                        print!(" ")
                    }
                }
                print!("\n");

                match tx.send(AudioState {quaternion: [0.; 4]}) {
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