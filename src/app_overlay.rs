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

use std::ops::RangeInclusive;
use std::sync::Arc;

use egui::{ComboBox, ScrollArea, Slider, Ui};
use egui_winit_vulkano::{Gui, GuiConfig};
use vulkano::command_buffer::SecondaryAutoCommandBuffer;
use vulkano::device::Queue;
use vulkano::render_pass::Subpass;
use vulkano::swapchain::{Surface, Swapchain};
use winit::{event::WindowEvent, event_loop::EventLoop};

use crate::app_config::{AppConfig, Scheme};
use crate::engine::{ConfigConstants, Engine};

#[derive(Clone, Copy)]
struct ConfigUiScheme {
    pub index_rgb: [[u8; 3]; 4],
    pub index_val: [f32; 4],
    pub speed_rgb: [[u8; 3]; 4],
    pub speed_val: [f32; 4],
}

pub struct AppOverlay {
    config_window: ConfigWindow,
    gui: Gui,
    help_visible: bool,
}

struct ConfigWindow {
    color_schemes: Vec<ConfigUiScheme>,
    init_color_schemes: Vec<ConfigUiScheme>,
    edit_scheme_index: usize,

    config: ConfigConstants,
    init_config: ConfigConstants,
    visible: bool,
}

const DEFAULT_VISIBILITY: bool = false;

// Helper for viewing color schemes in the config UI.
fn add_color_scheme(
    ui: &mut Ui,
    config_scheme: &mut ConfigUiScheme,
    scheme: &mut Scheme,
    displayed_scheme_index: &mut usize,
    edit_scheme_index: usize,
    engine: &mut Engine,
) {
    // Helper to add rgb widgets and sliders associated with part of a color-scheme.
    fn add_scheme_element(
        ui: &mut Ui,
        rgb: &mut [[u8; 3]; 4],
        val: &mut [f32; 4],
        range: RangeInclusive<f32>,
        changed: &mut bool,
    ) {
        let n = rgb.len();
        for i in 0..n - 1 {
            *changed |= ui.color_edit_button_srgb(&mut rgb[i]).changed();
            *changed |= ui.add(Slider::new(&mut val[i], range.clone())).changed();
            ui.end_row();
        }
        *changed |= ui.color_edit_button_srgb(&mut rgb[n - 1]).changed();
        ui.end_row();
    }

    // Helper to enforce the given list is an increasing sequence.
    fn enforce_limits(vals: &mut [f32; 4], changed: &mut bool) {
        let mut max = 0.;
        for v in &mut vals[0..3] {
            if *v < max {
                *v = max;
                *changed = true;
            } else {
                max = *v;
            }
        }
    }
    let mut changed = false;
    enforce_limits(&mut config_scheme.index_val, &mut changed);
    enforce_limits(&mut config_scheme.speed_val, &mut changed);

    ScrollArea::vertical().max_height(350.).show(ui, |ui| {
        ui.heading("Index-Based Color Scheme");
        egui::Grid::new("scheme_index_grid").show(ui, |ui| {
            add_scheme_element(
                ui,
                &mut config_scheme.index_rgb,
                &mut config_scheme.index_val,
                0.0..=1.,
                &mut changed,
            );
        });
        ui.heading("Speed-Based Color Scheme");
        egui::Grid::new("scheme_speed_grid").show(ui, |ui| {
            add_scheme_element(
                ui,
                &mut config_scheme.speed_rgb,
                &mut config_scheme.speed_val,
                0.0..=10.,
                &mut changed,
            );
        });

        if edit_scheme_index != *displayed_scheme_index
            && ui.button("Make this color scheme active").clicked()
        {
            *displayed_scheme_index = edit_scheme_index;
            engine.update_color_scheme(config_scheme.into());
        }

        if changed {
            *scheme = config_scheme.into();
            if edit_scheme_index == *displayed_scheme_index {
                engine.update_color_scheme(config_scheme.into());
            }
        }
    });
}

fn update_app_constants(engine: &mut Engine, config: ConfigConstants) {
    let constants = constants_from_presentable(config);
    engine.update_app_constants(constants);
}

// Define the layout and behavior of the config UI.
fn create_config_ui(
    gui: &mut Gui,
    config_window: &mut ConfigWindow,
    engine: &mut Engine,
    color_scheme_names: &[String],
    color_schemes: &mut [Scheme],
    displayed_scheme_index: &mut usize,
) {
    let ctx = gui.context();
    egui::Window::new("App Config")
        .open(&mut config_window.visible)
        .resizable(true)
        .show(&ctx, |ui| {
            let mut data_changed = false;
            ComboBox::from_label("Selected Color Scheme")
                .selected_text(color_scheme_names[config_window.edit_scheme_index].clone())
                .show_ui(ui, |ui| {
                    for (i, name) in color_scheme_names.iter().enumerate() {
                        ui.selectable_value(&mut config_window.edit_scheme_index, i, name.clone());
                    }
                });
            add_color_scheme(
                ui,
                &mut config_window.color_schemes[config_window.edit_scheme_index],
                &mut color_schemes[config_window.edit_scheme_index],
                displayed_scheme_index,
                config_window.edit_scheme_index,
                engine,
            );
            ui.separator();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.config.audio_scale, -30.0..=5.)
                        .text("audio scale (dB)"),
                )
                .changed();
            data_changed |= ui
                .add(Slider::new(&mut config_window.config.max_speed, 0.0..=10.).text("max speed"))
                .changed();
            data_changed |= ui
                .add(Slider::new(&mut config_window.config.point_size, 0.0..=8.).text("point size"))
                .changed();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.config.friction_scale, 0.0..=5.)
                        .text("friction scale"),
                )
                .changed();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.config.spring_coefficient, 0.0..=200.)
                        .text("spring coefficient"),
                )
                .changed();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.config.vertical_fov, 30.0..=105.)
                        .text("vertical fov"),
                )
                .changed();

            // Allow a checkbox to toggle the hiding of stationary particles.
            let mut hide_stationary_particles = config_window.config.hide_stationary_particles > 0;
            if ui
                .checkbox(&mut hide_stationary_particles, "Hide stationary particles")
                .changed()
            {
                data_changed = true;
                config_window.config.hide_stationary_particles =
                    u32::from(hide_stationary_particles);
            }

            // Separate between the `Reset` button and setting configuration values.
            ui.separator();

            ui.horizontal(|ui| {
                // Allow user to reset back to values currently applied.
                if ui
                    .button("Reset")
                    .on_hover_text("Reset displayed values to the constants used at launch.")
                    .clicked()
                {
                    config_window.config = config_window.init_config;
                    config_window
                        .color_schemes
                        .copy_from_slice(&config_window.init_color_schemes);

                    update_app_constants(engine, config_window.config);

                    let new_colors: Vec<_> = config_window
                        .color_schemes
                        .iter()
                        .map(|cs| (*cs).into())
                        .collect();
                    color_schemes.copy_from_slice(&new_colors);
                    engine.update_color_scheme(color_schemes[*displayed_scheme_index]);
                }
            });

            if data_changed {
                update_app_constants(engine, config_window.config);
            }
        });
}

enum HelpWindowEntry {
    Title(&'static str),
    Item(&'static str, &'static str),
    Empty(),
}

// Define the layout and behavior of the config UI.
fn create_help_ui(gui: &mut Gui, visible: &mut bool) {
    use HelpWindowEntry::{Empty, Item, Title};
    let ctx = gui.context();
    egui::Window::new("Help")
        .open(visible)
        .resizable(true)
        .show(&ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                let controls_list = [
                    Title("App-Window Management"),
                    Item("F11", "Toggle window fullscreen"),
                    Item("ESC", "If fullscreen, then enter windowed mode. Else, close the application"),
                    #[cfg(all(not(debug_assertions), target_os = "windows"))]
                    Item("ENTER", "Toggle the visibility of the output command prompt"),
                    Empty(),
                    Title("Overlay-Window Management"),
                    Item("F1", "Toggle visibility of this Help window"),
                    Item("C", "Toggle visibility of the App Config window"),
                    Empty(),
                    Title("Audio"),
                    Item("R", "Toggle the application's responsiveness to system audio"),
                    Empty(),
                    Title("Visuals"),
                    Item("SPACE", "Toggle kaleidoscope effect on fractals"),
                    Item("J", "Toggle 'jello' effect on particles (i.e., the fixing of particles to a position with spring tension)"),
                    Item("P", "Toggle the rendering and updating of particles"),
                    Item("H", "Toggles whether to hide stationary particles"),
                    Item("CAPS", "Toggle negative-color effect for particles"),
                    Item("D", "Toggle between 2D and 3D projections of the particles"),
                    Item("TAB", "Cycle through particle color schemes. *Requires that all overlay windows are closed*"),
                    Item("0", "Select the 'empty' fractal"),
                    Item("1-6", "Select the fractal corresponding to the respective key"),
                    Item("MOUSE-BTTN", "Holding the primary or secondary mouse button applies a repulsive or attractive force, respectively, at the cursor's position"),
                    Item("MOUSE-SCRL", "Scrolling up or down changes the strength of the cursor's applied force"),
                ];
                egui::Grid::new("scheme_index_grid").show(ui, |ui| {
                    for entry in controls_list {
                        match entry {
                            Empty() => {}
                            Item(key, desc) => {
                                ui.vertical_centered(|ui| ui.label(egui::RichText::new(key).monospace().strong()));
                                ui.label(desc);
                            }
                            Title(title) => {
                                ui.separator();
                                ui.heading(title);
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        });
}

impl AppOverlay {
    pub fn new(
        surface: Arc<Surface>,
        swapchain: &Arc<Swapchain>,
        queue: Arc<Queue>,
        event_loop: &EventLoop<()>,
        subpass: Subpass,
        app_config: &AppConfig,
    ) -> Self {
        let gui = Gui::new_with_subpass(
            event_loop,
            surface,
            queue,
            subpass,
            GuiConfig {
                preferred_format: Some(swapchain.image_format()),
                is_overlay: true,
                ..Default::default()
            },
        );

        let initial_config = constants_to_presentable(app_config.into());
        let initial_colors: Vec<ConfigUiScheme> = app_config
            .color_schemes
            .iter()
            .map(|cs| (*cs).into())
            .collect();

        let config_window = ConfigWindow {
            color_schemes: initial_colors.clone(),
            init_color_schemes: initial_colors,
            edit_scheme_index: 0,
            config: initial_config,
            init_config: initial_config,
            visible: DEFAULT_VISIBILITY,
        };

        Self {
            config_window,
            gui,
            help_visible: app_config.launch_help_visible,
        }
    }

    pub fn handle_input(&mut self, event: &WindowEvent) -> bool {
        // Handle UI events.
        self.gui.update(event)
    }

    // Draw config UI to window.
    pub fn draw(
        &mut self,
        engine: &mut Engine,
        color_scheme_names: &[String],
        color_schemes: &mut [Scheme],
        displayed_scheme_index: &mut usize,
    ) -> Option<SecondaryAutoCommandBuffer> {
        // Quick escape the render if window is not visible.
        if !self.visible() {
            return None;
        }

        // Setup UI layout.
        self.gui.immediate_ui(|gui| {
            // Draw config window.
            create_config_ui(
                gui,
                &mut self.config_window,
                engine,
                color_scheme_names,
                color_schemes,
                displayed_scheme_index,
            );

            // Draw help window.
            create_help_ui(gui, &mut self.help_visible);
        });

        Some(
            self.gui
                .draw_on_subpass_image(engine.window().inner_size().into()),
        )
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }
    pub fn toggle_config(&mut self) {
        self.config_window.visible = !self.config_window.visible;
    }
    pub fn toggle_hide_stationary_particles(&mut self, engine: &mut Engine) {
        self.config_window.config.hide_stationary_particles =
            1 - self.config_window.config.hide_stationary_particles;
        update_app_constants(engine, self.config_window.config);
    }
    pub fn visible(&self) -> bool {
        self.help_visible || self.config_window.visible
    }
}

const DECIBEL_SCALE: f32 = std::f32::consts::LN_10 / 10.;

// Helpers for converting between presentation and internal units of measure.
fn constants_to_presentable(app_constants: ConfigConstants) -> ConfigConstants {
    ConfigConstants {
        audio_scale: app_constants.audio_scale.ln() / DECIBEL_SCALE,
        vertical_fov: app_constants.vertical_fov * 360. / std::f32::consts::PI,
        ..app_constants
    }
}
fn constants_from_presentable(app_constants: ConfigConstants) -> ConfigConstants {
    ConfigConstants {
        audio_scale: (DECIBEL_SCALE * app_constants.audio_scale).exp(),
        vertical_fov: app_constants.vertical_fov * std::f32::consts::PI / 360.,
        ..app_constants
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
        Self::from(*ui_scheme)
    }
}
