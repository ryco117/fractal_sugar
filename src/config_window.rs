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

use std::ops::RangeInclusive;
use std::sync::Arc;

use egui::{ComboBox, ScrollArea, Slider, Ui};
use egui_winit_vulkano::Gui;
use vulkano::device::Queue;
use vulkano::image::ImageViewAbstract;
use vulkano::swapchain::{Surface, Swapchain};
use vulkano::sync::GpuFuture;
use winit::{event::WindowEvent, event_loop::EventLoop};

use crate::app_config::{AppConfig, Scheme};
use crate::engine::{AppConstants, Engine};

#[derive(Clone, Copy)]
struct ConfigUiScheme {
    pub index_rgb: [[u8; 3]; 4],
    pub index_val: [f32; 4],
    pub speed_rgb: [[u8; 3]; 4],
    pub speed_val: [f32; 4],
}

pub struct ConfigWindow {
    config_state: AppConfigState,
    gui: Gui,
    queue: Arc<Queue>,
    visible: bool,
}

struct AppConfigState {
    color_schemes: Vec<ConfigUiScheme>,
    init_color_schemes: Vec<ConfigUiScheme>,
    edit_scheme_index: usize,

    state: AppConstants,
    init_state: AppConstants,
}

const DEFAULT_VISIBILITY: bool = false;

fn add_scheme_element(
    ui: &mut Ui,
    rgb: &mut [[u8; 3]; 4],
    val: &mut [f32; 4],
    range: RangeInclusive<f32>,
) {
    let n = rgb.len();
    for i in 0..n - 1 {
        ui.color_edit_button_srgb(&mut rgb[i]);
        ui.add(Slider::new(&mut val[i], range.clone()));
        ui.end_row();
    }
    ui.color_edit_button_srgb(&mut rgb[n - 1]);
    ui.end_row();
}

fn add_color_scheme(
    ui: &mut Ui,
    scheme: &mut ConfigUiScheme,
    displayed_scheme_index: &mut usize,
    edit_scheme_index: usize,
    engine: &mut Engine,
) {
    // Enforce limits
    fn enforce_limits(vals: &mut [f32; 4]) {
        let mut max = 0.;
        for v in vals[0..3].iter_mut() {
            if *v < max {
                *v = max;
            }
            max = *v;
        }
    }
    enforce_limits(&mut scheme.index_val);
    enforce_limits(&mut scheme.speed_val);

    ScrollArea::vertical().max_height(350.).show(ui, |ui| {
        ui.heading("Index-Based Color Scheme");
        egui::Grid::new("scheme_index_grid").show(ui, |ui| {
            add_scheme_element(ui, &mut scheme.index_rgb, &mut scheme.index_val, 0.0..=1.);
        });
        ui.heading("Speed-Based Color Scheme");
        egui::Grid::new("scheme_speed_grid").show(ui, |ui| {
            add_scheme_element(ui, &mut scheme.speed_rgb, &mut scheme.speed_val, 0.0..=10.);
        });

        if edit_scheme_index != *displayed_scheme_index
            && ui.button("Make this color scheme active").clicked()
        {
            *displayed_scheme_index = edit_scheme_index;
            engine.update_color_scheme(scheme.into());
        }
    });
}

// Define the layout and behavior of the config UI
fn create_ui(
    gui: &mut Gui,
    config_state: &mut AppConfigState,
    engine: &mut Engine,
    color_scheme_names: &[String],
    color_schemes: &mut [Scheme],
    displayed_scheme_index: &mut usize,
    visible: &mut bool,
) {
    let ctx = gui.context();
    let name = "App Config";
    egui::Window::new(name)
        .open(visible)
        .resizable(true)
        .show(&ctx, |ui| {
            ComboBox::from_label("Active Color Scheme")
                .selected_text(color_scheme_names[config_state.edit_scheme_index].clone())
                .show_ui(ui, |ui| {
                    for (i, name) in color_scheme_names.iter().enumerate() {
                        ui.selectable_value(&mut config_state.edit_scheme_index, i, name.clone());
                    }
                });
            add_color_scheme(
                ui,
                &mut config_state.color_schemes[config_state.edit_scheme_index],
                displayed_scheme_index,
                config_state.edit_scheme_index,
                engine,
            );
            ui.separator();
            ui.add(
                Slider::new(&mut config_state.state.audio_scale, -30.0..=5.)
                    .text("audio scale (dB)"),
            );
            ui.add(Slider::new(&mut config_state.state.max_speed, 0.0..=10.).text("max speed"));
            ui.add(Slider::new(&mut config_state.state.point_size, 0.0..=8.).text("point size"));
            ui.add(
                Slider::new(&mut config_state.state.friction_scale, 0.0..=5.)
                    .text("friction scale"),
            );
            ui.add(
                Slider::new(&mut config_state.state.spring_coefficient, 0.0..=200.)
                    .text("spring coefficient"),
            );
            ui.add(
                Slider::new(&mut config_state.state.vertical_fov, 30.0..=105.).text("vertical fov"),
            );
            ui.separator();
            ui.horizontal(|ui| {
                // Allow user to reset back to values currently applied
                if ui
                    .button("Reset")
                    .on_hover_text("Reset displayed values to the constants used at launch.")
                    .clicked()
                {
                    config_state.state = config_state.init_state;
                    config_state
                        .color_schemes
                        .copy_from_slice(&config_state.init_color_schemes);
                }

                // Apply the values on screen to the GPU
                if ui
                    .button("Apply")
                    .on_hover_text("Apply displayed values to the scene.")
                    .clicked()
                {
                    let constants = constants_from_presentable(config_state.state);
                    engine.update_app_constants(constants);

                    let new_colors: Vec<_> = config_state
                        .color_schemes
                        .iter()
                        .map(|cs| (*cs).into())
                        .collect();
                    color_schemes.copy_from_slice(&new_colors);
                    engine.update_color_scheme(color_schemes[*displayed_scheme_index]);
                }
            });
        });
}

impl ConfigWindow {
    pub fn new(
        surface: Arc<Surface>,
        swapchain: &Arc<Swapchain>,
        queue: Arc<Queue>,
        event_loop: &EventLoop<()>,
        app_config: &AppConfig,
    ) -> Self {
        let gui = Gui::new(
            event_loop,
            surface,
            Some(swapchain.image_format()),
            queue.clone(),
            true,
        );

        let initial_state = constants_to_presentable(app_config.into());
        let initial_colors: Vec<ConfigUiScheme> = app_config
            .color_schemes
            .iter()
            .map(|cs| (*cs).into())
            .collect();

        let config_state = AppConfigState {
            color_schemes: initial_colors.clone(),
            init_color_schemes: initial_colors,
            edit_scheme_index: 0,
            state: initial_state,
            init_state: initial_state,
        };

        Self {
            config_state,
            gui,
            queue,
            visible: DEFAULT_VISIBILITY,
        }
    }

    pub fn handle_input(&mut self, event: &WindowEvent) -> bool {
        // Handle UI events
        self.gui.update(event)
    }

    // Draw config UI to window
    pub fn draw(
        &mut self,
        engine: &mut Engine,
        color_scheme_names: &[String],
        color_schemes: &mut [Scheme],
        displayed_scheme_index: &mut usize,
        frame: Arc<dyn ImageViewAbstract>,
        before_future: Box<dyn GpuFuture>,
    ) -> Box<dyn GpuFuture> {
        // Quick escape the render if window is not visible
        if !self.visible {
            return vulkano::sync::now(self.queue.device().clone()).boxed();
        }

        // Setup UI layout
        self.gui.immediate_ui(|gui| {
            create_ui(
                gui,
                &mut self.config_state,
                engine,
                color_scheme_names,
                color_schemes,
                displayed_scheme_index,
                &mut self.visible,
            );
        });

        self.gui.draw_on_image(before_future, frame)
    }

    pub fn toggle_overlay(&mut self) {
        self.visible = !self.visible;
    }
    pub fn overlay_visible(&self) -> bool {
        self.visible
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
        #[allow(clippy::cast_sign_loss)]
        fn convert(x: f32) -> u8 {
            (x * 255.) as u8
        }
        fn unzip(a: [[f32; 4]; 4]) -> ([[u8; 3]; 4], [f32; 4]) {
            (
                a.map(|a| [convert(a[0]), convert(a[1]), convert(a[2])]),
                a.map(|a| a[3]),
            )
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
        fn convert(i: u8) -> f32 {
            f32::from(i) / 255.
        }
        fn zip(a: &[[u8; 3]; 4], b: &[f32; 4]) -> [[f32; 4]; 4] {
            fn append(a: [u8; 3], b: f32) -> [f32; 4] {
                [convert(a[0]), convert(a[1]), convert(a[2]), b]
            }
            [
                append(a[0], b[0]),
                append(a[1], b[1]),
                append(a[2], b[2]),
                append(a[3], b[3]),
            ]
        }

        let index = zip(&ui_scheme.index_rgb, &ui_scheme.index_val);
        let speed = zip(&ui_scheme.speed_rgb, &ui_scheme.speed_val);
        Self { index, speed }
    }
}
impl From<&mut ConfigUiScheme> for Scheme {
    fn from(ui_scheme: &mut ConfigUiScheme) -> Self {
        fn convert(i: u8) -> f32 {
            f32::from(i) / 255.
        }
        fn zip(a: &[[u8; 3]; 4], b: &[f32; 4]) -> [[f32; 4]; 4] {
            fn append(a: [u8; 3], b: f32) -> [f32; 4] {
                [convert(a[0]), convert(a[1]), convert(a[2]), b]
            }
            [
                append(a[0], b[0]),
                append(a[1], b[1]),
                append(a[2], b[2]),
                append(a[3], b[3]),
            ]
        }

        let index = zip(&ui_scheme.index_rgb, &ui_scheme.index_val);
        let speed = zip(&ui_scheme.speed_rgb, &ui_scheme.speed_val);
        Self { index, speed }
    }
}
