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

use std::sync::Arc;

use egui::Slider;
use egui_winit_vulkano::Gui;
use vulkano::device::Queue;
use vulkano::image::view::ImageView;
use vulkano::image::SwapchainImage;
use vulkano::instance::Instance;
use vulkano::swapchain::{PresentMode, Surface};
use winit::window::WindowId;
use winit::{
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

use crate::app_config::AppConfig;
use crate::engine::core::{select_hardware, AcquiredImageData, EngineSwapchain};
use crate::engine::{AppConstants, Engine};

pub struct ConfigWindow {
    framebuffers: Vec<Arc<ImageView<SwapchainImage<Window>>>>,
    gui: Gui,
    id: WindowId,
    initial_state: AppConstants,
    state: AppConstants,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    queue: Arc<Queue>,
    visible: bool,
}

const DEFAULT_VISIBILITY: bool = false;
const CONFIG_WINDOW_SIZE: [u32; 2] = [400, 220];

// Define the layout and behavior of the config UI
fn create_ui(
    gui: &mut Gui,
    state: &mut AppConstants,
    init_state: &AppConstants,
    engine: &mut Engine,
) {
    let ctx = gui.context();
    egui::CentralPanel::default().show(&ctx, |ui| {
        ui.heading("App Config");
        ui.separator();
        ui.add(Slider::new(&mut state.audio_scale, -30.0..=5.).text("audio scale (dB)"));
        ui.add(Slider::new(&mut state.max_speed, 0.0..=10.).text("max speed"));
        ui.add(Slider::new(&mut state.point_size, 0.0..=8.).text("point size"));
        ui.add(Slider::new(&mut state.friction_scale, 0.0..=6.).text("friction scale"));
        ui.add(Slider::new(&mut state.spring_coefficient, 0.0..=200.).text("spring coefficient"));
        ui.add(Slider::new(&mut state.vertical_fov, 30.0..=105.).text("vertical fov"));
        ui.separator();
        ui.horizontal_top(|ui| {
            // Allow user to reset back to values used at creation
            if ui
                .button("Reset")
                .on_hover_text("Reset displayed values to the constants used at launch.")
                .clicked()
            {
                *state = *init_state;
            }

            // Apply the values on screen to the GPU
            if ui
                .button("Apply")
                .on_hover_text("Apply displayed values to the scene.")
                .clicked()
            {
                let constants = constants_from_presentable(*state);
                engine.update_app_constants(constants);
            }
        });
    });
}

impl ConfigWindow {
    pub fn new(
        instance: &Arc<Instance>,
        event_loop: &EventLoop<()>,
        app_config: &AppConfig,
    ) -> Self {
        use vulkano_win::VkSurfaceBuild;
        let surface = WindowBuilder::new()
            .with_title("app config")
            .with_resizable(false)
            .with_inner_size(LogicalSize::<u32>::from(CONFIG_WINDOW_SIZE))
            .with_visible(DEFAULT_VISIBILITY)
            .build_vk_surface(event_loop, instance.clone())
            .unwrap();

        let (physical_device, device, queue) = select_hardware(instance, &surface);

        let swapchain =
            EngineSwapchain::new(&physical_device, device, surface.clone(), PresentMode::Fifo);

        let framebuffers = swapchain
            .images()
            .iter()
            .map(|img| ImageView::new_default(img.clone()).unwrap())
            .collect();

        let gui = Gui::new(
            event_loop,
            surface.clone(),
            Some(swapchain.image_format()),
            queue.clone(),
            false,
        );

        let initial_state = constants_to_presentable(app_config.into());
        Self {
            framebuffers,
            gui,
            id: surface.window().id(),
            initial_state,
            state: initial_state,
            surface,
            swapchain,
            queue,
            visible: DEFAULT_VISIBILITY,
        }
    }

    pub fn handle_input(&mut self, event: &WindowEvent) {
        // Handle events and request update next draw
        self.gui.update(event);
        self.window().request_redraw();

        // Ensure to handle the 'close' event
        if event == &WindowEvent::CloseRequested {
            self.window().set_visible(false);
            self.visible = false;
        }
    }

    // Draw config UI to window
    pub fn draw(&mut self, engine: &mut Engine) {
        // Quick escape the render if window is not visible
        if !self.visible {
            return;
        }

        // Acquire next frame for rendering
        let AcquiredImageData {
            acquire_future,
            image_index,
            ..
        } = match self.swapchain.acquire_next_image() {
            Ok(data) => data,
            Err(e) => panic!("Failed to acquire next image: {:?}", e),
        };

        // Setup UI layout
        self.gui.immediate_ui(|gui| {
            create_ui(gui, &mut self.state, &self.initial_state, engine);
        });

        // Draw commands
        let future = self
            .gui
            .draw_on_image(acquire_future, self.framebuffers[image_index].clone());
        self.swapchain.present(self.queue.clone(), future);
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
        self.window().set_visible(self.visible);
    }

    // Getters
    pub fn id(&self) -> WindowId {
        self.id
    }
    pub fn window(&self) -> &Window {
        self.surface.window()
    }
}

const DECIBEL_SCALE: f32 = std::f32::consts::LN_10 / 10.;

// Helpers for converting between presentation and internal units of measure
fn constants_to_presentable(app_constants: AppConstants) -> AppConstants {
    let AppConstants {
        max_speed,
        particle_count,
        spring_coefficient,
        point_size,
        friction_scale,
        audio_scale,
        vertical_fov,
    } = app_constants;
    AppConstants {
        max_speed,
        particle_count,
        spring_coefficient,
        point_size,
        friction_scale,
        audio_scale: audio_scale.ln() / DECIBEL_SCALE,
        vertical_fov: vertical_fov * 360. / std::f32::consts::PI,
    }
}
fn constants_from_presentable(app_constants: AppConstants) -> AppConstants {
    let AppConstants {
        max_speed,
        particle_count,
        spring_coefficient,
        point_size,
        friction_scale,
        audio_scale,
        vertical_fov,
    } = app_constants;
    AppConstants {
        max_speed,
        particle_count,
        spring_coefficient,
        point_size,
        friction_scale,
        audio_scale: (DECIBEL_SCALE * audio_scale).exp(),
        vertical_fov: vertical_fov * std::f32::consts::PI / 360.,
    }
}
