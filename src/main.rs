use std::sync::mpsc;
use std::time::SystemTime;

use winit::event::{DeviceEvent, ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Fullscreen;

use engine::swapchain::RecreateSwapchainResult;

mod audio;
mod engine;
mod my_math;
mod space_filling_curves;

use audio::AudioState;
use my_math::{Quaternion, Vector2, Vector3, Vector4};

// App constants
const BASE_VOLUME: f32 = 0.5;
const BASE_ANGULAR_VELOCITY: f32 = 0.025;

fn main() {
    // First, create global event loop to manage window events
    let event_loop = EventLoop::new();

    // Use Engine helper to initialize Vulkan instance
    let mut engine = engine::Engine::new(&event_loop);

    // Capture reference to audio stream and use message passing to receive data
    let (tx, rx) = mpsc::channel();
    let _capture_stream_option = audio::create_default_loopback(tx);

    // Window state vars
    let mut window_resized = false;
    let mut recreate_swapchain = false;
    let mut window_is_fullscreen = false;
    let mut window_is_focused = true;
    engine.get_surface().window().focus_window();
    let mut last_frame_time = SystemTime::now();
    let mut last_mouse_movement = SystemTime::now();
    let mut is_cursor_visible = true;

    // Audio state vars?
    let mut game_time: f32 = 0.;
    let mut audio_state = AudioState::default();

    // Game state vars?
    let mut fix_particles = false;
    let mut render_particles = true;
    let mut distance_estimator_id = 1;
    let mut camera_quaternion = Quaternion::default();

    // Create local copies so they can be upated more frequently than FFT
    let mut local_angular_velocity = Vector4::new(0., 1., 0., 0.);
    let mut local_reactive_bass = Vector3::default();
    let mut local_reactive_mids = Vector3::default();
    let mut local_reactive_high = Vector3::default();
    let mut local_smooth_bass = Vector3::default();
    let mut local_smooth_mids = Vector3::default();
    let mut local_smooth_high = Vector3::default();

    // Run window loop
    println!("Begin window loop...");
    event_loop.run(move |event, _, control_flow| match event {
        // Handle window close
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            println!("The close button was pressed, exiting");
            *control_flow = ControlFlow::Exit
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

            // Closures for exponential value interpolation
            let interpolate_floats = |source: f32, target: f32, scale: f32| -> f32 {
                let smooth = 1. - (delta_time * -scale).exp();
                source + smooth * (target - source)
            };
            let interpolate_vec2 = |source: &mut Vector2, target: &Vector2, scale: f32| {
                let smooth = 1. - (delta_time * -scale).exp();
                *source += smooth * (*target - *source)
            };
            let interpolate_vec3 = |source: &mut Vector3, target: &Vector3, scale: f32| {
                let smooth = 1. - (delta_time * -scale).exp();
                *source += smooth * (*target - *source)
            };

            // Handle any changes to audio state
            match rx.try_recv() {
                // Update audio state vars
                Ok(AudioState {
                    volume,

                    big_boomer,
                    curl_attractors,
                    attractors,

                    reactive_bass,
                    reactive_mids,
                    reactive_high,

                    kick_angular_velocity,
                }) => {
                    // Update volume
                    audio_state.volume = interpolate_floats(4.0, audio_state.volume, volume);

                    // Update 2D big boomers
                    if fix_particles {
                        interpolate_vec2(
                            &mut audio_state.big_boomer.0,
                            &big_boomer.0,
                            7.5 * audio_state.big_boomer.1,
                        );
                        audio_state.big_boomer.1 = big_boomer.1
                    } else {
                        audio_state.big_boomer = big_boomer
                    }
                    // Update 2D (curl)attractors
                    for i in 0..2 {
                        audio_state.curl_attractors[i] = curl_attractors[i];
                        audio_state.attractors[i] = attractors[i]
                    }

                    // Update fractal state
                    if let Some(omega) = kick_angular_velocity {
                        local_angular_velocity = omega;
                    }
                    audio_state.reactive_bass = reactive_bass;
                    audio_state.reactive_mids = reactive_mids;
                    audio_state.reactive_high = reactive_high;
                }

                // No new data, interpolate towards baseline
                Err(mpsc::TryRecvError::Empty) => {
                    audio_state.volume = interpolate_floats(1.0, audio_state.volume, BASE_VOLUME)
                }

                // Unexpected error, bail
                Err(e) => panic!("Failed to receive data from audio thread: {:?}", e),
            }

            // Update per-frame state
            game_time += delta_time * audio_state.volume.sqrt();
            camera_quaternion.rotate_by(Quaternion::build(
                local_angular_velocity.xyz(),
                delta_time * local_angular_velocity.w,
            ));
            local_angular_velocity.w =
                interpolate_floats(local_angular_velocity.w, BASE_ANGULAR_VELOCITY, 0.25);
            interpolate_vec3(
                &mut local_reactive_bass,
                &audio_state.reactive_bass,
                (0.8 * audio_state.big_boomer.1.sqrt()).min(1.) * 0.325,
            );
            interpolate_vec3(
                &mut local_reactive_mids,
                &audio_state.reactive_mids,
                (0.8 * audio_state.curl_attractors[0].1.sqrt()).min(1.) * 0.325,
            );
            interpolate_vec3(
                &mut local_reactive_high,
                &audio_state.reactive_high,
                (0.8 * audio_state.attractors[0].1.sqrt()).min(1.) * 0.325,
            );
            interpolate_vec3(&mut local_smooth_bass, &local_reactive_bass, 0.12);
            interpolate_vec3(&mut local_smooth_mids, &local_reactive_mids, 0.12);
            interpolate_vec3(&mut local_smooth_high, &local_reactive_high, 0.12);

            // If cursor is visible and has been stationary then hide it
            if is_cursor_visible
                && window_is_focused
                && last_mouse_movement.elapsed().unwrap().as_secs_f32() > 3.
            {
                engine.get_surface().window().set_cursor_visible(false);
                is_cursor_visible = false
            }

            // Handle any necessary recreations (usually from window resizing)
            let dimensions = engine.get_surface().window().inner_size();
            if window_resized || recreate_swapchain {
                match engine.recreate_swapchain(dimensions, window_resized) {
                    RecreateSwapchainResult::Success => {
                        recreate_swapchain = false;
                        window_resized = false
                    }
                    RecreateSwapchainResult::ExtentNotSupported => return,
                }
            }

            // Unzip (point, strength) arrays for passing to shader
            fn simple_unzip(arr: &[(Vector2, f32); 2]) -> ([[f32; 2]; 2], [f32; 2]) {
                (arr.map(|e| e.0.into()), arr.map(|e| e.1))
            }
            let (curl_attractors, curl_attractor_strengths) =
                simple_unzip(&audio_state.curl_attractors);
            let (attractors, attractor_strengths) = simple_unzip(&audio_state.attractors);

            // Create per-frame data for particle compute-shader
            let compute_push_constants = engine::ComputePushConstants {
                big_boomer: audio_state.big_boomer.0.into(),
                big_boomer_strength: audio_state.big_boomer.1,

                curl_attractors,
                curl_attractor_strengths,

                attractors,
                attractor_strengths,

                time: game_time,
                delta_time,
                fix_particles: if fix_particles { 1 } else { 0 },
            };

            // Create fractal data
            let fractal_data = engine::FractalPushConstants {
                quaternion: camera_quaternion.into(),

                reactive_bass: local_reactive_bass.into(),
                reactive_mids: local_reactive_mids.into(),
                reactive_high: local_reactive_high.into(),

                smooth_bass: local_smooth_bass.into(),
                smooth_mids: local_smooth_mids.into(),
                smooth_high: local_smooth_high.into(),

                time: game_time,
                width: dimensions.width as f32,
                height: dimensions.height as f32,
                distance_estimator_id,
            };

            // Draw frame and return whether a swapchain recreation was deemed necessary
            recreate_swapchain |=
                engine.draw_frame(compute_push_constants, fractal_data, render_particles)
        }

        // Handle some keyboard input
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: pressed_state,
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    ..
                },
            ..
        } => {
            match (pressed_state, keycode) {
                // Handle fullscreen togle (F11)
                (ElementState::Pressed, VirtualKeyCode::F11) => {
                    if window_is_fullscreen {
                        engine.get_surface().window().set_fullscreen(None);
                        window_is_fullscreen = false
                    } else {
                        engine
                            .get_surface()
                            .window()
                            .set_fullscreen(Some(Fullscreen::Borderless(None)));
                        window_is_fullscreen = true
                    }
                }

                // Handle Escape key
                (ElementState::Pressed, VirtualKeyCode::Escape) => {
                    if window_is_fullscreen {
                        // Leave fullscreen
                        engine.get_surface().window().set_fullscreen(None);
                        window_is_fullscreen = false
                    } else {
                        // Exit window loop
                        println!("The Escape key was pressed, exiting");
                        *control_flow = ControlFlow::Exit
                    }
                }

                // Handle toggling of Jello mode (fixing particles)
                (ElementState::Pressed, VirtualKeyCode::J) => fix_particles = !fix_particles,

                // Handle toggling of particle rendering
                (ElementState::Pressed, VirtualKeyCode::P) => render_particles = !render_particles,

                // Set different fractal types
                (ElementState::Pressed, VirtualKeyCode::Key0) => distance_estimator_id = 0,
                (ElementState::Pressed, VirtualKeyCode::Key1) => distance_estimator_id = 1,
                (ElementState::Pressed, VirtualKeyCode::Key2) => distance_estimator_id = 2,
                (ElementState::Pressed, VirtualKeyCode::Key3) => distance_estimator_id = 3,
                (ElementState::Pressed, VirtualKeyCode::Key4) => distance_estimator_id = 4,
                (ElementState::Pressed, VirtualKeyCode::Key5) => distance_estimator_id = 5,

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
                engine.get_surface().window().set_cursor_visible(true);
                is_cursor_visible = true
            }
            window_is_focused = focused
        }

        // Handle mouse movement
        Event::DeviceEvent {
            event: DeviceEvent::MouseMotion { .. },
            ..
        } => {
            last_mouse_movement = SystemTime::now();
            engine.get_surface().window().set_cursor_visible(true);
            is_cursor_visible = true
        }

        // Catch-all
        _ => {}
    })
}
