use std::sync::mpsc;
use std::time::Instant;

use winit::event::{Event, DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::window::Fullscreen;

use engine::swapchain::RecreateSwapchainResult;

mod audio;
mod engine;

use audio::AudioState;

// App constants
const BASE_VOLUME: f32 = 1.;

fn main() {
    // First, create global event loop to manage window events
    let event_loop = EventLoop::new();

    // Use Engine helper to initialize Vulkan instance
    let mut engine = engine::Engine::new(&event_loop);

    // Window state vars
    let mut window_resized = false;
    let mut recreate_swapchain = false;
    let mut window_is_fullscreen = false;
    let mut last_frame_time = Instant::now();
    let mut last_mouse_movement = Instant::now();
    let mut is_cursor_visible = true;

    // Audio state vars?
    let mut game_time: f32 = 0.;
    let mut audio_state = AudioState::default();

    // Capture reference to audio stream and use message passing to receive data
    let (tx, rx) = mpsc::channel();
    let _capture_stream_option = audio::create_default_loopback(tx);

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
            let now = Instant::now();
            let delta_time = (now - last_frame_time).as_secs_f32();
            last_frame_time = now;

            // Closure for exponential value interpolation between floats
            let interpolate_floats = |scale: f32, source: f32, target: f32| -> f32 {
                let smooth = 1. - (delta_time * -scale).exp();
                source + (target - source) * smooth
            };

            match rx.try_recv() {
                // Update audio state vars
                Ok(AudioState {volume, ..}) => {
                    audio_state.volume = interpolate_floats(32.0, audio_state.volume, volume)
                }

                // No new data, interpolate towards baseline
                Err(mpsc::TryRecvError::Empty) => {
                    audio_state.volume = interpolate_floats(1.0, audio_state.volume, BASE_VOLUME)
                }

                // Unexpected error, bail
                Err(e) => panic!("Failed to receive data from audio thread: {:?}", e)
            }

            game_time += delta_time * audio_state.volume.sqrt();

            if last_mouse_movement.elapsed().as_secs_f32() > 3. && is_cursor_visible {
                engine.get_surface().window().set_cursor_visible(false);
                is_cursor_visible = false
            }

            let dimensions = engine.get_surface().window().inner_size();

            // Handle possible structure recreations necessary (usually from window resizing)
            if window_resized || recreate_swapchain {
                match engine.recreate_swapchain(dimensions, window_resized) {
                    RecreateSwapchainResult::Success => {recreate_swapchain = false; window_resized = false}
                    RecreateSwapchainResult::ExtentNotSupported => return,
                    RecreateSwapchainResult::Failure(err) => panic!("Failed to recreate swapchain: {:?}", err)
                }
            }

            // Create per-frame data
            let push_constants = engine::renderer::PushConstantData {
                time: game_time,
                width: dimensions.width as f32,
                height: dimensions.height as f32
            };

            // Draw frame and return whether a swapchain recreation was deemed necessary
            recreate_swapchain |= engine.draw_frame(push_constants)
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
                _ => {}
            }
        }

        // Handle mouse movement
        Event::DeviceEvent {
            event: DeviceEvent::MouseMotion {..},
            ..
        } => {
            last_mouse_movement = Instant::now();
            engine.get_surface().window().set_cursor_visible(true);
            is_cursor_visible = true
        }

        // Catch-all
        _ => {}
    })
}