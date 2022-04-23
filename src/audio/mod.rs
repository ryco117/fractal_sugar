use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SupportedStreamConfig};
use rustfft::{FftPlanner, num_complex::Complex};

use crate::my_math::Vector2;
use crate::space_filling_curves;

const PRINT_SPECTRUM: bool = true;

// Audio state to pass to UI thread
pub struct AudioState {
    pub big_boomer: (Vector2, f32),
    pub curl_attractors: [(Vector2, f32); 2],
    pub attractors: [(Vector2, f32); 2],
    pub volume: f32
}
impl Default for AudioState {
    fn default() -> Self {
        AudioState {
            big_boomer: (Vector2::new(0., 0.), 0.),
            curl_attractors: [(Vector2::new(0., 0.), 0.); 2],
            attractors: [(Vector2::new(0., 0.), 0.); 2],
            volume: 0.
        }
    }
}

// Simple type to help understand results from `analyze_frequency_range` closure
struct FrequencyAnalysis {
    pub loudest_frequency: f32,
    pub loudest_volume: f32,
    pub total_volume: f32
}

fn processing_thread_from_sample_rate(sample_rate: f32, tx: Sender<AudioState>, rx_acc: Receiver<Vec<Complex<f32>>>) {
    std::mem::drop(std::thread::spawn(move || {
        // Calculate some processing constants outside loop
        let size = if sample_rate > 48_000. { 4096 } else { 2048 }; // Use a fixed power-of-two for best performance
        let fsize = size as f32; // Size of the sample buffer as floating point
        let scale = 1. / fsize.sqrt(); // Rescale elements by 1/sqrt(n)
        let frequency_resolution = sample_rate / fsize; // Hertz per frequency bin after applying FFT

        // Helper function for converting frequency in Hertz to buffer index
        let frequency_to_index = |f: f32| -> usize { size.min((f / frequency_resolution).round() as usize) };

        // Store audio in a resizable array before processing, with some extra space to try to avoid heap allocations
        let mut audio_storage_buffer: Vec<Complex<f32>> = Vec::with_capacity(size + 1024);

        // Create factory and FFT once based on size
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(size);

        loop {
            // Append incoming audio data until we have sufficient samples
            while audio_storage_buffer.len() < size {
                let mut d = match rx_acc.recv() {
                    Ok(data) => data, // Update audio state vars
                    Err(e) => panic!("Failed to receive data from audio accumulator thread: {:?}", e)
                };
                audio_storage_buffer.append(&mut d)
            }
            let mut complex = &mut audio_storage_buffer[0..size];

            // Perform FFT on data in-place
            fft.process(&mut complex);

            // Create helper closure for determining the loudest frequency bin(s) within a frequency range
            let analyze_frequency_range = |start_frequency: f32, end_frequency: f32| -> FrequencyAnalysis {
                let start_index = frequency_to_index(start_frequency);
                let end_index = frequency_to_index(end_frequency);
                let mut total_volume: f32 = 0.;
                let mut max_volume: (usize, f32) = (start_index, 0.);
                for i in start_index..end_index {
                    let v = scale*complex[i].norm();
                    total_volume += v;

                    // Basics of determining largest frequency bins
                    if v > max_volume.1 {
                        max_volume = (i, v)
                    }
                }

                FrequencyAnalysis {
                    loudest_frequency: (max_volume.0 - start_index) as f32 / (end_index - start_index) as f32,
                    loudest_volume: max_volume.1,
                    total_volume
                }
            };

            // Analyze each frequency range
            let bass_analysis = analyze_frequency_range(30., 150.);
            let mids_analysis = analyze_frequency_range(150., 1_200.);
            let high_analysis = analyze_frequency_range(1_200., 12_000.);

            // Get total volume from all (relevant) frequencies
            let volume = bass_analysis.total_volume + mids_analysis.total_volume + high_analysis.total_volume;

            // Send updated state to UI thread
            match tx.send(AudioState {
                big_boomer: (space_filling_curves::square::default_curve_to_square(bass_analysis.loudest_frequency.powf(0.85)), bass_analysis.loudest_volume),
                curl_attractors: [(space_filling_curves::square::default_curve_to_square(mids_analysis.loudest_frequency.powf(0.625)), mids_analysis.loudest_volume), (Vector2::new(0., 0.), 0.)],
                attractors: [(space_filling_curves::square::default_curve_to_square(high_analysis.loudest_frequency.powf(0.2)), high_analysis.loudest_volume), (Vector2::new(0., 0.), 0.)],
                volume
            }) {
                Ok(()) => {}
                Err(_) => println!("UI thread receiver disconnected..")
            }

            if PRINT_SPECTRUM {
                // Scale to smaller array for displaying
                const DISPLAY_FFT_SIZE: usize = 64;
                let mut display_bins: [f32; DISPLAY_FFT_SIZE] = [0.; DISPLAY_FFT_SIZE];
                let display_start_index = frequency_to_index(30.);
                let display_end_index = frequency_to_index(16_000.);
                let r = (display_end_index - display_start_index) / DISPLAY_FFT_SIZE;
                let mut volume: f32 = 0.;
                let mut max_volume: (usize, f32) = (display_start_index, 0.);
                for i in 0..DISPLAY_FFT_SIZE { // Remove bounds as they are always over represented?
                    let mut t = 0.;
                    let index = display_start_index + i*r;
                    for j in 0..r {
                        let k = index + j;
                        let v = complex[k].norm();
                        t += v;

                        // Basics of determining largest frequency bins
                        if v > max_volume.1 {
                            max_volume = (k, v)
                        }
                    }

                    let v = scale*t;
                    display_bins[i] = v;
                    volume += v;
                }

                // Display simple audio spectrum
                let mut string_to_print = String::new();
                string_to_print = display_bins.into_iter().fold(string_to_print, |acc, x| {
                    acc +
                        if x > 3. {
                            "#"
                        } else if x > 1. {
                            "*"
                        } else if x > 0.2 {
                            "_"
                        } else {
                            " "
                        }
                });
                println!("{} Volume:{:>3.0} Freq:{:>5.0}Hz",
                    string_to_print,
                    volume,
                    max_volume.0 as f32 * frequency_resolution);
            }

            // TODO: Copy elements with index greater than `size` to the start of array since they weren't used in the previous FFT
            audio_storage_buffer.truncate(0)
        } // end unconditional loop
    }))
}

fn create_audio_loopback(default_audio_out: Device, audio_config: SupportedStreamConfig, tx_acc: Sender<Vec<Complex<f32>>>) -> cpal::Stream {
    // Store channel constants for use in callback
    let channel_count = audio_config.channels() as usize;
    let fchannel_count = channel_count as f32;

    // Create loopback stream for passing for processing
    match default_audio_out.build_input_stream(
        &audio_config.config(),
        move |data: &[f32], _| -> () {
            // Account for audio-channel packing of samples
            let size = data.len() / channel_count;

            // Short-circuit when there is no data
            if size == 0 { return }

            // Map data to mutable complex array.
            // This allows us to transfer ownership to processing thread and more easily use
            let complex: Vec<Complex<f32>> = {
                // Collect samples in groups equal in size to the audio-channel count, averaging over them
                (0..size).map(|i: usize| {
                    let k = channel_count*i;
                    let avg: f32 = data[k..k+channel_count].iter().fold(0., |acc, x| acc + x) / fchannel_count;
                    Complex::<f32>::new(avg, 0.) // Return new complex value with real part equal to the average amplitude across channels
                }).collect()
            };

            // Send new audio data to audio processing thread
            match tx_acc.send(complex) {
                Ok(()) => {}
                Err(_) => println!("Audio-processor receiver disconnected..")
            }
        },
        |e| panic!("Error on audio input stream: {:?}", e)
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

// Create new audio stream from the default audio-out device
pub fn create_default_loopback(tx: Sender<AudioState>) -> cpal::Stream {
    // Create CPAL default instance
    let audio_host = cpal::default_host();

    // Get the default audio out device
    let default_audio_out = audio_host.default_output_device().expect("There must be at least one output device");
    println!("Default audio out: {:?}", default_audio_out.name().unwrap_or(String::from("Unnamed device")));

    // Search device for a supported Float32 compatible format
    let audio_config = match default_audio_out.supported_output_configs().unwrap()
        .find(|c| c.sample_format() == SampleFormat::F32) {
        Some(config) => {
            println!("Default config from output device: {:?}", config);
            let sample_rate = config.min_sample_rate();
            config.with_sample_rate(sample_rate)
        }
        None => panic!("Could not find a supported audio format meeting our requirements")
    };

    // Store stream details we are intersted in
    let sample_rate = audio_config.sample_rate().0 as f32;

    // Create an accumulator channel to compose enough bytes for a reasonable FFT
    let (tx_acc, rx_acc) = mpsc::channel();
    let _ = processing_thread_from_sample_rate(sample_rate, tx, rx_acc);

    // Create and return loopback capture stream
    create_audio_loopback(default_audio_out, audio_config, tx_acc)
}