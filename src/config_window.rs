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

use egui_winit_vulkano::Gui;
use vulkano::device::Queue;
use vulkano::image::view::ImageView;
use vulkano::image::SwapchainImage;
use vulkano::instance::Instance;
use vulkano::swapchain::{PresentMode, Surface};
use winit::{
    event::WindowEvent,
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

use crate::engine::core::{select_hardware, AcquiredImageData, EngineSwapchain};

fn sized_text(ui: &mut egui::Ui, text: impl Into<String>, size: f32) {
    ui.label(egui::RichText::new(text).size(size));
}

pub fn create_ui(gui: &mut Gui) {
    let ctx = gui.context();
    egui::CentralPanel::default().show(&ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add(egui::widgets::Label::new("Hi there!"));
            sized_text(ui, "Rich Text", 32.0);
        });
        ui.separator();
        ui.color_edit_button_rgb(&mut [1.; 3]);
    });
}

pub struct ConfigWindow {
    framebuffers: Vec<Arc<ImageView<SwapchainImage<Window>>>>,
    gui: Gui,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    queue: Arc<Queue>,
    visible: bool,
}

const DEFAULT_VISIBILITY: bool = false;

impl ConfigWindow {
    pub fn new(instance: &Arc<Instance>, event_loop: &EventLoop<()>) -> Self {
        use vulkano_win::VkSurfaceBuild;
        let surface = WindowBuilder::new()
            .with_title("app config")
            .with_resizable(false)
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

        Self {
            framebuffers,
            gui,
            surface,
            swapchain,
            queue,
            visible: DEFAULT_VISIBILITY,
        }
    }

    pub fn handle_input(&mut self, event: &WindowEvent) {
        self.gui.update(event);

        // Ensure to handle the 'close' event
        if event == &WindowEvent::CloseRequested {
            self.window().set_visible(false);
            self.visible = false;
        }
    }

    pub fn draw(&mut self) {
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
        self.gui.immediate_ui(create_ui);

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
    pub fn window(&self) -> &Window {
        self.surface.window()
    }
}
