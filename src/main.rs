use std::sync::mpsc;
use std::time::SystemTime;

use winit::event::{Event, DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::window::Fullscreen;

use engine::swapchain::RecreateSwapchainResult;

mod audio;
mod engine;
mod my_math;
mod space_filling_curves;

use my_math::Vector2;
use audio::AudioState;

// App constants
const BASE_VOLUME: f32 = 1.;

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
    let mut window_is_focused = true; engine.get_surface().window().focus_window();
    let mut last_frame_time = SystemTime::now();
    let mut last_mouse_movement = SystemTime::now();
    let mut is_cursor_visible = true;

    // Audio state vars?
    let mut game_time: f32 = 0.;
    let mut audio_state = AudioState::default();

    // Game state vars?
    let mut fix_particles = true;

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
        } => {
            window_resized = true
        }

        // All UI events have been handled (ie., executes once per frame)
        Event::MainEventsCleared => {
            // Handle per-frame timing
            let now = SystemTime::now();
            let delta_time = now.duration_since(last_frame_time).unwrap().as_secs_f32();
            last_frame_time = now;

            // Closures for exponential value interpolation
            let interpolate_floats = |scale: f32, source: f32, target: f32| -> f32 {
                let smooth = 1. - (delta_time * -scale).exp();
                source + (target - source) * smooth
            };
            let interpolate_vec2 = |scale: f32, source: &mut Vector2, target: &Vector2| {
                let smooth = 1. - (delta_time * -scale).exp();
                *source += (*target - *source).scale(smooth)
            };

            // Handle any changes to audio state
            match rx.try_recv() {
                // Update audio state vars
                Ok(AudioState{volume, big_boomer, curl_attractors, attractors}) => {
                    // Update volume
                    audio_state.volume = interpolate_floats(16.0, audio_state.volume, volume);

                    // Update 2D big boomers
                    if fix_particles {
                        interpolate_vec2(5.*audio_state.big_boomer.1, &mut audio_state.big_boomer.0, &big_boomer.0);
                        audio_state.big_boomer.1 = big_boomer.1
                    } else {
                        audio_state.big_boomer = big_boomer
                    }
                    // Update 2D (curl)attractors
                    for i in 0..2 {
                        audio_state.curl_attractors[i] = curl_attractors[i];
                        audio_state.attractors[i] = attractors[i]
                    }
                }

                // No new data, interpolate towards baseline
                Err(mpsc::TryRecvError::Empty) => {
                    audio_state.volume = interpolate_floats(1.0, audio_state.volume, BASE_VOLUME)
                }

                // Unexpected error, bail
                Err(e) => panic!("Failed to receive data from audio thread: {:?}", e)
            }

            // Update state time
            game_time += delta_time * audio_state.volume.sqrt();

            // If cursor is visible and has been stationary then hide it
            if is_cursor_visible && window_is_focused && last_mouse_movement.elapsed().unwrap().as_secs_f32() > 3. {
                engine.get_surface().window().set_cursor_visible(false);
                is_cursor_visible = false
            }

            // Handle any necessary recreations (usually from window resizing)
            let dimensions = engine.get_surface().window().inner_size();
            if window_resized || recreate_swapchain {
                match engine.recreate_swapchain(dimensions, window_resized) {
                    RecreateSwapchainResult::Success => { recreate_swapchain = false; window_resized = false }
                    RecreateSwapchainResult::ExtentNotSupported => return,
                    RecreateSwapchainResult::Failure(err) => panic!("Failed to recreate swapchain: {:?}", err)
                }
            }

            // Create per-frame data
            let push_constants = engine::renderer::PushConstantData {
                temp_data: [0.; 4],
                time: game_time,
                width: dimensions.width as f32,
                height: dimensions.height as f32
            };

            // Unzip (point, strength) arrays for passing to shader
            fn simple_unzip(arr: &[(Vector2, f32); 2]) -> ([Vector2; 2], [f32; 2]) { (arr.map(|e| e.0), arr.map(|e| e.1)) }
            let (curl_attractors, curl_attractor_strengths) = simple_unzip(&audio_state.curl_attractors);
            let (attractors, attractor_strengths) = simple_unzip(&audio_state.attractors);

            // Create per-frame data for particle compute-shader
            let compute_push_constants = engine::renderer::ComputePushConstantData {
                big_boomer: audio_state.big_boomer.0,
                big_boomer_strength: audio_state.big_boomer.1,

                curl_attractors,
                curl_attractor_strengths,

                attractors,
                attractor_strengths,

                time: game_time,
                delta_time,
                fix_particles: if fix_particles {1} else {0}
            };

            // Draw frame and return whether a swapchain recreation was deemed necessary
            recreate_swapchain |= engine.draw_frame(push_constants, compute_push_constants)
        }

        // Handle some keyboard input
        Event::WindowEvent {
            event: WindowEvent::KeyboardInput {
                input: KeyboardInput {
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
                (ElementState::Pressed, VirtualKeyCode::F11) => if window_is_fullscreen {
                    engine.get_surface().window().set_fullscreen(None);
                    window_is_fullscreen = false
                } else {
                    engine.get_surface().window().set_fullscreen(Some(Fullscreen::Borderless(None)));
                    window_is_fullscreen = true
                }

                // Handle Escape key
                (ElementState::Pressed, VirtualKeyCode::Escape) => if window_is_fullscreen {
                    // Leave fullscreen
                    engine.get_surface().window().set_fullscreen(None);
                    window_is_fullscreen = false
                } else {
                    // Exit window loop
                    println!("The Escape key was pressed, exiting");
                    *control_flow = ControlFlow::Exit
                }

                // Handle toggling of Jello mode (fixing particles)
                (ElementState::Pressed, VirtualKeyCode::J) => {
                    fix_particles = !fix_particles;
                }

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
            event: DeviceEvent::MouseMotion {..},
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