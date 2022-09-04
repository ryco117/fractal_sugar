/*
    fractal_sugar - An experimental audio-visualizer combining fractals and particle simulations.
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

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

// Windows API for CMD management
extern crate kernel32;
extern crate user32;
extern crate winapi;

use std::sync::mpsc;
use std::time::SystemTime;

use winit::dpi::PhysicalPosition;
use winit::event::{
    ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent,
};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Fullscreen;

use winapi::um::winuser::{SW_HIDE, SW_SHOW};

use engine::swapchain::RecreateSwapchainResult;

mod app_config;
mod audio;
mod engine;
mod my_math;
mod space_filling_curves;

use app_config::AppConfig;
use audio::AudioState;
use my_math::helpers::{interpolate_floats, interpolate_vec3};
use my_math::{Quaternion, Vector3, Vector4};

// App constants
const BASE_ANGULAR_VELOCITY: f32 = 0.02;
const CURSOR_LOOSE_STRENGTH: f32 = 0.75;
const CURSOR_FIXED_STRENGTH: f32 = 1.75;
const KALEIDOSCOPE_SPEED: f32 = 0.275;
const SCROLL_SENSITIVITY: f32 = 0.15;

#[derive(Clone, Copy)]
enum KaleidoscopeDirection {
    Forward,
    ForwardComplete,
    Backward,
    BackwardComplete,
}

struct LocalAudioState {
    pub latest_volume: f32,

    // Particle forces to apply
    pub big_boomer: Vector4,
    pub curl_attractors: [Vector4; 2],
    pub attractors: [Vector4; 2],

    // Target vectors used for fractal coloring
    pub reactive_bass: Vector3,
    pub reactive_mids: Vector3,
    pub reactive_high: Vector3,

    // Local values used for interpolating values between updates from audio thread
    pub local_volume: f32,
    pub local_angular_velocity: Vector4,
    pub local_reactive_bass: Vector3,
    pub local_reactive_mids: Vector3,
    pub local_reactive_high: Vector3,
    pub local_smooth_bass: Vector3,
    pub local_smooth_mids: Vector3,
    pub local_smooth_high: Vector3,
}
impl Default for LocalAudioState {
    fn default() -> Self {
        Self {
            latest_volume: 0.,

            big_boomer: Vector4::default(),
            curl_attractors: [Vector4::default(); 2],
            attractors: [Vector4::default(); 2],

            // 3D (Fractals)
            reactive_bass: Vector3::default(),
            reactive_mids: Vector3::default(),
            reactive_high: Vector3::default(),

            local_volume: 0.,
            local_angular_velocity: Vector4::new(0., 1., 0., 0.),
            local_reactive_bass: Vector3::default(),
            local_reactive_mids: Vector3::default(),
            local_reactive_high: Vector3::default(),
            local_smooth_bass: Vector3::default(),
            local_smooth_mids: Vector3::default(),
            local_smooth_high: Vector3::default(),
        }
    }
}

fn bool_to_u32(b: bool) -> u32 {
    if b {
        1
    } else {
        0
    }
}

fn main() {
    let app_config = {
        let filepath = "app_config.toml";
        match app_config::parse_file(filepath) {
            Ok(config) => config,
            Err(e) => {
                println!(
                    "Failed to process custom color schemes file `{}`: {:?}",
                    filepath, e
                );
                AppConfig::default()
            }
        }
    };

    // First, create global event loop to manage window events
    let event_loop = EventLoop::new();

    // Use Engine helper to initialize Vulkan instance
    let mut engine = engine::Engine::new(&event_loop, &app_config);

    // Capture reference to audio stream and use message passing to receive data
    let (tx, rx) = mpsc::channel();
    let _capture_stream_option = audio::create_default_loopback(tx);

    // Window state vars
    let mut window_resized = false;
    let mut recreate_swapchain = false;
    let mut window_is_fullscreen = false;
    let mut window_is_focused = true;
    engine.surface().window().focus_window();
    let mut last_frame_time = SystemTime::now();
    let mut last_mouse_movement = SystemTime::now();
    let (console_handle, mut console_visible) = unsafe {
        let hwnd = kernel32::GetConsoleWindow();
        user32::ShowWindow(hwnd, 0);
        (hwnd, false)
    };

    // Audio state vars?
    let mut game_time: f32 = 0.;
    let mut audio_state = LocalAudioState::default();

    // Game state vars?
    let mut fix_particles = true;
    let mut render_particles = true;
    let mut distance_estimator_id = 4;
    let mut camera_quaternion = Quaternion::default();
    let mut is_cursor_visible = true;
    let mut cursor_position = PhysicalPosition::<f64>::default();
    let mut cursor_force = 0.;
    let mut cursor_force_mult = 1.5;
    let mut kaleidoscope = 0.;
    let mut kaleidoscope_dir = KaleidoscopeDirection::BackwardComplete;
    let mut alternate_colors = false;
    let mut particles_are_3d = false;
    let mut color_scheme_index = 0;

    // Run window loop
    println!("Begin window loop...");
    event_loop.run(move |event, _, control_flow| match event {
        // Handle window close
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            println!("The close button was pressed, exiting");
            *control_flow = ControlFlow::Exit;
        }

        // Handle resize
        Event::WindowEvent {
            event: WindowEvent::Resized(_),
            ..
        } => window_resized = true,

        // All UI events have been handled (ie., executes once per frame)
        Event::MainEventsCleared => {
            // Handle per-frame timing
            let now = SystemTime::now();
            let delta_time = now.duration_since(last_frame_time).unwrap().as_secs_f32();
            last_frame_time = now;

            // Handle any changes to audio state
            match rx.try_recv() {
                // Update audio state vars
                Ok(AudioState {
                    volume,

                    bass_note,
                    mids_notes,
                    high_notes,

                    reactive_bass,
                    reactive_mids,
                    reactive_high,

                    kick_angular_velocity,
                }) => {
                    // Update volume
                    audio_state.latest_volume = volume;

                    let (big_boomer, curl_attractors, attractors) = if particles_are_3d {
                        (
                            audio::map_note_to_cube(bass_note, audio::BASS_POW),
                            mids_notes.map(|n| audio::map_note_to_cube(n, audio::MIDS_POW)),
                            high_notes.map(|n| audio::map_note_to_cube(n, audio::HIGH_POW)),
                        )
                    } else {
                        (
                            audio::map_note_to_square(bass_note, audio::BASS_POW),
                            mids_notes.map(|n| audio::map_note_to_square(n, audio::MIDS_POW)),
                            high_notes.map(|n| audio::map_note_to_square(n, audio::HIGH_POW)),
                        )
                    };

                    // Update 2D big boomers
                    if fix_particles {
                        let smooth = 1. - (-7.25 * big_boomer.w * delta_time).exp();
                        audio_state.big_boomer.x +=
                            smooth * (big_boomer.x - audio_state.big_boomer.x);
                        audio_state.big_boomer.y +=
                            smooth * (big_boomer.y - audio_state.big_boomer.y);
                        audio_state.big_boomer.z +=
                            smooth * (big_boomer.z - audio_state.big_boomer.z);
                        audio_state.big_boomer.w = big_boomer.w;
                    } else {
                        audio_state.big_boomer = big_boomer;
                    }

                    // Update 2D (curl)attractors
                    let c_len = curl_attractors.len();
                    let a_len = attractors.len();
                    audio_state.curl_attractors[..c_len].copy_from_slice(&curl_attractors[..c_len]);
                    audio_state.attractors[..a_len].copy_from_slice(&attractors[..a_len]);

                    // Update fractal state
                    if let Some(omega) = kick_angular_velocity {
                        audio_state.local_angular_velocity = omega;
                    }
                    audio_state.reactive_bass = reactive_bass;
                    audio_state.reactive_mids = reactive_mids;
                    audio_state.reactive_high = reactive_high;
                }

                // No new data, continue on
                Err(mpsc::TryRecvError::Empty) => {}

                // Unexpected error, bail
                Err(e) => panic!("Failed to receive data from audio thread: {:?}", e),
            }

            // Update per-frame state
            interpolate_floats(
                &mut audio_state.local_volume,
                audio_state.latest_volume,
                delta_time * -1.8,
            );
            let audio_scaled_delta_time = delta_time * audio_state.local_volume.sqrt();
            game_time += audio_scaled_delta_time;
            camera_quaternion.rotate_by(Quaternion::build(
                audio_state.local_angular_velocity.xyz(),
                delta_time * audio_state.local_angular_velocity.w,
            ));
            interpolate_floats(
                &mut audio_state.local_angular_velocity.w,
                BASE_ANGULAR_VELOCITY,
                delta_time * -0.375,
            );
            interpolate_vec3(
                &mut audio_state.local_reactive_bass,
                &audio_state.reactive_bass,
                delta_time * (0.8 * audio_state.big_boomer.w.sqrt()).min(1.) * -0.36,
            );
            interpolate_vec3(
                &mut audio_state.local_reactive_mids,
                &audio_state.reactive_mids,
                delta_time * (0.8 * audio_state.curl_attractors[0].w.sqrt()).min(1.) * -0.36,
            );
            interpolate_vec3(
                &mut audio_state.local_reactive_high,
                &audio_state.reactive_high,
                delta_time * (0.8 * audio_state.attractors[0].w.sqrt()).min(1.) * -0.36,
            );
            interpolate_vec3(
                &mut audio_state.local_smooth_bass,
                &audio_state.local_reactive_bass,
                delta_time * -0.15,
            );
            interpolate_vec3(
                &mut audio_state.local_smooth_mids,
                &audio_state.local_reactive_mids,
                delta_time * -0.15,
            );
            interpolate_vec3(
                &mut audio_state.local_smooth_high,
                &audio_state.local_reactive_high,
                delta_time * -0.15,
            );
            (kaleidoscope, kaleidoscope_dir) = match kaleidoscope_dir {
                KaleidoscopeDirection::Forward => {
                    kaleidoscope += KALEIDOSCOPE_SPEED * audio_scaled_delta_time;
                    if kaleidoscope >= 1. {
                        (1., KaleidoscopeDirection::ForwardComplete)
                    } else {
                        (kaleidoscope, KaleidoscopeDirection::Forward)
                    }
                }
                KaleidoscopeDirection::Backward => {
                    kaleidoscope -= KALEIDOSCOPE_SPEED * audio_scaled_delta_time;
                    if kaleidoscope <= 0. {
                        (0., KaleidoscopeDirection::BackwardComplete)
                    } else {
                        (kaleidoscope, KaleidoscopeDirection::Backward)
                    }
                }
                _ => (kaleidoscope, kaleidoscope_dir),
            };

            let surface = engine.surface();

            // If cursor is visible and has been stationary then hide it
            if is_cursor_visible
                && window_is_focused
                && last_mouse_movement.elapsed().unwrap().as_secs_f32() > 2.
            {
                surface.window().set_cursor_visible(false);
                is_cursor_visible = false;
            }

            // Handle any necessary recreations (usually from window resizing)
            let dimensions = surface.window().inner_size();
            if window_resized || recreate_swapchain {
                match engine.recreate_swapchain(dimensions, window_resized) {
                    RecreateSwapchainResult::Success => {
                        recreate_swapchain = false;
                        window_resized = false;
                    }
                    RecreateSwapchainResult::ExtentNotSupported => return,
                }
            }

            let width = dimensions.width as f32;
            let height = dimensions.height as f32;
            let aspect_ratio = width / height;

            // Create a unique attractor based on mouse position
            let cursor_attractor = {
                let strength = if fix_particles {
                    CURSOR_FIXED_STRENGTH
                } else {
                    CURSOR_LOOSE_STRENGTH
                } * cursor_force_mult
                    * cursor_force;

                let x_norm = (2. * (cursor_position.x / dimensions.width as f64) - 1.) as f32;
                let y_norm = (2. * (cursor_position.y / dimensions.height as f64) - 1.) as f32;
                if render_particles && particles_are_3d && cursor_force != 0. {
                    const VERTICAL_FOV: f32 = std::f32::consts::FRAC_PI_2 / 2.5; // Roughly 70 degree vertical VERTICAL_FOV
                    const PARTICLE_CAMERA_ORBIT: Vector3 = Vector3::new(0., 0., 1.75); // Keep in sync with orbit of `particles.vert`
                    const PERSPECTIVE_DISTANCE: f32 = 1.35;
                    let fov_y = VERTICAL_FOV.tan();
                    let fov_x = fov_y * aspect_ratio;

                    // Map cursor to 3D world using camera orientation
                    let mut v = camera_quaternion.rotate_point(
                        PERSPECTIVE_DISTANCE * Vector3::new(x_norm * fov_x, y_norm * fov_y, -1.),
                    );
                    v += camera_quaternion.rotate_point(PARTICLE_CAMERA_ORBIT);
                    [v.x, v.y, v.z, strength]
                } else {
                    [x_norm, y_norm, 0., strength]
                }
            };

            // Create per-frame data for particle compute-shader
            let particle_data = if render_particles {
                let compute = engine::ParticleComputePushConstants {
                    big_boomer: audio_state.big_boomer.into(),

                    curl_attractors: audio_state.curl_attractors.map(std::convert::Into::into),

                    attractors: [
                        audio_state.attractors[0].into(),
                        audio_state.attractors[1].into(),
                        cursor_attractor,
                    ],

                    time: game_time,
                    delta_time,
                    width,
                    height,
                    fix_particles: bool_to_u32(fix_particles),
                    use_third_dimension: bool_to_u32(particles_are_3d),
                };

                let vertex = engine::ParticleVertexPushConstants {
                    quaternion: camera_quaternion.into(),
                    time: game_time,
                    aspect_ratio,
                    rendering_fractal: bool_to_u32(distance_estimator_id != 0),
                    alternate_colors: bool_to_u32(alternate_colors),
                    use_third_dimension: bool_to_u32(particles_are_3d),
                };

                Some((compute, vertex))
            } else {
                None
            };

            // Create fractal data
            let fractal_data = engine::FractalPushConstants {
                quaternion: camera_quaternion.into(),

                reactive_bass: audio_state.local_reactive_bass.into(),
                reactive_mids: audio_state.local_reactive_mids.into(),
                reactive_high: audio_state.local_reactive_high.into(),

                smooth_bass: audio_state.local_smooth_bass.into(),
                smooth_mids: audio_state.local_smooth_mids.into(),
                smooth_high: audio_state.local_smooth_high.into(),

                time: game_time,
                aspect_ratio,
                kaleidoscope: kaleidoscope.powf(0.65),
                distance_estimator_id,
                orbit_distance: if render_particles && particles_are_3d {
                    1.42
                } else {
                    1.
                },
                render_background: bool_to_u32(!render_particles),
            };

            // Draw frame and return whether a swapchain recreation was deemed necessary
            recreate_swapchain |= engine.draw_frame(particle_data, fractal_data);
        }

        // Handle some keyboard input
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                },
            ..
        } => {
            match keycode {
                // Handle fullscreen toggle (F11)
                VirtualKeyCode::F11 => {
                    if window_is_fullscreen {
                        engine.surface().window().set_fullscreen(None);
                        window_is_fullscreen = false;
                    } else {
                        engine
                            .surface()
                            .window()
                            .set_fullscreen(Some(Fullscreen::Borderless(None)));
                        window_is_fullscreen = true;
                    }
                }

                // Handle Escape key
                VirtualKeyCode::Escape => {
                    if window_is_fullscreen {
                        // Leave fullscreen
                        engine.surface().window().set_fullscreen(None);
                        window_is_fullscreen = false;
                    } else {
                        // Exit window loop
                        println!("The Escape key was pressed, exiting");
                        *control_flow = ControlFlow::Exit;
                    }
                }

                // Handle Space bar for toggling Kaleidoscope effect
                VirtualKeyCode::Space => {
                    #[allow(clippy::enum_glob_use)]
                    use KaleidoscopeDirection::*;
                    kaleidoscope_dir = match kaleidoscope_dir {
                        Forward | ForwardComplete => Backward,
                        Backward | BackwardComplete => Forward,
                    }
                }

                // Handle toggling of Jello mode (fixing particles)
                VirtualKeyCode::J => fix_particles = !fix_particles,

                // Handle toggling of particle rendering
                VirtualKeyCode::P => render_particles = !render_particles,

                // Handle toggling of alternate colors
                VirtualKeyCode::Capital => alternate_colors = !alternate_colors,

                // Handle toggling of 3D particles
                VirtualKeyCode::D => particles_are_3d = !particles_are_3d,

                // Tab through different color schemes / palattes ?
                VirtualKeyCode::Tab => {
                    color_scheme_index = (color_scheme_index + 1) % app_config.color_schemes.len();
                    engine.update_color_scheme(app_config.color_schemes[color_scheme_index]);
                }

                // Set different fractal types
                VirtualKeyCode::Key0 => distance_estimator_id = 0,
                VirtualKeyCode::Key1 => distance_estimator_id = 1,
                VirtualKeyCode::Key2 => distance_estimator_id = 2,
                VirtualKeyCode::Key3 => distance_estimator_id = 3,
                VirtualKeyCode::Key4 => distance_estimator_id = 4,
                VirtualKeyCode::Key5 => distance_estimator_id = 5,

                // Handle toggling the debug-console.
                // NOTE: Does not successfully hide `Windows Terminal` based CMD prompts
                VirtualKeyCode::Return => unsafe {
                    user32::ShowWindow(
                        console_handle,
                        if console_visible { SW_HIDE } else { SW_SHOW },
                    );
                    console_visible = !console_visible;
                },

                // No-op
                _ => {}
            }
        }

        // Track window focus in a state var
        Event::WindowEvent {
            event: WindowEvent::Focused(focused),
            ..
        } => {
            if !focused {
                // Force cursor visibility when focus is lost
                engine.surface().window().set_cursor_visible(true);
                is_cursor_visible = true;
            }
            window_is_focused = focused;
        }

        // Handle mouse movement
        Event::WindowEvent {
            event: WindowEvent::CursorMoved { position, .. },
            ..
        } => {
            last_mouse_movement = SystemTime::now();
            engine.surface().window().set_cursor_visible(true);
            is_cursor_visible = true;

            cursor_position = position;
        }

        // Handle mouse buttons to allow cursor-applied forces
        Event::WindowEvent {
            event: WindowEvent::MouseInput { state, button, .. },
            ..
        } => {
            let pressed = match state {
                ElementState::Pressed => 1.,
                ElementState::Released => -1.,
            };
            cursor_force += pressed
                * match button {
                    MouseButton::Left => -1.,
                    MouseButton::Right => 1.,
                    _ => 0.,
                };
        }

        // Handle mouse scroll wheel to change strength of cursor-applied forces
        Event::WindowEvent {
            event: WindowEvent::MouseWheel { delta, .. },
            ..
        } => {
            let delta = match delta {
                MouseScrollDelta::LineDelta(_, y) => y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32,
            };
            cursor_force_mult *= (SCROLL_SENSITIVITY * delta).exp();
        }

        // Catch-all
        _ => {}
    })
}
