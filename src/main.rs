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

// TODO: Remove file-wide allow statements
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use std::time::SystemTime;

use engine::Engine;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent,
};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Fullscreen;

use engine::swapchain::RecreateSwapchainResult;

mod app_config;
mod audio;
mod engine;
mod my_math;
mod space_filling_curves;

use app_config::AppConfig;
use my_math::helpers::{interpolate_floats, interpolate_vec3};
use my_math::{Quaternion, Vector3, Vector4};

// App constants
const BASE_ANGULAR_VELOCITY: f32 = 0.02;
const CURSOR_LOOSE_STRENGTH: f32 = 0.75;
const CURSOR_FIXED_STRENGTH: f32 = 1.75;
const KALEIDOSCOPE_SPEED: f32 = 0.275;
const SCROLL_SENSITIVITY: f32 = 0.15;

enum AlternateColors {
    Normal,
    Inverse,
}
enum KaleidoscopeDirection {
    Forward,
    ForwardComplete,
    Backward,
    BackwardComplete,
}

struct LocalAudioState {
    pub play_time: f32,
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

#[allow(clippy::struct_excessive_bools)]
struct GameState {
    pub fix_particles: bool,
    pub render_particles: bool,
    pub distance_estimator_id: u32,
    pub camera_quaternion: Quaternion,
    pub is_cursor_visible: bool,
    pub cursor_position: PhysicalPosition<f64>,
    pub cursor_force: f32,
    pub cursor_force_mult: f32,
    pub kaleidoscope: f32,
    pub kaleidoscope_dir: KaleidoscopeDirection,
    pub alternate_colors: AlternateColors,
    pub particles_are_3d: bool,
    pub color_scheme_index: usize,
}

#[allow(clippy::struct_excessive_bools)]
struct WindowState {
    pub resized: bool,
    pub recreate_swapchain: bool,
    pub is_fullscreen: bool,
    pub is_focused: bool,
    pub last_frame_time: SystemTime,
    pub last_mouse_movement: SystemTime,
}

fn main() {
    // Determine the runtime app configuration
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

    // Create global event loop to manage window events
    let event_loop = EventLoop::new();

    // Use Engine helper to initialize Vulkan instance
    let mut engine = engine::Engine::new(&event_loop, &app_config);

    // Capture reference to audio stream and use message passing to receive data
    let (tx, rx) = crossbeam_channel::bounded(4);
    let _capture_stream_option = audio::process_loopback_audio_and_send(tx);

    // State vars
    engine.surface().window().focus_window();
    let mut window_state = WindowState::default();
    let mut audio_state = LocalAudioState::default();
    let mut game_state = GameState::default();

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
        } => window_state.resized = true,

        // All UI events have been handled (ie., executes once per frame)
        Event::MainEventsCleared => tock_frame(
            &mut engine,
            &mut audio_state,
            &mut game_state,
            &mut window_state,
            &rx,
        ),

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
                    if window_state.is_fullscreen {
                        engine.surface().window().set_fullscreen(None);
                        window_state.is_fullscreen = false;
                    } else {
                        engine
                            .surface()
                            .window()
                            .set_fullscreen(Some(Fullscreen::Borderless(None)));
                        window_state.is_fullscreen = true;
                    }
                }

                // Handle Escape key
                VirtualKeyCode::Escape => {
                    if window_state.is_fullscreen {
                        // Leave fullscreen
                        engine.surface().window().set_fullscreen(None);
                        window_state.is_fullscreen = false;
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
                    game_state.kaleidoscope_dir = match game_state.kaleidoscope_dir {
                        Forward | ForwardComplete => Backward,
                        Backward | BackwardComplete => Forward,
                    }
                }

                // Handle toggling of Jello mode (i.e., fixing particles to positions)
                VirtualKeyCode::J => game_state.fix_particles = !game_state.fix_particles,

                // Handle toggling of particle rendering
                VirtualKeyCode::P => game_state.render_particles = !game_state.render_particles,

                // Handle toggling of alternate colors
                VirtualKeyCode::Capital => {
                    game_state.alternate_colors = match game_state.alternate_colors {
                        AlternateColors::Inverse => AlternateColors::Normal,
                        AlternateColors::Normal => AlternateColors::Inverse,
                    }
                }

                // Handle toggling of 3D particles
                VirtualKeyCode::D => game_state.particles_are_3d = !game_state.particles_are_3d,

                // Tab through different color schemes / palattes ?
                VirtualKeyCode::Tab => {
                    game_state.color_scheme_index =
                        (game_state.color_scheme_index + 1) % app_config.color_schemes.len();
                    engine.update_color_scheme(
                        app_config.color_schemes[game_state.color_scheme_index],
                    );
                }

                // Set different fractal types
                VirtualKeyCode::Key0 => game_state.distance_estimator_id = 0,
                VirtualKeyCode::Key1 => game_state.distance_estimator_id = 1,
                VirtualKeyCode::Key2 => game_state.distance_estimator_id = 2,
                VirtualKeyCode::Key3 => game_state.distance_estimator_id = 3,
                VirtualKeyCode::Key4 => game_state.distance_estimator_id = 4,
                VirtualKeyCode::Key5 => game_state.distance_estimator_id = 5,

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
                game_state.is_cursor_visible = true;
            }
            window_state.is_focused = focused;
        }

        // Handle mouse movement
        Event::WindowEvent {
            event: WindowEvent::CursorMoved { position, .. },
            ..
        } => {
            window_state.last_mouse_movement = SystemTime::now();
            engine.surface().window().set_cursor_visible(true);
            game_state.is_cursor_visible = true;

            game_state.cursor_position = position;
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
            game_state.cursor_force += pressed
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
            game_state.cursor_force_mult *= (SCROLL_SENSITIVITY * delta).exp();
        }

        // Catch-all
        _ => {}
    })
}

// Update per-frame state and draw to window
fn tock_frame(
    engine: &mut Engine,
    audio_state: &mut LocalAudioState,
    game_state: &mut GameState,
    window_state: &mut WindowState,
    rx: &crossbeam_channel::Receiver<audio::State>,
) {
    // Handle per-frame timing
    let now = SystemTime::now();
    let delta_time = now
        .duration_since(window_state.last_frame_time)
        .unwrap_or_default()
        .as_secs_f32();
    window_state.last_frame_time = now;

    // Handle any changes to audio state from the input stream
    update_audio_state_from_stream(rx, audio_state, delta_time, game_state);

    // Update per-frame state
    interpolate_frames(audio_state, delta_time, game_state);

    let surface = engine.surface();

    // If cursor is visible and has been stationary then hide it
    if game_state.is_cursor_visible
        && window_state.is_focused
        && window_state
            .last_mouse_movement
            .elapsed()
            .unwrap_or_default()
            .as_secs_f32()
            > 2.
    {
        surface.window().set_cursor_visible(false);
        game_state.is_cursor_visible = false;
    }

    // Handle any necessary recreations (usually from window resizing)
    let dimensions = surface.window().inner_size();
    if window_state.resized || window_state.recreate_swapchain {
        match engine.recreate_swapchain(dimensions, window_state.resized) {
            RecreateSwapchainResult::Success => {
                window_state.recreate_swapchain = false;
                window_state.resized = false;
            }
            RecreateSwapchainResult::ExtentNotSupported => return,
        }
    }

    let width = dimensions.width as f32;
    let height = dimensions.height as f32;
    let aspect_ratio = width / height;

    // Create per-frame data for particle compute-shader
    let particle_data = if game_state.render_particles {
        // Create a unique attractor based on mouse position
        let cursor_attractor = {
            let strength = if game_state.fix_particles {
                CURSOR_FIXED_STRENGTH
            } else {
                CURSOR_LOOSE_STRENGTH
            } * game_state.cursor_force_mult
                * game_state.cursor_force;

            let Vector3 { x, y, z, .. } =
                screen_position_to_world(game_state, dimensions, aspect_ratio);
            [x, y, z, strength]
        };

        let compute = engine::ParticleComputePushConstants {
            big_boomer: audio_state.big_boomer.into(),

            curl_attractors: audio_state.curl_attractors.map(std::convert::Into::into),

            attractors: [
                audio_state.attractors[0].into(),
                audio_state.attractors[1].into(),
                cursor_attractor,
            ],

            time: audio_state.play_time,
            delta_time,
            width,
            height,
            fix_particles: bool_to_u32(game_state.fix_particles),
            use_third_dimension: bool_to_u32(game_state.particles_are_3d),
        };

        let vertex = engine::ParticleVertexPushConstants {
            quaternion: game_state.camera_quaternion.into(),
            time: audio_state.play_time,
            aspect_ratio,
            rendering_fractal: bool_to_u32(game_state.distance_estimator_id != 0),
            alternate_colors: match game_state.alternate_colors {
                AlternateColors::Inverse => 1,
                AlternateColors::Normal => 0,
            },
            use_third_dimension: bool_to_u32(game_state.particles_are_3d),
        };

        Some((compute, vertex))
    } else {
        None
    };

    // Create fractal data
    let fractal_data = engine::FractalPushConstants {
        quaternion: game_state.camera_quaternion.into(),

        reactive_bass: audio_state.local_reactive_bass.into(),
        reactive_mids: audio_state.local_reactive_mids.into(),
        reactive_high: audio_state.local_reactive_high.into(),

        smooth_bass: audio_state.local_smooth_bass.into(),
        smooth_mids: audio_state.local_smooth_mids.into(),
        smooth_high: audio_state.local_smooth_high.into(),

        time: audio_state.play_time,
        aspect_ratio,
        kaleidoscope: game_state.kaleidoscope.powf(0.65),
        distance_estimator_id: game_state.distance_estimator_id,
        orbit_distance: if game_state.render_particles && game_state.particles_are_3d {
            1.42
        } else {
            1.
        },
        render_background: bool_to_u32(!game_state.render_particles),
    };

    // Draw frame and return whether a swapchain recreation was deemed necessary
    window_state.recreate_swapchain |= engine.draw_frame(particle_data, fractal_data);
}

// Helper for receiving the latest audio state from the input stream
fn update_audio_state_from_stream(
    rx: &crossbeam_channel::Receiver<audio::State>,
    audio_state: &mut LocalAudioState,
    delta_time: f32,
    game_state: &GameState,
) {
    // Handle any changes to audio state
    match rx.try_recv() {
        // Update audio state vars
        Ok(audio::State {
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

            let (big_boomer, curl_attractors, attractors) = if game_state.particles_are_3d {
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
            if game_state.fix_particles {
                let smooth = 1. - (-7.25 * big_boomer.w * delta_time).exp();
                audio_state.big_boomer.x += smooth * (big_boomer.x - audio_state.big_boomer.x);
                audio_state.big_boomer.y += smooth * (big_boomer.y - audio_state.big_boomer.y);
                audio_state.big_boomer.z += smooth * (big_boomer.z - audio_state.big_boomer.z);
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
        Err(crossbeam_channel::TryRecvError::Empty) => {}

        // Unexpected error, bail
        Err(e) => panic!("Failed to receive data from audio thread: {:?}", e),
    }
}

// Helper for interpolating data on a per-frame basis
fn interpolate_frames(
    audio_state: &mut LocalAudioState,
    delta_time: f32,
    game_state: &mut GameState,
) {
    interpolate_floats(
        &mut audio_state.local_volume,
        audio_state.latest_volume,
        delta_time * -1.8,
    );
    let audio_scaled_delta_time = delta_time * audio_state.local_volume.sqrt();
    audio_state.play_time += audio_scaled_delta_time;
    game_state.camera_quaternion.rotate_by(Quaternion::build(
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

    // Check and possibly update kaleidoscope animation state
    match game_state.kaleidoscope_dir {
        KaleidoscopeDirection::Forward => {
            game_state.kaleidoscope += KALEIDOSCOPE_SPEED * audio_scaled_delta_time;
            if game_state.kaleidoscope >= 1. {
                game_state.kaleidoscope = 1.;
                game_state.kaleidoscope_dir = KaleidoscopeDirection::ForwardComplete;
            }
        }
        KaleidoscopeDirection::Backward => {
            game_state.kaleidoscope -= KALEIDOSCOPE_SPEED * audio_scaled_delta_time;
            if game_state.kaleidoscope <= 0. {
                game_state.kaleidoscope = 0.;
                game_state.kaleidoscope_dir = KaleidoscopeDirection::BackwardComplete;
            }
        }
        _ => {}
    };
}

// Use game state to correctly map positions from screen space to world
fn screen_position_to_world(
    game_state: &GameState,
    dimensions: PhysicalSize<u32>,
    aspect_ratio: f32,
) -> Vector3 {
    #[allow(clippy::cast_lossless)]
    fn normalize_cursor(p: f64, max: u32) -> f32 {
        (2. * (p / max as f64) - 1.) as f32
    }
    let x_norm = normalize_cursor(game_state.cursor_position.x, dimensions.width);
    let y_norm = normalize_cursor(game_state.cursor_position.y, dimensions.height);

    if game_state.particles_are_3d && game_state.cursor_force != 0. {
        const VERTICAL_FOV: f32 = std::f32::consts::FRAC_PI_2 / 2.5; // Roughly 70 degree vertical VERTICAL_FOV
        const PARTICLE_CAMERA_ORBIT: Vector3 = Vector3::new(0., 0., 1.75); // Keep in sync with orbit of `particles.vert`
        const PERSPECTIVE_DISTANCE: f32 = 1.35;
        let fov_y = VERTICAL_FOV.tan();
        let fov_x = fov_y * aspect_ratio;

        // Map cursor to 3D world using camera orientation
        let mut v = game_state
            .camera_quaternion
            .rotate_point(PERSPECTIVE_DISTANCE * Vector3::new(x_norm * fov_x, y_norm * fov_y, -1.));
        v += game_state
            .camera_quaternion
            .rotate_point(PARTICLE_CAMERA_ORBIT);
        Vector3::new(v.x, v.y, v.z)
    } else {
        Vector3::new(x_norm, y_norm, 0.)
    }
}

// Helper for converting booleans to unsigned 32-bit integers.
// This is necessary for GPU memory alignment
fn bool_to_u32(b: bool) -> u32 {
    if b {
        1
    } else {
        0
    }
}

impl Default for LocalAudioState {
    fn default() -> Self {
        Self {
            play_time: 0.,
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

impl Default for GameState {
    fn default() -> Self {
        Self {
            fix_particles: true,
            render_particles: true,
            distance_estimator_id: 4,
            camera_quaternion: Quaternion::default(),
            is_cursor_visible: true,
            cursor_position: PhysicalPosition::<f64>::default(),
            cursor_force: 0.,
            cursor_force_mult: 1.5,
            kaleidoscope: 0.,
            kaleidoscope_dir: KaleidoscopeDirection::BackwardComplete,
            alternate_colors: AlternateColors::Normal,
            particles_are_3d: false,
            color_scheme_index: 0,
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            resized: false,
            recreate_swapchain: false,
            is_fullscreen: false,
            is_focused: true,
            last_frame_time: SystemTime::now(),
            last_mouse_movement: SystemTime::now(),
        }
    }
}
