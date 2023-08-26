/*
    fractal_sugar - An experimental audio visualizer combining fractals and particle simulations.
    Copyright (C) 2022  Ryan Andersen

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use std::time::SystemTime;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SupportedStreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use rustfft::{num_complex::Complex, FftPlanner};

use crate::my_math::{Vector2, Vector3, Vector4};
use crate::space_filling_curves;
use crate::space_filling_curves::{cube::curve_to_cube_n, square::curve_to_square_n};

const PRINT_SPECTRUM: bool = true;

// Set some constants for scaling frequencies to sound/appear more linear
pub const BASS_POW: f32 = 0.84;
pub const MIDS_POW: f32 = 0.75;
pub const HIGH_POW: f32 = 0.445;

const BASS_KICK: f32 = 0.05;
const PREVIOUS_BASS_COUNT: usize = 16;

// Simple type to store a single note with normalized frequency and strength
#[derive(Clone, Copy, Default)]
pub struct Note {
    pub freq: f32,
    pub mag: f32,
}
impl Note {
    pub const fn new(freq: f32, mag: f32) -> Self {
        Self { freq, mag }
    }
}

// Audio state to pass to UI thread
#[derive(Default)]
pub struct State {
    pub volume: f32,

    // Notes for each instrument range (bass/mids/high).
    // Allow caller to determine mapping notes to space
    pub bass_note: Note,
    pub mids_notes: [Note; 2],
    pub high_notes: [Note; 2],

    // 3D (Fractals)
    pub kick_angular_velocity: Option<Vector4>,
    pub reactive_bass: Vector3,
    pub reactive_mids: Vector3,
    pub reactive_high: Vector3,
}

// Type to retrieve results from `analyze_frequency_range` helper
struct FrequencyAnalysis {
    pub loudest: Vec<Note>,
    pub total_volume: f32,
}

// Type to retrieve results from `analyze_audio_frequencies` helper
struct SpectrumAnalysis {
    pub bass_analysis: FrequencyAnalysis,
    pub current_bass: Vec<f32>,
    pub mids_analysis: FrequencyAnalysis,
    pub high_analysis: FrequencyAnalysis,
}

// Type for storing state and history of bass notes
struct BassHistoryAndState {
    pub kick_angular_velocity: Option<Vector4>,
    pub last_kick: SystemTime,
    pub previous_bass_index: usize,
    pub previous_bass: [Option<Vec<f32>>; PREVIOUS_BASS_COUNT],
}

// Type to help with passing re-used information in `analyze_audio_frequencies` helper
struct AudioChunkHelper<'a> {
    complex: &'a [Complex<f32>],
    size: usize,
    scale: f32,
    frequency_resolution: f32,
}

// Convert note analysis to 4D vector containing position and note strength
pub fn map_note_to_square(note: Note, pow: f32) -> Vector4 {
    let Vector2 { x, y } = 0.95 * curve_to_square_n(note.freq.powf(pow), 5);
    Vector4::new(x, y, 0., note.mag)
}
pub fn map_note_to_cube(note: Note, pow: f32) -> Vector4 {
    let Vector3 { x, y, z, .. } = 0.9 * map_freq_to_cube(note.freq, pow);
    Vector4::new(x, y, z, note.mag)
}

// Create a new thread for retrieving and processing audio chunks. Results are send over channel
fn spawn_audio_processing_thread(
    sample_rate: f32,
    tx: Sender<State>,
    rx_acc: Receiver<Vec<Complex<f32>>>,
) {
    std::thread::spawn(move || {
        // Calculate some processing constants outside loop
        let size = if sample_rate > 48_000. { 4096 } else { 2048 }; // Use a fixed power-of-two for best performance
        let size_float = size as f32; // Size of the sample buffer as floating point
        let scale = 1. / size_float.sqrt(); // Rescale elements by 1/sqrt(n)
        let frequency_resolution = sample_rate / size_float; // Hertz per frequency bin after applying FFT

        // Store audio in a resizable array before processing, with some extra space to try to avoid heap allocations
        let mut audio_storage_buffer: Vec<Complex<f32>> = Vec::with_capacity(size + 1024);

        // Create factory and FFT once based on size
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(size);

        // Keep track of state that we don't want UI to need to calculate
        let mut bass_state = BassHistoryAndState::default();

        loop {
            // Append incoming audio data until we have sufficient samples
            while audio_storage_buffer.len() < size {
                let mut d = match rx_acc.recv() {
                    Ok(data) => data, // Update audio state vars
                    Err(e) => panic!("Failed to receive data from audio accumulator thread: {e:?}"),
                };
                audio_storage_buffer.append(&mut d);
            }
            let complex = &mut audio_storage_buffer[0..size];

            // Perform FFT on data in-place
            fft.process(complex);

            // Analyze each frequency ranges
            let SpectrumAnalysis {
                bass_analysis,
                current_bass,
                mids_analysis,
                high_analysis,
            } = analyze_audio_frequencies(&AudioChunkHelper {
                complex,
                size,
                scale,
                frequency_resolution,
            });

            // Get total volume from all (relevant) frequencies
            let volume = bass_analysis.total_volume
                + mids_analysis.total_volume
                + high_analysis.total_volume;

            // Update bass state and history
            update_bass_history(&mut bass_state, &bass_analysis, current_bass);

            // Send updated state to UI thread
            match tx.send(State {
                volume,

                bass_note: bass_analysis.loudest[0],
                mids_notes: [mids_analysis.loudest[0], mids_analysis.loudest[1]],
                high_notes: [high_analysis.loudest[0], high_analysis.loudest[1]],

                kick_angular_velocity: bass_state.kick_angular_velocity.take(),
                reactive_bass: map_freq_to_cube(bass_analysis.loudest[0].freq, BASS_POW),
                reactive_mids: map_freq_to_cube(mids_analysis.loudest[0].freq, MIDS_POW),
                reactive_high: map_freq_to_cube(high_analysis.loudest[0].freq, HIGH_POW),
            }) {
                Ok(()) => {}
                Err(_) => println!("UI thread receiver disconnected.."),
            }

            // Optionally print frequency-spectrum to console
            if PRINT_SPECTRUM {
                const DISPLAY_FFT_SIZE: usize = 64;
                let mut display_bins: [f32; DISPLAY_FFT_SIZE] = [0.; DISPLAY_FFT_SIZE];
                let display_start_index = hertz_to_index(30., size, frequency_resolution);
                let display_end_index = hertz_to_index(12_000., size, frequency_resolution);
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
        } // end unconditional `loop`
    });
}

// Create a new audio stream from the default audio-out device.
// The retrieved data is then sent across the given channel to be processed
fn transfer_loopback_chunks_for_processing(
    default_audio_out: &Device,
    audio_config: &SupportedStreamConfig,
    tx_acc: Sender<Vec<Complex<f32>>>,
) -> cpal::Stream {
    // Store channel constants for use in callback
    let channel_count = audio_config.channels() as usize;
    let channel_count_f32 = channel_count as f32;

    // Create loopback stream for passing small audio-chunk to be processed in batches
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
                            / channel_count_f32;
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
        |e| panic!("Error on audio input stream: {e:?}"),
        None,
    ) {
        // Stream was created successfully
        Ok(stream) => {
            // Ensure loopback capture starts
            stream.play().expect("Failed to initiate loopback stream");
            stream
        }

        // Panic application if thread cannot capture audio-out
        Err(e) => panic!("Error capturing audio stream: {e:?}"),
    }
}

// Determine audio-out device and send the processed audio stream back to caller
// through the given asynchronous channel.
pub fn process_loopback_audio_and_send(tx: Sender<State>) -> cpal::Stream {
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
    let audio_config = match default_audio_out.default_output_config() {
        Ok(config) => {
            println!("Default config from output device: {config:?}");
            config
        }
        Err(e) => panic!("Could not find default audio format: {e:?}"),
    };

    // Store stream details we are intersted in
    let sample_rate = audio_config.sample_rate().0 as f32;

    // Create an accumulator channel to compose enough bytes for a reasonable FFT
    let (tx_acc, rx_acc) = bounded(4);
    spawn_audio_processing_thread(sample_rate, tx, rx_acc);

    // Create and return loopback capture stream
    transfer_loopback_chunks_for_processing(&default_audio_out, &audio_config, tx_acc)
}

// Convert normalized frequency to position in cube
fn map_freq_to_cube(freq: f32, pow: f32) -> Vector3 {
    curve_to_cube_n(freq.powf(pow), 6)
}

// Helper function for converting frequency in range [0, 1] to
#[allow(clippy::cast_sign_loss)]
fn normalized_frequency_to_index(f: f32, size: usize) -> usize {
    let max = size - 1;
    max.min(((max as f32) * f).round() as usize)
}

// Helper function for converting frequency in Hertz to buffer index
#[allow(clippy::cast_sign_loss)]
fn hertz_to_index(f: f32, size: usize, frequency_resolution: f32) -> usize {
    (size - 1).min((f / frequency_resolution).round() as usize)
}

// Create helper closure for determining the loudest frequency bin(s) within a frequency range
fn analyze_frequency_range(
    frequency_range: std::ops::Range<f32>,
    count: usize,
    mut delta: f32,
    min_volume: f32,
    vol_freq_scale: f32,
    audio_chunk: &AudioChunkHelper,
) -> FrequencyAnalysis {
    let start_index = hertz_to_index(
        frequency_range.start,
        audio_chunk.size,
        audio_chunk.frequency_resolution,
    );
    let end_index = hertz_to_index(
        frequency_range.end,
        audio_chunk.size,
        audio_chunk.frequency_resolution,
    );
    let len = end_index - start_index;
    let len_float = len as f32;
    delta /= 2.; // Allow caller to specify total width, even though we use distance from center

    // Create sorted array of notes in this frequency range
    let mut total_volume = 0.;
    let mut sorted: Vec<Note> = (0..len)
        .map(|i| {
            let frac = i as f32 / len_float;
            let v = audio_chunk.scale * audio_chunk.complex[start_index + i].norm();
            total_volume += v;
            Note::new(frac, f32::powf(vol_freq_scale, frac) * v)
        })
        .collect();

    let mut loudest: Vec<Note> = Vec::with_capacity(count);
    while !sorted.is_empty() && loudest.len() < count {
        sorted.sort_unstable_by(|x, y| {
            y.mag
                .partial_cmp(&x.mag)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let Note { freq, mag } = sorted[0];
        let remaining: Vec<Note> = sorted
            .into_iter()
            .filter(|x| (freq - x.freq).abs() > delta)
            .collect();

        // Update the strongest and the remaining lists. Reject values too quiet
        loudest.push(Note::new(freq, if mag >= min_volume { mag } else { 0. }));
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
}

// Given an audio chunk, determine information about bass, mids, and highs
fn analyze_audio_frequencies(audio_chunk: &AudioChunkHelper) -> SpectrumAnalysis {
    let (bass_analysis, current_bass) = {
        let frequency_range: std::ops::Range<f32> = 30.0..250.;
        let delta: f32 = 1.;
        let min_volume: f32 = 0.2;
        let vol_freq_scale = 1.825;
        let analysis = analyze_frequency_range(
            frequency_range.clone(),
            1,
            delta,
            min_volume,
            vol_freq_scale,
            audio_chunk,
        );

        // Do extra analysis for bass notes
        let current_bass: Vec<f32> = {
            let start_index = hertz_to_index(
                frequency_range.start,
                audio_chunk.size,
                audio_chunk.frequency_resolution,
            );
            let end_index = hertz_to_index(
                frequency_range.end,
                audio_chunk.size,
                audio_chunk.frequency_resolution,
            );
            let len = end_index - start_index;
            let len_f32 = len as f32;

            (0..len)
                .map(|i| {
                    let frac = i as f32 / len_f32;
                    let v = audio_chunk.scale * audio_chunk.complex[start_index + i].norm();
                    f32::powf(vol_freq_scale, frac) * v
                })
                .collect()
        };

        (analysis, current_bass)
    };
    let mids_analysis = {
        let frequency_range: std::ops::Range<f32> = 250.0..1_800.;
        let delta: f32 = 0.1;
        let min_volume: f32 = 0.025;
        let vol_freq_scale = 3.;
        analyze_frequency_range(
            frequency_range,
            2,
            delta,
            min_volume,
            vol_freq_scale,
            audio_chunk,
        )
    };
    let high_analysis = {
        let frequency_range: std::ops::Range<f32> = 1_800.0..16_000.;
        let delta: f32 = 0.1;
        let min_volume: f32 = 0.005;
        let vol_freq_scale = 8.;
        analyze_frequency_range(
            frequency_range,
            2,
            delta,
            min_volume,
            vol_freq_scale,
            audio_chunk,
        )
    };

    SpectrumAnalysis {
        bass_analysis,
        current_bass,
        mids_analysis,
        high_analysis,
    }
}

// Update the state and history of bass notes given the latest bass analysis
fn update_bass_history(
    bass_state: &mut BassHistoryAndState,
    bass_analysis: &FrequencyAnalysis,
    current_bass: Vec<f32>,
) {
    // Use analysis of bass notes to determine if a kick should occur
    let kick_elapsed = match bass_state.last_kick.elapsed() {
        Ok(d) => d.as_secs_f32(),
        _ => 0.,
    };
    let avg_prev_bass: f32 = bass_state.previous_bass.iter().fold(0., |acc, x| {
        acc + match x {
            Some(v) => {
                let l = v.len();
                if l > 0 {
                    v[normalized_frequency_to_index(bass_analysis.loudest[0].freq, l)]
                } else {
                    0.
                }
            }
            None => 0.,
        }
    }) / (bass_state.previous_bass.len() as f32);

    if (bass_analysis.loudest[0].mag > 4. || bass_analysis.loudest[0].mag * kick_elapsed > 8.)
        && kick_elapsed > 0.8
        && bass_analysis.loudest[0].mag > 1.25
        && bass_analysis.loudest[0].mag > 3. * avg_prev_bass
    {
        let v = space_filling_curves::cube::curve_to_cube_n(
            bass_analysis.loudest[0].freq.powf(BASS_POW),
            6,
        );
        bass_state.kick_angular_velocity = Some(Vector4::new(
            v.x,
            v.y,
            v.z,
            BASS_KICK * bass_analysis.total_volume.sqrt(),
        ));
        bass_state.last_kick = SystemTime::now();
    }

    // Update bass history
    bass_state.previous_bass[bass_state.previous_bass_index] = Some(current_bass);
    bass_state.previous_bass_index =
        (bass_state.previous_bass_index + 1) % bass_state.previous_bass.len();
}

impl Default for BassHistoryAndState {
    fn default() -> Self {
        Self {
            kick_angular_velocity: None,
            last_kick: SystemTime::now(),
            previous_bass_index: 0,
            previous_bass: Default::default(),
        }
    }
}
