/*
    fractal_sugar - An experimental audio visualizer combining fractals and particle simulations.
    Copyright (C) 2022,2023  Ryan Andersen

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

// Ensure Windows release builds are not console apps.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// TODO: Remove file-wide allow statements.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
use std::time::Instant;

use app_overlay::AppOverlay;
#[cfg(all(not(debug_assertions), target_os = "windows"))]
use companion_console::ConsoleState;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent,
};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Fullscreen;

use engine::core::{RecreateSwapchainResult, WindowSurface};
use engine::{DrawData, Engine};

mod app_config;
mod app_overlay;
mod audio;
mod engine;
mod my_math;
mod space_filling_curves;

use app_config::{AppConfig, Scheme};
use my_math::helpers::{interpolate_floats, interpolate_vec3};
use my_math::{Quaternion, Vector3, Vector4};

// App constants
const BASE_ANGULAR_VELOCITY: f32 = 0.02;
const CURSOR_LOOSE_STRENGTH: f32 = 0.75;
const CURSOR_FIXED_STRENGTH: f32 = 1.75;
const KALEIDOSCOPE_SPEED: f32 = 0.275;
const SCROLL_SENSITIVITY: f32 = 0.15;

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

// Game-state enums
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
#[derive(PartialEq)]
enum ParticleTension {
    None,
    Spring,
}

#[derive(Clone, Copy)]
pub struct RuntimeConstants {
    pub distance_estimator_id: u32,
    pub render_particles: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct GameState {
    pub fix_particles: ParticleTension,
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
    pub audio_responsive: bool,
    pub runtime_constants: RuntimeConstants,
}

#[allow(clippy::struct_excessive_bools)]
struct WindowState {
    pub resized: bool,
    pub recreate_swapchain: bool,
    pub is_fullscreen: bool,
    pub is_focused: bool,
    pub last_frame_time: Instant,
    pub last_mouse_movement: Instant,
}

// A helper for managing the audio input stream and the resulting audio-based state.
struct AudioManager {
    pub receiver: crossbeam_channel::Receiver<audio::State>,
    pub capture_stream: cpal::Stream,
    pub state: LocalAudioState,
}

struct FractalSugar {
    color_schemes: Vec<Scheme>,
    color_scheme_names: Vec<String>,

    app_overlay: AppOverlay,
    engine: Engine,
    event_loop: Option<EventLoop<()>>,

    #[cfg(all(not(debug_assertions), target_os = "windows"))]
    console_state: Option<ConsoleState>,

    audio: AudioManager,
    game_state: GameState,
    window_state: WindowState,
}

fn main() {
    // Initialize app instance
    let fractal_sugar = FractalSugar::new();

    // Endless app-loop
    fractal_sugar.run()
}

impl FractalSugar {
    pub fn new() -> Self {
        // Windows-specific console clean-up. Important that this occurs before print statements for debugging
        #[cfg(all(not(debug_assertions), target_os = "windows"))]
        let console_state = ConsoleState::new(false);

        // Fetch command-line arguments
        let args: Vec<String> = std::env::args().collect();
        assert!(args.len() <= 2, "fractal_sugar accepts at most one argument, the TOML app configuration file. The default path is 'app_config.toml'");

        // Determine the runtime app configuration
        let app_config = {
            let filepath = match args.get(1) {
                Some(path) => path.as_str(),
                None => "app_config.toml",
            };
            match app_config::parse_file(filepath) {
                Ok(config) => config,
                Err(e) => {
                    println!("Failed to process custom color schemes file `{filepath}`: {e:?}");
                    AppConfig::default()
                }
            }
        };

        // Load icon from file resources
        let icon = {
            let icon_bytes = std::include_bytes!("../res/fractal_sugar.ico");
            let ico_reader = std::io::Cursor::<&[u8]>::new(icon_bytes);
            let ico_list = ico::IconDir::read(ico_reader).unwrap();
            let ico = ico_list
                .entries()
                .get(0)
                .expect("Icon doesn't have any layers");
            let image = ico.decode().unwrap();

            match winit::window::Icon::from_rgba(
                image.rgba_data().to_vec(),
                image.width(),
                image.height(),
            ) {
                Ok(icon) => Some(icon),
                Err(e) => {
                    println!("Failed to parse icon: {e:?}");
                    None
                }
            }
        };

        // Create global event loop to manage window events
        let event_loop = EventLoop::new();

        // Initialize game state so that the engine can leverage default values.
        let game_state = GameState::default();

        // Use Engine helper to initialize Vulkan instance
        let engine =
            engine::Engine::new(&event_loop, &app_config, game_state.runtime_constants, icon);

        // State vars
        engine.window().focus_window();
        let window_state = WindowState {
            is_fullscreen: app_config.launch_fullscreen,
            resized: false,
            recreate_swapchain: false,
            is_focused: true,
            last_frame_time: Instant::now(),
            last_mouse_movement: Instant::now(),
        };

        let config_window = AppOverlay::new(
            engine.surface().clone(),
            engine.swapchain(),
            engine.queue().clone(),
            &event_loop,
            engine.gui_pass(),
            &app_config,
        );

        Self {
            color_schemes: app_config.color_schemes,
            color_scheme_names: app_config.color_scheme_names,
            app_overlay: config_window,
            engine,
            event_loop: Some(event_loop),
            audio: AudioManager::default(),
            game_state,
            window_state,

            #[cfg(all(not(debug_assertions), target_os = "windows"))]
            console_state,
        }
    }

    pub fn run(mut self) -> ! {
        // Run window loop
        println!("Begin window loop...");
        self.event_loop
            .take()
            .unwrap()
            .run(move |event, _, control_flow| match event {
                // All UI events have been handled (i.e., executes once per frame).
                Event::MainEventsCleared => self.tock_frame(),

                Event::WindowEvent { event, .. } => {
                    let mut handle_event = true;
                    if self.app_overlay.visible() {
                        // Determine if this event should be handled by the config window.
                        handle_event = !self.app_overlay.handle_input(&event);
                    }

                    if handle_event {
                        self.handle_window_event(&event, control_flow);
                    }
                }
                _ => {}
            })
    }

    // Update per-frame state and draw to window
    fn tock_frame(&mut self) {
        // Handle per-frame timing
        let now = Instant::now();
        let delta_time = now
            .duration_since(self.window_state.last_frame_time)
            .as_secs_f32();
        self.window_state.last_frame_time = now;

        // Handle any changes to audio state from the input stream
        self.update_audio_state_from_stream(delta_time);

        // Update per-frame state
        self.interpolate_frames(delta_time);

        let surface = self.engine.surface();

        // If cursor is visible and has been stationary then hide it
        let window = surface.window();
        if self.game_state.is_cursor_visible
            && !self.app_overlay.visible()
            && self.window_state.is_focused
            && self
                .window_state
                .last_mouse_movement
                .elapsed()
                .as_secs_f32()
                > 2.
        {
            window.set_cursor_visible(false);
            self.game_state.is_cursor_visible = false;
        }

        // Handle any necessary recreations (usually from window resizing)
        let dimensions = window.inner_size();
        if self.window_state.resized || self.window_state.recreate_swapchain {
            match self
                .engine
                .recreate_swapchain(dimensions, self.window_state.resized)
            {
                RecreateSwapchainResult::Ok => {
                    self.window_state.recreate_swapchain = false;
                    self.window_state.resized = false;
                }
                RecreateSwapchainResult::ExtentNotSupported => return,
            }
        }

        // Create per-frame data for particle compute-shader
        let draw_data = self.next_shader_data(delta_time, self.engine.window().inner_size());

        // Get an optional command buffer to render the GUI
        let gui_command_buffer = if self.app_overlay.visible() {
            // Render the config as an overlay
            self.app_overlay.draw(
                &mut self.engine,
                &self.color_scheme_names,
                &mut self.color_schemes,
                &mut self.game_state.color_scheme_index,
            )
        } else {
            None
        };

        // Draw frame and return whether a swapchain recreation was deemed necessary
        let (future, suboptimal) = match self.engine.render(&draw_data, gui_command_buffer) {
            Ok(pair) => pair,
            Err(vulkano::swapchain::AcquireError::OutOfDate) => {
                self.window_state.recreate_swapchain = true;
                return;
            }
            Err(e) => panic!("Failed to acquire next image: {e:?}"),
        };

        self.window_state.recreate_swapchain |= self.engine.present(future) || suboptimal;
    }

    // Helper for receiving the latest audio state from the input stream
    fn update_audio_state_from_stream(&mut self, delta_time: f32) {
        // Allow user to toggle audio-responsiveness
        if !self.game_state.audio_responsive {
            match self.audio.receiver.try_recv() {
                Ok(_) | Err(crossbeam_channel::TryRecvError::Empty) => {}

                // Unexpected error, bail
                Err(e) => panic!("Failed to receive data from audio thread: {e:?}"),
            }
            return;
        }

        // Handle any changes to audio state
        match self.audio.receiver.try_recv() {
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
                self.audio.state.latest_volume = volume;

                let (big_boomer, curl_attractors, attractors) = if self.game_state.particles_are_3d
                {
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
                match self.game_state.fix_particles {
                    ParticleTension::Spring => {
                        let smooth = 1. - (-7.25 * big_boomer.w * delta_time).exp();
                        self.audio.state.big_boomer.x +=
                            smooth * (big_boomer.x - self.audio.state.big_boomer.x);
                        self.audio.state.big_boomer.y +=
                            smooth * (big_boomer.y - self.audio.state.big_boomer.y);
                        self.audio.state.big_boomer.z +=
                            smooth * (big_boomer.z - self.audio.state.big_boomer.z);
                        self.audio.state.big_boomer.w = big_boomer.w;
                    }
                    ParticleTension::None => self.audio.state.big_boomer = big_boomer,
                }

                // Update 2D (curl)attractors
                let c_len = curl_attractors.len();
                let a_len = attractors.len();
                self.audio.state.curl_attractors[..c_len]
                    .copy_from_slice(&curl_attractors[..c_len]);
                self.audio.state.attractors[..a_len].copy_from_slice(&attractors[..a_len]);

                // Update fractal state
                if let Some(omega) = kick_angular_velocity {
                    self.audio.state.local_angular_velocity = omega;
                }
                self.audio.state.reactive_bass = reactive_bass;
                self.audio.state.reactive_mids = reactive_mids;
                self.audio.state.reactive_high = reactive_high;
            }

            // No new data, continue on
            Err(crossbeam_channel::TryRecvError::Empty) => {}

            // Unexpected error, bail
            Err(e) => panic!("Failed to receive data from audio thread: {e:?}"),
        }
    }

    // Update the window and game state from keyboard inputs
    fn handle_keyboard_input(&mut self, keycode: VirtualKeyCode, control_flow: &mut ControlFlow) {
        match keycode {
            // Handle fullscreen toggle (F11)
            VirtualKeyCode::F11 => {
                if self.window_state.is_fullscreen {
                    self.engine.window().set_fullscreen(None);
                    self.window_state.is_fullscreen = false;
                } else {
                    self.engine
                        .window()
                        .set_fullscreen(Some(Fullscreen::Borderless(None)));
                    self.window_state.is_fullscreen = true;
                }
            }

            // Handle Escape key
            VirtualKeyCode::Escape => {
                if self.window_state.is_fullscreen {
                    // Leave fullscreen
                    self.engine.window().set_fullscreen(None);
                    self.window_state.is_fullscreen = false;
                } else {
                    // Exit window loop
                    println!("The Escape key was pressed, exiting");
                    *control_flow = ControlFlow::Exit;
                }
            }

            // Handle Space bar for toggling Kaleidoscope effect
            VirtualKeyCode::Space => {
                use KaleidoscopeDirection::{Backward, BackwardComplete, Forward, ForwardComplete};
                self.game_state.kaleidoscope_dir = match self.game_state.kaleidoscope_dir {
                    Forward | ForwardComplete => Backward,
                    Backward | BackwardComplete => Forward,
                }
            }

            // Handle toggling of Jello mode (i.e., fixing particles to positions)
            VirtualKeyCode::J => {
                self.game_state.fix_particles = match self.game_state.fix_particles {
                    ParticleTension::None => ParticleTension::Spring,
                    ParticleTension::Spring => ParticleTension::None,
                }
            }

            // Handle toggling of particle rendering.
            VirtualKeyCode::P => {
                // Toggle value stored in CPU memory.
                self.game_state.runtime_constants.render_particles =
                    !self.game_state.runtime_constants.render_particles;

                // Update value stored in GPU memory.
                self.engine
                    .runtime_constants_mut()
                    .write()
                    .unwrap()
                    .render_particles =
                    u32::from(self.game_state.runtime_constants.render_particles);
            }

            // Handle toggling of stationary particle visibility.
            VirtualKeyCode::H => {
                // Tell overlay to update the state.
                self.app_overlay
                    .toggle_hide_stationary_particles(&mut self.engine);
            }

            // Handle toggling of alternate colors
            VirtualKeyCode::Capital => {
                self.game_state.alternate_colors = match self.game_state.alternate_colors {
                    AlternateColors::Inverse => AlternateColors::Normal,
                    AlternateColors::Normal => AlternateColors::Inverse,
                }
            }

            // Handle toggling of 3D particles
            VirtualKeyCode::D => {
                self.game_state.particles_are_3d = !self.game_state.particles_are_3d;
            }

            // Tab through different color schemes / palattes ?
            VirtualKeyCode::Tab => {
                self.game_state.color_scheme_index =
                    (self.game_state.color_scheme_index + 1) % self.color_schemes.len();
                self.engine
                    .update_color_scheme(self.color_schemes[self.game_state.color_scheme_index]);
            }

            // Toggle display of config window
            VirtualKeyCode::C => self.app_overlay.toggle_config(),

            // Toggle display of help window
            VirtualKeyCode::F1 => self.app_overlay.toggle_help(),

            // Toggle audio-responsiveness
            VirtualKeyCode::R => {
                use cpal::traits::StreamTrait;
                self.game_state.audio_responsive = !self.game_state.audio_responsive;

                if self.game_state.audio_responsive {
                    self.audio.recreate_stream();
                } else {
                    // Ensure audio-state comes to a rest
                    self.audio.state.latest_volume = 0.;
                    self.audio.state.big_boomer = Vector4::default();
                    self.audio.state.curl_attractors = [Vector4::default(); 2];
                    self.audio.state.attractors = [Vector4::default(); 2];

                    // Pause audio stream
                    self.audio.capture_stream.pause().unwrap();
                }
            }

            // Handle toggling the companion-console.
            #[cfg(all(not(debug_assertions), target_os = "windows"))]
            VirtualKeyCode::Return => {
                if let Some(console_state) = &mut self.console_state {
                    if console_state.visible {
                        console_state.hide();
                    } else {
                        console_state.show();
                    };
                }
            }

            // Set different fractal types.
            VirtualKeyCode::Key0 => self.set_distance_estimate_id(0),
            VirtualKeyCode::Key1 => self.set_distance_estimate_id(1),
            VirtualKeyCode::Key2 => self.set_distance_estimate_id(2),
            VirtualKeyCode::Key3 => self.set_distance_estimate_id(3),
            VirtualKeyCode::Key4 => self.set_distance_estimate_id(4),
            VirtualKeyCode::Key5 => self.set_distance_estimate_id(5),
            VirtualKeyCode::Key6 => self.set_distance_estimate_id(6),

            // No-op
            _ => {}
        }
    }

    fn handle_window_event(&mut self, event: &WindowEvent, control_flow: &mut ControlFlow) {
        match *event {
            // Handle window close
            WindowEvent::CloseRequested => {
                println!("The close button was pressed, exiting");
                *control_flow = ControlFlow::Exit;
            }

            // Handle resize
            WindowEvent::Resized(_) => self.window_state.resized = true,

            // Handle some keyboard input
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(keycode),
                        ..
                    },
                ..
            } => self.handle_keyboard_input(keycode, control_flow),

            // Track window focus in a state var.
            WindowEvent::Focused(focused) => {
                if !focused {
                    // Force cursor visibility when focus is lost
                    self.engine.window().set_cursor_visible(true);
                    self.game_state.is_cursor_visible = true;
                }
                self.window_state.is_focused = focused;
            }

            // Handle mouse movement.
            WindowEvent::CursorMoved { position, .. } => {
                self.window_state.last_mouse_movement = Instant::now();
                self.engine.window().set_cursor_visible(true);
                self.game_state.is_cursor_visible = true;

                self.game_state.cursor_position = position;
            }

            // Handle mouse buttons to allow for cursor-applied forces.
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = match state {
                    ElementState::Pressed => 1.,
                    ElementState::Released => -1.,
                };
                self.game_state.cursor_force += pressed
                    * match button {
                        MouseButton::Left => -1.,
                        MouseButton::Right => 1.,
                        _ => 0.,
                    };

                // Allow users to fix any cursor-state issues by normalizing the magnitude when non-zero.
                let m = self.game_state.cursor_force.abs();
                if m > 1. {
                    self.game_state.cursor_force /= m;
                }
            }

            // Handle mouse scroll wheel to change strength of cursor-applied forces.
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                self.game_state.cursor_force_mult *= (SCROLL_SENSITIVITY * delta).exp();
            }

            _ => {}
        }
    }

    // Helper for interpolating data on a per-frame basis.
    fn interpolate_frames(&mut self, delta_time: f32) {
        // Interpolate the volume towards the latest.
        interpolate_floats(
            &mut self.audio.state.local_volume,
            self.audio.state.latest_volume,
            delta_time * -1.8,
        );

        // Use a volume-scaled delta-time to allow volume to control the speed of some actions.
        let audio_scaled_delta_time = delta_time * self.audio.state.local_volume.sqrt();
        self.audio.state.play_time += audio_scaled_delta_time;

        // Rotate the camera according to its angular velocity.
        self.game_state
            .camera_quaternion
            .rotate_by(Quaternion::build(
                self.audio.state.local_angular_velocity.xyz(),
                delta_time * self.audio.state.local_angular_velocity.w,
            ));

        // Interpolate the magnitude of the angular velocity towards the base value.
        interpolate_floats(
            &mut self.audio.state.local_angular_velocity.w,
            BASE_ANGULAR_VELOCITY,
            delta_time * -0.375,
        );

        // Interpolate the reactive vectors towards the latest.
        interpolate_vec3(
            &mut self.audio.state.local_reactive_bass,
            &self.audio.state.reactive_bass,
            delta_time * (0.8 * self.audio.state.big_boomer.w.sqrt()).min(1.) * -0.36,
        );
        interpolate_vec3(
            &mut self.audio.state.local_reactive_mids,
            &self.audio.state.reactive_mids,
            delta_time * (0.8 * self.audio.state.curl_attractors[0].w.sqrt()).min(1.) * -0.36,
        );
        interpolate_vec3(
            &mut self.audio.state.local_reactive_high,
            &self.audio.state.reactive_high,
            delta_time * (0.8 * self.audio.state.attractors[0].w.sqrt()).min(1.) * -0.36,
        );

        // Interpolate the smooth vectors towards the reactive vectors.
        interpolate_vec3(
            &mut self.audio.state.local_smooth_bass,
            &self.audio.state.local_reactive_bass,
            delta_time * -0.15,
        );
        interpolate_vec3(
            &mut self.audio.state.local_smooth_mids,
            &self.audio.state.local_reactive_mids,
            delta_time * -0.15,
        );
        interpolate_vec3(
            &mut self.audio.state.local_smooth_high,
            &self.audio.state.local_reactive_high,
            delta_time * -0.15,
        );

        // Check, and possibly update, the kaleidoscope animation state.
        match self.game_state.kaleidoscope_dir {
            KaleidoscopeDirection::Forward => {
                self.game_state.kaleidoscope += KALEIDOSCOPE_SPEED * audio_scaled_delta_time;
                if self.game_state.kaleidoscope >= 1. {
                    self.game_state.kaleidoscope = 1.;
                    self.game_state.kaleidoscope_dir = KaleidoscopeDirection::ForwardComplete;
                }
            }
            KaleidoscopeDirection::Backward => {
                self.game_state.kaleidoscope -= KALEIDOSCOPE_SPEED * audio_scaled_delta_time;
                if self.game_state.kaleidoscope <= 0. {
                    self.game_state.kaleidoscope = 0.;
                    self.game_state.kaleidoscope_dir = KaleidoscopeDirection::BackwardComplete;
                }
            }
            _ => {}
        };
    }

    // Create the push-constant data for the respective shaders from the current game state.
    #[allow(clippy::cast_precision_loss)]
    fn next_shader_data(&self, delta_time: f32, dimensions: PhysicalSize<u32>) -> DrawData {
        let width = dimensions.width as f32;
        let height = dimensions.height as f32;
        let aspect_ratio = width / height;

        // Create per-frame data for the particle compute-shader.
        let particle_data = if self.game_state.runtime_constants.render_particles {
            // Create a unique attractor based on the mouse position.
            let cursor_attractor = {
                let strength = if self.game_state.fix_particles == ParticleTension::Spring {
                    CURSOR_FIXED_STRENGTH
                } else {
                    CURSOR_LOOSE_STRENGTH
                } * self.game_state.cursor_force_mult
                    * self.game_state.cursor_force;

                let Vector3 { x, y, z, .. } =
                    self.screen_position_to_world(dimensions, aspect_ratio);
                [x, y, z, strength]
            };

            let compute = engine::ParticleComputePushConstants {
                big_boomer: self.audio.state.big_boomer.into(),

                curl_attractors: self
                    .audio
                    .state
                    .curl_attractors
                    .map(std::convert::Into::into),

                attractors: [
                    self.audio.state.attractors[0].into(),
                    self.audio.state.attractors[1].into(),
                    cursor_attractor,
                ],

                time: self.audio.state.play_time,
                delta_time,
                width,
                height,
                fix_particles: u32::from(self.game_state.fix_particles == ParticleTension::Spring),
                use_third_dimension: u32::from(self.game_state.particles_are_3d),
            };

            let vertex = engine::ParticleVertexPushConstants {
                quaternion: self.game_state.camera_quaternion.inv().into(),
                time: self.audio.state.play_time,
                alternate_colors: match self.game_state.alternate_colors {
                    AlternateColors::Inverse => 1,
                    AlternateColors::Normal => 0,
                },
                use_third_dimension: u32::from(self.game_state.particles_are_3d),
            };

            Some((compute, vertex))
        } else {
            None
        };

        // Create fractal data.
        let fractal_data = engine::FractalPushConstants {
            quaternion: self.game_state.camera_quaternion.into(),

            reactive_bass: self.audio.state.local_reactive_bass.into(),
            reactive_mids: self.audio.state.local_reactive_mids.into(),
            reactive_high: self.audio.state.local_reactive_high.into(),

            smooth_bass: self.audio.state.local_smooth_bass.into(),
            smooth_mids: self.audio.state.local_smooth_mids.into(),
            smooth_high: self.audio.state.local_smooth_high.into(),

            time: self.audio.state.play_time,
            kaleidoscope: self.game_state.kaleidoscope.powf(0.65),
            orbit_distance: if self.game_state.runtime_constants.render_particles
                && self.game_state.particles_are_3d
            {
                1.385
            } else {
                1.
            },
        };

        DrawData {
            particle_data,
            fractal_data,
        }
    }

    // Use game state to correctly map positions from screen space to world.
    fn screen_position_to_world(
        &self,
        dimensions: PhysicalSize<u32>,
        aspect_ratio: f32,
    ) -> Vector3 {
        #[allow(clippy::cast_lossless)]
        #[allow(clippy::cast_possible_truncation)]
        fn normalize_cursor(p: f64, max: u32) -> f32 {
            (2. * (p / max as f64) - 1.) as f32
        }
        let x_norm = normalize_cursor(self.game_state.cursor_position.x, dimensions.width);
        let y_norm = normalize_cursor(self.game_state.cursor_position.y, dimensions.height);

        if self.game_state.particles_are_3d && self.game_state.cursor_force != 0. {
            const PARTICLE_CAMERA_ORBIT: Vector3 = Vector3::new(0., 0., 1.75); // Keep in sync with orbit of `particles.vert`.
            const PERSPECTIVE_DISTANCE: f32 = 1.35;
            let fov_y = self
                .engine
                .app_constants()
                .read()
                .unwrap()
                .vertical_fov
                .tan();
            let fov_x = fov_y * aspect_ratio;

            // Map cursor to 3D world using camera orientation.
            let mut v = self.game_state.camera_quaternion.rotate_point(
                PERSPECTIVE_DISTANCE * Vector3::new(x_norm * fov_x, y_norm * fov_y, -1.),
            );
            v += self
                .game_state
                .camera_quaternion
                .rotate_point(PARTICLE_CAMERA_ORBIT);
            Vector3::new(v.x, v.y, v.z)
        } else {
            Vector3::new(x_norm, y_norm, 0.)
        }
    }

    // Helper to set a new distance estimator ID on CPU and GPU memory.
    fn set_distance_estimate_id(&mut self, id: u32) {
        self.game_state.runtime_constants.distance_estimator_id = id;
        self.engine
            .runtime_constants_mut()
            .write()
            .unwrap()
            .distance_estimator_id = id;
    }
}

impl Default for LocalAudioState {
    // Provide default audio state values.
    fn default() -> Self {
        Self {
            play_time: 0.,
            latest_volume: 0.,

            big_boomer: Vector4::default(),
            curl_attractors: [Vector4::default(); 2],
            attractors: [Vector4::default(); 2],

            // 3D (Fractals).
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
    // Provide default game state values.
    fn default() -> Self {
        Self {
            fix_particles: ParticleTension::Spring,
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
            audio_responsive: true,
            runtime_constants: RuntimeConstants::default(),
        }
    }
}

impl Default for RuntimeConstants {
    // Provide default values for runtime constants.
    fn default() -> Self {
        Self {
            render_particles: true,
            distance_estimator_id: 4,
        }
    }
}

impl RuntimeConstants {
    // Convert from CPU-side runtime constants to GPU-side runtime constants.
    #[must_use]
    pub fn to_engine_constants(&self, aspect_ratio: f32) -> engine::RuntimeConstants {
        engine::RuntimeConstants {
            aspect_ratio,
            render_particles: u32::from(self.render_particles),
            distance_estimator_id: self.distance_estimator_id,
        }
    }
}

const MAX_MESSAGE_BUFFER_COUNT: usize = 4;
impl AudioManager {
    // Create a default audio input stream and begin processing.
    pub fn default() -> Self {
        let (tx, receiver) = crossbeam_channel::bounded(MAX_MESSAGE_BUFFER_COUNT);
        Self {
            receiver,
            capture_stream: audio::process_loopback_audio_and_send(tx),
            state: LocalAudioState::default(),
        }
    }

    // Recreate the audio input stream.
    pub fn recreate_stream(&mut self) {
        let (tx, receiver) = crossbeam_channel::bounded(MAX_MESSAGE_BUFFER_COUNT);
        self.receiver = receiver;
        self.capture_stream = audio::process_loopback_audio_and_send(tx);
    }
}
