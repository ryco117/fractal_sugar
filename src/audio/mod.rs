use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::SystemTime;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SupportedStreamConfig};
use rustfft::{num_complex::Complex, FftPlanner};

use crate::my_math::{Vector2, Vector3, Vector4};
use crate::space_filling_curves;

const PRINT_SPECTRUM: bool = true;

const EMPTY_NOTE: Vector4 = Vector4::new(0., 0., 0., 0.);

// Set some constants for scaling frequencies to sound/appear more linear
const BASS_POW: f32 = 0.85;
const MIDS_POW: f32 = 0.75;
const HIGH_POW: f32 = 0.45;

const BASS_KICK: f32 = 0.05;
const PREVIOUS_BASS_COUNT: usize = 10;

// Audio state to pass to UI thread
pub struct AudioState {
    pub volume: f32,

    // 2D (Particles)
    pub big_boomer: Vector4,
    pub curl_attractors: [Vector4; 2],
    pub attractors: [Vector4; 2],

    // 3D (Fractals)
    pub kick_angular_velocity: Option<Vector4>,
    pub reactive_bass: Vector3,
    pub reactive_mids: Vector3,
    pub reactive_high: Vector3,
}
impl Default for AudioState {
    fn default() -> Self {
        AudioState {
            volume: 0.,

            big_boomer: EMPTY_NOTE,
            curl_attractors: [EMPTY_NOTE; 2],
            attractors: [EMPTY_NOTE; 2],

            kick_angular_velocity: None,
            reactive_bass: Vector3::default(),
            reactive_mids: Vector3::default(),
            reactive_high: Vector3::default(),
        }
    }
}

// Simple type to help understand results from `analyze_frequency_range` closure
struct FrequencyAnalysis {
    pub loudest: Vec<(f32, f32)>,
    pub total_volume: f32,
}

// Convert note analysis to 2D vectors with strengths
fn map_note_to_square(note: (f32, f32), pow: f32) -> Vector4 {
    let Vector2 { x, y } =
        0.95 * space_filling_curves::square::curve_to_square_n(note.0.powf(pow), 5);
    Vector4::new(x, y, 0., note.1)
}

// Convert note analysis to 3D vectors
fn map_freq_to_cube(freq: f32, pow: f32) -> Vector3 {
    space_filling_curves::cube::curve_to_cube_n(freq.powf(pow), 6)
}

fn processing_thread_from_sample_rate(
    sample_rate: f32,
    tx: Sender<AudioState>,
    rx_acc: Receiver<Vec<Complex<f32>>>,
) {
    std::thread::spawn(move || {
        // Calculate some processing constants outside loop
        let size = if sample_rate > 48_000. { 4096 } else { 2048 }; // Use a fixed power-of-two for best performance
        let fsize = size as f32; // Size of the sample buffer as floating point
        let scale = 1. / fsize.sqrt(); // Rescale elements by 1/sqrt(n)
        let frequency_resolution = sample_rate / fsize; // Hertz per frequency bin after applying FFT

        // Helper function for converting frequency in Hertz to buffer index
        let frequency_to_index =
            |f: f32| -> usize { size.min((f / frequency_resolution).round() as usize) };

        // Store audio in a resizable array before processing, with some extra space to try to avoid heap allocations
        let mut audio_storage_buffer: Vec<Complex<f32>> = Vec::with_capacity(size + 1024);

        // Create factory and FFT once based on size
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(size);

        // Keep track of state that we don't want UI to need to calculate
        let mut kick_angular_velocity = None;
        let mut last_kick = SystemTime::now();
        let mut previous_bass_index = 0;
        let mut previous_bass: [Option<Vec<f32>>; PREVIOUS_BASS_COUNT] = Default::default();

        loop {
            // Append incoming audio data until we have sufficient samples
            while audio_storage_buffer.len() < size {
                let mut d = match rx_acc.recv() {
                    Ok(data) => data, // Update audio state vars
                    Err(e) => panic!(
                        "Failed to receive data from audio accumulator thread: {:?}",
                        e
                    ),
                };
                audio_storage_buffer.append(&mut d);
            }
            let complex = &mut audio_storage_buffer[0..size];

            // Perform FFT on data in-place
            fft.process(complex);

            // Create helper closure for determining the loudest frequency bin(s) within a frequency range
            let analyze_frequency_range = |frequency_range: std::ops::Range<f32>,
                                           count: usize,
                                           mut delta: f32,
                                           min_volume: f32,
                                           vol_freq_scale: f32|
             -> FrequencyAnalysis {
                let start_index = frequency_to_index(frequency_range.start);
                let end_index = frequency_to_index(frequency_range.end);
                let len = end_index - start_index;
                let flen = len as f32;
                delta /= 2.; // Allow caller to specify total width, even though we use distance from center

                // Create sorted array of notes in this frequency range
                let mut total_volume = 0.;
                let mut sorted: Vec<(f32, f32)> = (0..len)
                    .map(|i| {
                        let frac = i as f32 / flen;
                        let v = scale * complex[start_index + i].norm();
                        total_volume += v;
                        (frac, f32::powf(vol_freq_scale, frac) * v)
                    })
                    .collect();

                let mut loudest: Vec<(f32, f32)> = Vec::with_capacity(count);
                while !sorted.is_empty() && loudest.len() < count {
                    sorted.sort_unstable_by(|x, y| {
                        y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let (t, v) = sorted[0];
                    let remaining: Vec<(f32, f32)> = sorted
                        .into_iter()
                        .filter(|x| (t - x.0).abs() > delta)
                        .collect();

                    // Update the strongest and the remaining lists. Reject values too quiet
                    loudest.push((t, if v >= min_volume { v } else { 0. }));
                    sorted = remaining;
                }
                assert_eq!(
                    count,
                    loudest.len(),
                    "Calling code assumes requested number of notes will be returned"
                );

                FrequencyAnalysis {
                    loudest,
                    total_volume,
                }
            };

            // Analyze each frequency ranges
            let (bass_analysis, current_bass) = {
                let frequency_range: std::ops::Range<f32> = 30.0..275.;
                let delta: f32 = 1.;
                let min_volume: f32 = 0.25;
                let vol_freq_scale = 1.8;
                let analysis = analyze_frequency_range(
                    frequency_range.clone(),
                    1,
                    delta,
                    min_volume,
                    vol_freq_scale,
                );

                // Do extra analysis for
                let current_bass: Vec<f32> = {
                    let start_index = frequency_to_index(frequency_range.start);
                    let end_index = frequency_to_index(frequency_range.end);
                    let len = end_index - start_index;
                    let flen = len as f32;

                    (0..len)
                        .map(|i| {
                            let frac = i as f32 / flen;
                            let v = scale * complex[start_index + i].norm();
                            f32::powf(vol_freq_scale, frac) * v
                        })
                        .collect()
                };

                (analysis, current_bass)
            };
            let mids_analysis = {
                let frequency_range: std::ops::Range<f32> = 275.0..1_600.;
                let delta: f32 = 0.1;
                let min_volume: f32 = 0.05;
                let scale = 2.75;
                analyze_frequency_range(frequency_range, 2, delta, min_volume, scale)
            };
            let high_analysis = {
                let frequency_range: std::ops::Range<f32> = 1_600.0..16_000.;
                let delta: f32 = 0.1;
                let min_volume: f32 = 0.01;
                let scale = 4.;
                analyze_frequency_range(frequency_range, 2, delta, min_volume, scale)
            };

            // Get total volume from all (relevant) frequencies
            let volume = bass_analysis.total_volume
                + mids_analysis.total_volume
                + high_analysis.total_volume;

            // Use analysis of bass notes to determine if a kick should occur
            let kick_elapsed = match last_kick.elapsed() {
                Ok(d) => d.as_secs_f32(),
                _ => 0.,
            };
            let avg_prev_bass: f32 = previous_bass.iter().fold(0., |acc, x| {
                acc + match x {
                    Some(v) => {
                        let l = v.len();
                        if l > 0 {
                            v[(((l as f32) * bass_analysis.loudest[0].0).round() as usize)
                                .min(l - 1)]
                        } else {
                            0.
                        }
                    }
                    None => 0.,
                }
            }) / (previous_bass.len() as f32);
            if (bass_analysis.loudest[0].1 > 4. || bass_analysis.loudest[0].1 * kick_elapsed > 8.)
                && kick_elapsed > 0.8
                && bass_analysis.loudest[0].1 > 1.25
                && bass_analysis.loudest[0].1 > 3. * avg_prev_bass
            {
                let v = space_filling_curves::cube::curve_to_cube_n(
                    bass_analysis.loudest[0].0.powf(BASS_POW),
                    6,
                );
                kick_angular_velocity = Some(Vector4::new(
                    v.x,
                    v.y,
                    v.z,
                    BASS_KICK * bass_analysis.total_volume.sqrt(),
                ));
                last_kick = SystemTime::now();
            }
            // Regardless, update bass history
            previous_bass[previous_bass_index] = Some(current_bass);
            previous_bass_index = (previous_bass_index + 1) % previous_bass.len();

            // Send updated state to UI thread
            match tx.send(AudioState {
                volume,

                big_boomer: map_note_to_square(bass_analysis.loudest[0], BASS_POW),
                curl_attractors: [
                    map_note_to_square(mids_analysis.loudest[0], MIDS_POW),
                    map_note_to_square(mids_analysis.loudest[1], MIDS_POW),
                ],
                attractors: [
                    map_note_to_square(high_analysis.loudest[0], HIGH_POW),
                    map_note_to_square(high_analysis.loudest[1], HIGH_POW),
                ],

                kick_angular_velocity: kick_angular_velocity.take(),
                reactive_bass: map_freq_to_cube(bass_analysis.loudest[0].0, BASS_POW),
                reactive_mids: map_freq_to_cube(mids_analysis.loudest[0].0, MIDS_POW),
                reactive_high: map_freq_to_cube(high_analysis.loudest[0].0, HIGH_POW),
            }) {
                Ok(()) => {}
                Err(_) => println!("UI thread receiver disconnected.."),
            }

            // Optionally print frequency-spectrum to console
            if PRINT_SPECTRUM {
                const DISPLAY_FFT_SIZE: usize = 64;
                let mut display_bins: [f32; DISPLAY_FFT_SIZE] = [0.; DISPLAY_FFT_SIZE];
                let display_start_index = frequency_to_index(30.);
                let display_end_index = frequency_to_index(12_000.);
                let r = (display_end_index - display_start_index) / DISPLAY_FFT_SIZE;
                let mut volume: f32 = 0.;
                let mut max_volume: (usize, f32) = (display_start_index, 0.);
                for (i, display_bin) in display_bins.iter_mut().enumerate() {
                    let mut t = 0.;
                    let index = display_start_index + i * r;
                    for j in 0..r {
                        let k = index + j;
                        let v = complex[k].norm();
                        t += v;

                        // Basics of determining largest frequency bins
                        if v > max_volume.1 {
                            max_volume = (k, v);
                        }
                    }

                    let v = scale * t;
                    *display_bin = v;
                    volume += v;
                }

                // Display simple audio spectrum
                let mut string_to_print = String::new();
                string_to_print = display_bins.into_iter().fold(string_to_print, |acc, x| {
                    acc + if x > 3. {
                        "#"
                    } else if x > 1. {
                        "*"
                    } else if x > 0.2 {
                        "_"
                    } else {
                        " "
                    }
                });
                println!(
                    "{} Volume:{:>3.0} Freq:{:>5.0}Hz",
                    string_to_print,
                    volume,
                    max_volume.0 as f32 * frequency_resolution
                );
            }

            // Copy elements with index >= `size` to the start of array since they haven't been used yet
            audio_storage_buffer.copy_within(size.., 0);
            audio_storage_buffer.truncate(audio_storage_buffer.len() - size);
        } // end unconditional loop
    });
}

fn create_audio_loopback(
    default_audio_out: &Device,
    audio_config: &SupportedStreamConfig,
    tx_acc: Sender<Vec<Complex<f32>>>,
) -> cpal::Stream {
    // Store channel constants for use in callback
    let channel_count = audio_config.channels() as usize;
    let fchannel_count = channel_count as f32;

    // Create loopback stream for passing for processing
    match default_audio_out.build_input_stream(
        &audio_config.config(),
        move |data: &[f32], _| {
            // Account for audio-channel packing of samples
            let size = data.len() / channel_count;

            // Short-circuit when there is no data
            if size == 0 {
                return;
            }

            // Map data to mutable complex array.
            // This allows us to transfer ownership to processing thread and more easily use
            let complex: Vec<Complex<f32>> = {
                // Collect samples in groups equal in size to the audio-channel count, averaging over them
                (0..size)
                    .map(|i: usize| {
                        let k = channel_count * i;
                        let avg: f32 = data[k..k + channel_count].iter().fold(0., |acc, x| acc + x)
                            / fchannel_count;
                        Complex::<f32>::new(avg, 0.) // Return new complex value with real part equal to the average amplitude across channels
                    })
                    .collect()
            };

            // Send new audio data to audio processing thread
            match tx_acc.send(complex) {
                Ok(()) => {}
                Err(_) => println!("Audio-processor receiver disconnected.."),
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
        Err(e) => panic!("Error capturing audio stream: {:?}", e),
    }
}

// Create new audio stream from the default audio-out device
pub fn create_default_loopback(tx: Sender<AudioState>) -> cpal::Stream {
    // Create CPAL default instance
    let audio_host = cpal::default_host();

    // Get the default audio out device
    let default_audio_out = audio_host
        .default_output_device()
        .expect("There must be at least one output device");
    println!(
        "Default audio out: {:?}",
        default_audio_out
            .name()
            .unwrap_or_else(|_| String::from("Unnamed device"))
    );

    // Search device for a supported Float32 compatible format
    let audio_config = match default_audio_out
        .supported_output_configs()
        .unwrap()
        .find(|c| c.sample_format() == SampleFormat::F32)
    {
        Some(config) => {
            println!("Default config from output device: {:?}", config);
            let sample_rate = config.min_sample_rate();
            config.with_sample_rate(sample_rate)
        }
        None => panic!("Could not find a supported audio format meeting our requirements"),
    };

    // Store stream details we are intersted in
    let sample_rate = audio_config.sample_rate().0 as f32;

    // Create an accumulator channel to compose enough bytes for a reasonable FFT
    let (tx_acc, rx_acc) = mpsc::channel();
    processing_thread_from_sample_rate(sample_rate, tx, rx_acc);

    // Create and return loopback capture stream
    create_audio_loopback(&default_audio_out, &audio_config, tx_acc)
}
