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

pub struct AppOverlay {
    config_window: ConfigWindow,
    gui: Gui,
    help_visible: bool,
    queue: Arc<Queue>,
}

struct ConfigWindow {
    color_schemes: Vec<ConfigUiScheme>,
    init_color_schemes: Vec<ConfigUiScheme>,
    edit_scheme_index: usize,

    state: AppConstants,
    init_state: AppConstants,
    visible: bool,
}

const DEFAULT_VISIBILITY: bool = false;

fn add_color_scheme(
    ui: &mut Ui,
    config_scheme: &mut ConfigUiScheme,
    scheme: &mut Scheme,
    displayed_scheme_index: &mut usize,
    edit_scheme_index: usize,
    engine: &mut Engine,
) {
    // Helper to add rgb widgets and sliders associated with part of a color-scheme
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

    // Helper to enforce the given list is an increasing sequence
    fn enforce_limits(vals: &mut [f32; 4], changed: &mut bool) {
        let mut max = 0.;
        for v in vals[0..3].iter_mut() {
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

// Define the layout and behavior of the config UI
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
            ComboBox::from_label("Active Color Scheme")
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
                    Slider::new(&mut config_window.state.audio_scale, -30.0..=5.)
                        .text("audio scale (dB)"),
                )
                .changed();
            data_changed |= ui
                .add(Slider::new(&mut config_window.state.max_speed, 0.0..=10.).text("max speed"))
                .changed();
            data_changed |= ui
                .add(Slider::new(&mut config_window.state.point_size, 0.0..=8.).text("point size"))
                .changed();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.state.friction_scale, 0.0..=5.)
                        .text("friction scale"),
                )
                .changed();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.state.spring_coefficient, 0.0..=200.)
                        .text("spring coefficient"),
                )
                .changed();
            data_changed |= ui
                .add(
                    Slider::new(&mut config_window.state.vertical_fov, 30.0..=105.)
                        .text("vertical fov"),
                )
                .changed();
            ui.separator();
            ui.horizontal(|ui| {
                // Allow user to reset back to values currently applied
                if ui
                    .button("Reset")
                    .on_hover_text("Reset displayed values to the constants used at launch.")
                    .clicked()
                {
                    config_window.state = config_window.init_state;
                    config_window
                        .color_schemes
                        .copy_from_slice(&config_window.init_color_schemes);

                    let constants = constants_from_presentable(config_window.state);
                    engine.update_app_constants(constants);

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
                let constants = constants_from_presentable(config_window.state);
                engine.update_app_constants(constants);
            }
        });
}

enum HelpWindowEntry {
    Title(&'static str),
    Item(&'static str, &'static str),
    Empty(),
}

// Define the layout and behavior of the config UI
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
                    #[cfg(target_os = "windows")]
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
                    Item("CAPS", "Toggle negative-color effect for particles"),
                    Item("D", "Toggle between 2D and 3D projections of the particles"),
                    Item("TAB", "Cycle through particle color schemes. *Requires that all overlay windows are closed*"),
                    Item("0", "Select the 'empty' fractal"),
                    Item("1-5", "Select the fractal corresponding to the respective key"),
                    Item("MOUSE-BTTN", "Holding the primary or secondary mouse button applies a repulsive or attractive force, respectively, at the cursor's position"),
                    Item("MOUSE-SCRL", "Scrolling up or down changes the strength of the cursor's applied force"),
                ];
                egui::Grid::new("scheme_index_grid").show(ui, |ui| {
                    for entry in controls_list {
                        match entry {
                            Empty() => {},
                            Item(key, desc) => {
                                ui.vertical_centered(|ui| ui.label(egui::RichText::new(key).monospace()));
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

        let config_window = ConfigWindow {
            color_schemes: initial_colors.clone(),
            init_color_schemes: initial_colors,
            edit_scheme_index: 0,
            state: initial_state,
            init_state: initial_state,
            visible: DEFAULT_VISIBILITY,
        };

        Self {
            config_window,
            gui,
            help_visible: app_config.launch_help_visible,
            queue,
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
        if !self.visible() {
            return vulkano::sync::now(self.queue.device().clone()).boxed();
        }

        // Setup UI layout
        self.gui.immediate_ui(|gui| {
            // Draw config window
            create_config_ui(
                gui,
                &mut self.config_window,
                engine,
                color_scheme_names,
                color_schemes,
                displayed_scheme_index,
            );

            // Draw help window
            create_help_ui(gui, &mut self.help_visible);
        });

        self.gui.draw_on_image(before_future, frame)
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }
    pub fn toggle_config(&mut self) {
        self.config_window.visible = !self.config_window.visible;
    }
    pub fn visible(&self) -> bool {
        self.help_visible || self.config_window.visible
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
