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
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass};
use vulkano::swapchain::{PresentMode, Surface};
use vulkano::{device::Device, instance::Instance};
use winit::{
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
    framebuffers: Vec<Arc<Framebuffer>>,
    pub gui: Gui,
    render_pass: Arc<RenderPass>,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    queue: Arc<Queue>,
}

impl ConfigWindow {
    pub fn new(instance: Arc<Instance>, event_loop: &EventLoop<()>) -> Self {
        use vulkano_win::VkSurfaceBuild;
        let surface = WindowBuilder::new()
            .with_title("app config")
            .with_resizable(false)
            .with_visible(false)
            .build_vk_surface(event_loop, instance.clone())
            .unwrap();

        let (physical_device, device, queue) = select_hardware(&instance, &surface);

        let swapchain = EngineSwapchain::new(
            &physical_device,
            device.clone(),
            surface.clone(),
            PresentMode::Fifo,
        );

        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    format: swapchain.image_format(),
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        )
        .unwrap();

        let framebuffers = create_framebuffers(
            &device,
            &render_pass,
            surface.window().inner_size().into(),
            swapchain.images(),
            swapchain.image_format(),
        );

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
            render_pass,
            surface,
            swapchain,
            queue,
        }
    }

    pub fn draw(&mut self) {
        match self.swapchain.acquire_next_image() {
            Ok(AcquiredImageData {
                acquire_future,
                image_index,
                ..
            }) => {
                let future = self.gui.draw_on_image(
                    acquire_future,
                    self.framebuffers[image_index].attachments()[0].clone(),
                );

                self.swapchain.present(self.queue.clone(), future);
            }
            Err(e) => panic!("Failed to acquire next image: {:?}", e),
        }
    }
}

fn create_framebuffers(
    device: &Arc<Device>,
    render_pass: &Arc<RenderPass>,
    dimensions: [u32; 2],
    images: &Vec<Arc<SwapchainImage<Window>>>,
    image_format: vulkano::format::Format,
) -> Vec<Arc<Framebuffer>> {
    (0..images.len())
        .map(|i| {
            let view = ImageView::new_default(images[i].clone()).unwrap();

            // Create framebuffer specifying underlying renderpass and image attachments
            Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![view],
                    ..Default::default()
                },
            )
            .unwrap()
        })
        .collect()
}
