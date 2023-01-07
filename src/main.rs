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

// Ensure Windows builds are not console apps
#![windows_subsystem = "windows"]
// TODO: Remove file-wide allow statements
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use std::time::SystemTime;

use config_window::ConfigWindow;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent,
};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Fullscreen, WindowId};

use engine::core::{RecreateSwapchainResult, WindowSurface};
use engine::{DrawData, Engine};

mod app_config;
mod audio;
mod config_window;
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

#[allow(clippy::struct_excessive_bools)]
struct GameState {
    pub fix_particles: ParticleTension,
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
    pub audio_responsive: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct WindowState {
    pub resized: bool,
    pub recreate_swapchain: bool,
    pub is_fullscreen: bool,
    pub is_focused: bool,
    pub last_frame_time: SystemTime,
    pub last_mouse_movement: SystemTime,
    pub window_id: WindowId,
}

#[cfg(target_os = "windows")]
struct ConsoleState {
    pub handle: windows::Win32::Foundation::HWND,
    pub visible: bool,
}

struct FractalSugar {
    color_schemes: Vec<Scheme>,
    color_scheme_names: Vec<String>,

    config_window: ConfigWindow,
    engine: Engine,
    event_loop: Option<EventLoop<()>>,
    audio_receiver: crossbeam_channel::Receiver<audio::State>,

    #[cfg(target_os = "windows")]
    console_state: Option<ConsoleState>,

    // This field keeps the audio stream alive for the duration of the app
    #[allow(dead_code)]
    capture_stream: cpal::Stream,

    audio_state: LocalAudioState,
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
        #[cfg(target_os = "windows")]
        let console_state = unsafe {
            use windows::Win32::{
                System::Console::{AllocConsole, GetConsoleWindow},
                UI::WindowsAndMessaging::IsWindowVisible,
            };
            const ALLOC_NEW_TERMINAL: bool = true;
            if ALLOC_NEW_TERMINAL && AllocConsole().into() {
                let handle = GetConsoleWindow();
                let mut state = ConsoleState {
                    handle,
                    visible: IsWindowVisible(handle).into(),
                };

                // If console window is hidden by default, do not toggle.
                if state.visible {
                    toggle_console_visibility(&mut state);
                }

                Some(state)
            } else {
                None
            }
        };

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
                    println!(
                        "Failed to process custom color schemes file `{}`: {:?}",
                        filepath, e
                    );
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

        // Use Engine helper to initialize Vulkan instance
        let engine = engine::Engine::new(&event_loop, &app_config, icon);

        // Capture reference to audio stream and use message passing to receive data
        let (tx, rx) = crossbeam_channel::bounded(4);
        let capture_stream = audio::process_loopback_audio_and_send(tx);

        // State vars
        engine.window().focus_window();
        let window_state = WindowState {
            is_fullscreen: app_config.launch_fullscreen,
            resized: false,
            recreate_swapchain: false,
            is_focused: true,
            last_frame_time: SystemTime::now(),
            last_mouse_movement: SystemTime::now(),
            window_id: engine.window().id(),
        };
        let audio_state = LocalAudioState::default();
        let game_state = GameState::default();

        let config_window = ConfigWindow::new(engine.instance(), &event_loop, &app_config);

        Self {
            color_schemes: app_config.color_schemes,
            color_scheme_names: app_config.color_scheme_names,
            config_window,
            engine,
            event_loop: Some(event_loop),
            audio_receiver: rx,
            capture_stream,
            audio_state,
            game_state,
            window_state,

            #[cfg(target_os = "windows")]
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
                // All UI events have been handled (ie., executes once per frame)
                Event::MainEventsCleared => {
                    self.tock_frame();

                    // Request config window is redrawn
                    self.config_window.window().request_redraw();
                }

                // Handle all window-interaction events
                Event::WindowEvent { event, window_id } => {
                    if window_id == self.window_state.window_id {
                        // Handle events to main window
                        self.handle_window_event(&event, control_flow);
                    } else if window_id == self.config_window.id() {
                        self.config_window.handle_input(&event);
                    }
                }

                // Handle drawing of config window
                Event::RedrawRequested(window_id) if window_id == self.config_window.id() => {
                    self.config_window.draw(
                        &mut self.engine,
                        &self.color_scheme_names,
                        &mut self.color_schemes,
                        &mut self.game_state.color_scheme_index,
                    );
                }

                // Catch-all
                _ => {}
            })
    }

    // Update per-frame state and draw to window
    fn tock_frame(&mut self) {
        // Handle per-frame timing
        let now = SystemTime::now();
        let delta_time = now
            .duration_since(self.window_state.last_frame_time)
            .unwrap_or_default()
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
            && self.window_state.is_focused
            && self
                .window_state
                .last_mouse_movement
                .elapsed()
                .unwrap_or_default()
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

        // Draw frame and return whether a swapchain recreation was deemed necessary
        let (future, suboptimal) = match self.engine.render(&draw_data) {
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
            match self.audio_receiver.try_recv() {
                Ok(_) | Err(crossbeam_channel::TryRecvError::Empty) => {}

                // Unexpected error, bail
                Err(e) => panic!("Failed to receive data from audio thread: {e:?}"),
            }
            return;
        }

        // Handle any changes to audio state
        match self.audio_receiver.try_recv() {
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
                self.audio_state.latest_volume = volume;

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
                        self.audio_state.big_boomer.x +=
                            smooth * (big_boomer.x - self.audio_state.big_boomer.x);
                        self.audio_state.big_boomer.y +=
                            smooth * (big_boomer.y - self.audio_state.big_boomer.y);
                        self.audio_state.big_boomer.z +=
                            smooth * (big_boomer.z - self.audio_state.big_boomer.z);
                        self.audio_state.big_boomer.w = big_boomer.w;
                    }
                    ParticleTension::None => self.audio_state.big_boomer = big_boomer,
                }

                // Update 2D (curl)attractors
                let c_len = curl_attractors.len();
                let a_len = attractors.len();
                self.audio_state.curl_attractors[..c_len]
                    .copy_from_slice(&curl_attractors[..c_len]);
                self.audio_state.attractors[..a_len].copy_from_slice(&attractors[..a_len]);

                // Update fractal state
                if let Some(omega) = kick_angular_velocity {
                    self.audio_state.local_angular_velocity = omega;
                }
                self.audio_state.reactive_bass = reactive_bass;
                self.audio_state.reactive_mids = reactive_mids;
                self.audio_state.reactive_high = reactive_high;
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
                    std::process::exit(0);
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

            // Handle toggling of particle rendering
            VirtualKeyCode::P => {
                self.game_state.render_particles = !self.game_state.render_particles;
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

            // Toggle display of GUI
            VirtualKeyCode::G => self.config_window.toggle_visibility(),

            // Toggle audio-responsiveness
            VirtualKeyCode::R => {
                self.game_state.audio_responsive = !self.game_state.audio_responsive;

                if !self.game_state.audio_responsive {
                    // Ensure audio-state comes to a rest
                    self.audio_state.latest_volume = 0.;
                    self.audio_state.big_boomer = Vector4::default();
                    self.audio_state.curl_attractors = [Vector4::default(); 2];
                    self.audio_state.attractors = [Vector4::default(); 2];
                }
            }

            // Handle toggling the debug-console.
            // NOTE: Does not successfully hide `Windows Terminal` based CMD prompts
            #[cfg(target_os = "windows")]
            VirtualKeyCode::Return => {
                if let Some(console_state) = &mut self.console_state {
                    toggle_console_visibility(console_state);
                }
            }

            // Set different fractal types
            VirtualKeyCode::Key0 => self.game_state.distance_estimator_id = 0,
            VirtualKeyCode::Key1 => self.game_state.distance_estimator_id = 1,
            VirtualKeyCode::Key2 => self.game_state.distance_estimator_id = 2,
            VirtualKeyCode::Key3 => self.game_state.distance_estimator_id = 3,
            VirtualKeyCode::Key4 => self.game_state.distance_estimator_id = 4,
            VirtualKeyCode::Key5 => self.game_state.distance_estimator_id = 5,

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
                std::process::exit(0);
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

            // Track window focus in a state var
            WindowEvent::Focused(focused) => {
                if !focused {
                    // Force cursor visibility when focus is lost
                    self.engine.window().set_cursor_visible(true);
                    self.game_state.is_cursor_visible = true;
                }
                self.window_state.is_focused = focused;
            }

            // Handle mouse movement
            WindowEvent::CursorMoved { position, .. } => {
                self.window_state.last_mouse_movement = SystemTime::now();
                self.engine.window().set_cursor_visible(true);
                self.game_state.is_cursor_visible = true;

                self.game_state.cursor_position = position;
            }

            // Handle mouse buttons to allow cursor-applied forces
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
            }

            // Handle mouse scroll wheel to change strength of cursor-applied forces
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

    // Helper for interpolating data on a per-frame basis
    fn interpolate_frames(&mut self, delta_time: f32) {
        interpolate_floats(
            &mut self.audio_state.local_volume,
            self.audio_state.latest_volume,
            delta_time * -1.8,
        );
        let audio_scaled_delta_time = delta_time * self.audio_state.local_volume.sqrt();
        self.audio_state.play_time += audio_scaled_delta_time;
        self.game_state
            .camera_quaternion
            .rotate_by(Quaternion::build(
                self.audio_state.local_angular_velocity.xyz(),
                delta_time * self.audio_state.local_angular_velocity.w,
            ));
        interpolate_floats(
            &mut self.audio_state.local_angular_velocity.w,
            BASE_ANGULAR_VELOCITY,
            delta_time * -0.375,
        );
        interpolate_vec3(
            &mut self.audio_state.local_reactive_bass,
            &self.audio_state.reactive_bass,
            delta_time * (0.8 * self.audio_state.big_boomer.w.sqrt()).min(1.) * -0.36,
        );
        interpolate_vec3(
            &mut self.audio_state.local_reactive_mids,
            &self.audio_state.reactive_mids,
            delta_time * (0.8 * self.audio_state.curl_attractors[0].w.sqrt()).min(1.) * -0.36,
        );
        interpolate_vec3(
            &mut self.audio_state.local_reactive_high,
            &self.audio_state.reactive_high,
            delta_time * (0.8 * self.audio_state.attractors[0].w.sqrt()).min(1.) * -0.36,
        );
        interpolate_vec3(
            &mut self.audio_state.local_smooth_bass,
            &self.audio_state.local_reactive_bass,
            delta_time * -0.15,
        );
        interpolate_vec3(
            &mut self.audio_state.local_smooth_mids,
            &self.audio_state.local_reactive_mids,
            delta_time * -0.15,
        );
        interpolate_vec3(
            &mut self.audio_state.local_smooth_high,
            &self.audio_state.local_reactive_high,
            delta_time * -0.15,
        );

        // Check and possibly update kaleidoscope animation state
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

    // Create
    fn next_shader_data(&self, delta_time: f32, dimensions: PhysicalSize<u32>) -> DrawData {
        let width = dimensions.width as f32;
        let height = dimensions.height as f32;
        let aspect_ratio = width / height;

        // Create per-frame data for particle compute-shader
        let particle_data = if self.game_state.render_particles {
            // Create a unique attractor based on mouse position
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
                big_boomer: self.audio_state.big_boomer.into(),

                curl_attractors: self
                    .audio_state
                    .curl_attractors
                    .map(std::convert::Into::into),

                attractors: [
                    self.audio_state.attractors[0].into(),
                    self.audio_state.attractors[1].into(),
                    cursor_attractor,
                ],

                time: self.audio_state.play_time,
                delta_time,
                width,
                height,
                fix_particles: u32::from(self.game_state.fix_particles == ParticleTension::Spring),
                use_third_dimension: u32::from(self.game_state.particles_are_3d),
            };

            let vertex = engine::ParticleVertexPushConstants {
                quaternion: self.game_state.camera_quaternion.into(),
                time: self.audio_state.play_time,
                aspect_ratio,
                rendering_fractal: u32::from(self.game_state.distance_estimator_id != 0),
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

        // Create fractal data
        let fractal_data = engine::FractalPushConstants {
            quaternion: self.game_state.camera_quaternion.into(),

            reactive_bass: self.audio_state.local_reactive_bass.into(),
            reactive_mids: self.audio_state.local_reactive_mids.into(),
            reactive_high: self.audio_state.local_reactive_high.into(),

            smooth_bass: self.audio_state.local_smooth_bass.into(),
            smooth_mids: self.audio_state.local_smooth_mids.into(),
            smooth_high: self.audio_state.local_smooth_high.into(),

            time: self.audio_state.play_time,
            aspect_ratio,
            kaleidoscope: self.game_state.kaleidoscope.powf(0.65),
            distance_estimator_id: self.game_state.distance_estimator_id,
            orbit_distance: if self.game_state.render_particles && self.game_state.particles_are_3d
            {
                1.42
            } else {
                1.
            },
            render_background: u32::from(!self.game_state.render_particles),
        };

        DrawData {
            particle_data,
            fractal_data,
        }
    }

    // Use game state to correctly map positions from screen space to world
    fn screen_position_to_world(
        &self,
        dimensions: PhysicalSize<u32>,
        aspect_ratio: f32,
    ) -> Vector3 {
        #[allow(clippy::cast_lossless)]
        fn normalize_cursor(p: f64, max: u32) -> f32 {
            (2. * (p / max as f64) - 1.) as f32
        }
        let x_norm = normalize_cursor(self.game_state.cursor_position.x, dimensions.width);
        let y_norm = normalize_cursor(self.game_state.cursor_position.y, dimensions.height);

        if self.game_state.particles_are_3d && self.game_state.cursor_force != 0. {
            const PARTICLE_CAMERA_ORBIT: Vector3 = Vector3::new(0., 0., 1.75); // Keep in sync with orbit of `particles.vert`
            const PERSPECTIVE_DISTANCE: f32 = 1.35;
            let fov_y = self.engine.app_constants().vertical_fov.tan();
            let fov_x = fov_y * aspect_ratio;

            // Map cursor to 3D world using camera orientation
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
}

#[cfg(target_os = "windows")]
fn toggle_console_visibility(console_state: &mut ConsoleState) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE};
        ShowWindow(
            console_state.handle,
            if console_state.visible {
                SW_HIDE
            } else {
                SW_SHOWNOACTIVATE
            },
        );
        console_state.visible = !console_state.visible;
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
            fix_particles: ParticleTension::Spring,
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
            audio_responsive: true,
        }
    }
}
