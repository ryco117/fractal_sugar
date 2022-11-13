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

use egui::{ScrollArea, Slider, Ui};
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

use crate::app_config::{AppConfig, Scheme};
use crate::engine::core::{select_hardware, AcquiredImageData, EngineSwapchain, WindowSurface};
use crate::engine::{AppConstants, Engine};

#[derive(Clone, Copy)]
struct ConfigUiScheme {
    pub index_rgb: [[f32; 3]; 4],
    pub index_val: [f32; 4],
    pub speed_rgb: [[f32; 3]; 4],
    pub speed_val: [f32; 4],
}

pub struct ConfigWindow {
    colors: Vec<ConfigUiScheme>,
    framebuffers: Vec<Arc<ImageView<SwapchainImage>>>,
    gui: Gui,
    id: WindowId,
    initial_colors: Vec<ConfigUiScheme>,
    initial_state: AppConstants,
    state: AppConstants,
    surface: Arc<Surface>,
    swapchain: EngineSwapchain,
    queue: Arc<Queue>,
    visible: bool,
}
// ui_schemes: &mut [ConfigUiScheme],
// init_colors: &[ConfigUiScheme],
// 
// state: &mut AppConstants,
// init_state: &AppConstants,
// 
// color_scheme_names: &[String],
// color_schemes: &mut [Scheme],
// displayed_scheme_index: usize,

const DEFAULT_VISIBILITY: bool = false;
const CONFIG_WINDOW_SIZE: [u32; 2] = [400, 600];

fn add_color_scheme(ui: &mut Ui, name: &String, scheme: &mut ConfigUiScheme) {
    ui.heading(name);
    egui::Grid::new("colors").show(ui, |ui| {
        ui.label("Index: ");
        for i in 0..scheme.index_rgb.len() {
            ui.color_edit_button_rgb(&mut scheme.index_rgb[i]);
            ui.add(Slider::new(&mut scheme.index_val[i], 0.0..=1.));
        }
        ui.end_row();

        ui.label("Speed: ");
        for i in 0..scheme.speed_rgb.len() {
            ui.color_edit_button_rgb(&mut scheme.speed_rgb[i]);
            ui.add(Slider::new(&mut scheme.speed_val[i], 0.0..=10.));
        }
        ui.end_row();
    });
    ui.separator();
}

// Define the layout and behavior of the config UI
fn create_ui(
    gui: &mut Gui,
    ui_schemes: &mut [ConfigUiScheme],
    init_colors: &[ConfigUiScheme],
    state: &mut AppConstants,
    init_state: &AppConstants,
    engine: &mut Engine,
    color_scheme_names: &[String],
    color_schemes: &mut [Scheme],
    displayed_scheme_index: usize,
) {
    let ctx = gui.context();
    egui::TopBottomPanel::bottom("bottom_panel").show(&ctx, |ui| {
        ui.horizontal_top(|ui| {
            // Allow user to reset back to values used at creation
            if ui
                .button("Reset")
                .on_hover_text("Reset displayed values to the constants used at launch.")
                .clicked()
            {
                *state = *init_state;
                ui_schemes.copy_from_slice(init_colors);
            }

            // Apply the values on screen to the GPU
            if ui
                .button("Apply")
                .on_hover_text("Apply displayed values to the scene.")
                .clicked()
            {
                let constants = constants_from_presentable(*state);
                engine.update_app_constants(constants);

                let new_colors: Vec<Scheme> = ui_schemes.iter().map(|us| (*us).into()).collect();
                color_schemes.copy_from_slice(&new_colors);
                engine.update_color_scheme(color_schemes[displayed_scheme_index]);
            }
        });
    });

    egui::CentralPanel::default().show(&ctx, |ui| {
        ui.heading("App Config");
        ui.separator();
        ScrollArea::both().max_height(350.).show(ui, |ui| {
            for (i, scheme) in ui_schemes.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    add_color_scheme(
                        ui,
                        color_scheme_names.get(i).unwrap_or(&String::default()),
                        scheme,
                    );
                });
            }
        });
        ui.separator();
        ui.add(Slider::new(&mut state.audio_scale, -30.0..=5.).text("audio scale (dB)"));
        ui.add(Slider::new(&mut state.max_speed, 0.0..=10.).text("max speed"));
        ui.add(Slider::new(&mut state.point_size, 0.0..=8.).text("point size"));
        ui.add(Slider::new(&mut state.friction_scale, 0.0..=5.).text("friction scale"));
        ui.add(Slider::new(&mut state.spring_coefficient, 0.0..=200.).text("spring coefficient"));
        ui.add(Slider::new(&mut state.vertical_fov, 30.0..=105.).text("vertical fov"));
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
        let initial_colors: Vec<ConfigUiScheme> = app_config
            .color_schemes
            .iter()
            .map(|cs| (*cs).into())
            .collect();

        Self {
            colors: initial_colors.clone(),
            framebuffers,
            gui,
            id: surface.window().id(),
            initial_colors,
            initial_state,
            state: initial_state,
            surface,
            swapchain,
            queue,
            visible: DEFAULT_VISIBILITY,
        }
    }

    pub fn handle_input(&mut self, event: &WindowEvent) {
        // Handle UI events
        self.gui.update(event);

        // Ensure to handle the 'close' event
        if event == &WindowEvent::CloseRequested {
            self.window().set_visible(false);
            self.visible = false;
        }
    }

    // Draw config UI to window
    pub fn draw(
        &mut self,
        engine: &mut Engine,
        color_scheme_names: &[String],
        color_schemes: &mut [Scheme],
        displayed_scheme_index: usize,
    ) {
        // Quick escape the render if window is not visible
        if !self.visible {
            return;
        }

        // Setup UI layout
        self.gui.immediate_ui(|gui| {
            create_ui(
                gui,
                &mut self.colors,
                &self.initial_colors,
                &mut self.state,
                &self.initial_state,
                engine,
                color_scheme_names,
                color_schemes,
                displayed_scheme_index,
            );
        });

        // Acquire next frame for rendering
        let AcquiredImageData {
            acquire_future,
            image_index,
            ..
        } = match self.swapchain.acquire_next_image() {
            Ok(data) => data,
            Err(e) => panic!("Failed to acquire next image: {:?}", e),
        };

        // Draw commands
        let future = self.gui.draw_on_image(
            acquire_future,
            self.framebuffers[image_index as usize].clone(),
        );
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

impl From<Scheme> for ConfigUiScheme {
    fn from(scheme: Scheme) -> Self {
        fn unzip(a: [[f32; 4]; 4]) -> ([[f32; 3]; 4], [f32; 4]) {
            (a.map(|a| [a[0], a[1], a[2]]), a.map(|a| a[3]))
        }

        let (index_rgb, index_val) = unzip(scheme.index);
        let (speed_rgb, speed_val) = unzip(scheme.speed);
        Self {
            index_rgb,
            index_val,
            speed_rgb,
            speed_val,
        }
    }
}

impl From<ConfigUiScheme> for Scheme {
    fn from(ui_scheme: ConfigUiScheme) -> Self {
        fn zip(a: [[f32; 3]; 4], b: [f32; 4]) -> [[f32; 4]; 4] {
            fn append(a: [f32; 3], b: f32) -> [f32; 4] {
                [a[0], a[1], a[2], b]
            }
            [
                append(a[0], b[0]),
                append(a[1], b[1]),
                append(a[2], b[2]),
                append(a[3], b[3]),
            ]
        }

        let index = zip(ui_scheme.index_rgb, ui_scheme.index_val);
        let speed = zip(ui_scheme.speed_rgb, ui_scheme.speed_val);
        Self { index, speed }
    }
}
