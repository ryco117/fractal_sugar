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

use bytemuck::{Pod, Zeroable};
use vulkano::buffer::{device_local::DeviceLocalBuffer, BufferUsage};
use vulkano::descriptor_set::single_layout_pool::SingleLayoutDescSetPool;
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::image::view::ImageView;
use vulkano::image::{AttachmentImage, ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, Pipeline};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::swapchain::{PresentInfo, PresentMode, Surface};
use vulkano::sync::{FenceSignalFuture, GpuFuture};
use vulkano_win::VkSurfaceBuild;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::EventLoop;
use winit::window::{Fullscreen, Icon, Window, WindowBuilder};

pub mod core;
mod object;
pub mod pipeline;
pub mod renderer;
mod vertex;

use self::core::{EngineSwapchain, RecreateSwapchainResult};
use crate::app_config::{AppConfig, Scheme};
use object::{Fractal, Particles};
pub use object::{FractalPushConstants, ParticleComputePushConstants, ParticleVertexPushConstants};

type EngineFrameFuture = Arc<FenceSignalFuture<Box<dyn GpuFuture>>>;

pub struct Engine {
    app_constants: Arc<DeviceLocalBuffer<AppConstants>>,
    device: Arc<Device>,
    fences: Vec<Option<EngineFrameFuture>>,
    fractal: Fractal,
    framebuffers: Vec<Arc<Framebuffer>>,
    particles: Particles,
    previous_fence_index: usize,
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    viewport: Viewport,
}

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 450;
const DEBUG_VULKAN: bool = false;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct AppConstants {
    pub max_speed: f32,
    pub particle_count: f32,
}

pub struct DrawData {
    pub particle_data: Option<(
        object::ParticleComputePushConstants,
        object::ParticleVertexPushConstants,
    )>,
    pub fractal_data: object::FractalPushConstants,
}

impl Engine {
    pub fn new(event_loop: &EventLoop<()>, app_config: &AppConfig, icon: Option<Icon>) -> Self {
        // Create instance with extensions required for windowing (and optional debugging layer(s))
        let instance = {
            let library =
                vulkano::VulkanLibrary::new().expect("Could not determine Vulkan library to use.");
            let enabled_extensions = vulkano_win::required_extensions(&library);
            Instance::new(
                library,
                InstanceCreateInfo {
                    enabled_extensions,
                    enabled_layers: if DEBUG_VULKAN {
                        vec!["VK_LAYER_KHRONOS_validation".to_owned()]
                    } else {
                        vec![]
                    },
                    enumerate_portability: true, // Allows for non-conformant devices to be considered when searching for the best graphics device
                    ..Default::default()
                },
            )
            .expect("Failed to create Vulkan instance")
        };

        // Create a window! Set some basic window properties and get a vulkan surface
        let surface = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
            .with_title("fractal_sugar")
            .with_window_icon(icon)
            .with_fullscreen(if app_config.launch_fullscreen {
                Some(Fullscreen::Borderless(None))
            } else {
                None
            })
            .build_vk_surface(event_loop, instance.clone())
            .unwrap();

        // Fetch device resources based on what is available to the system
        let (physical_device, device, queue) = core::select_hardware(&instance, &surface);

        // Create swapchain and associated image buffers from the relevant parameters
        let engine_swapchain = EngineSwapchain::new(
            &physical_device,
            device.clone(),
            surface.clone(),
            PresentMode::Fifo,
        );
        let image_format = engine_swapchain.swapchain().image_format();

        // Create a frame-in-flight fence for each image buffer.
        // This allows CPU work to continue while GPU is busy with previous frames
        let frames_in_flight = engine_swapchain.images().len();
        let fences: Vec<Option<EngineFrameFuture>> = vec![None; frames_in_flight];

        // Before creating descriptor sets and other buffers, allocate app-constants buffer
        let particle_count: usize = app_config.particle_count;
        let particle_count_f32: f32 = particle_count as f32;
        let (app_constants, app_constants_future) = DeviceLocalBuffer::from_data(
            AppConstants {
                max_speed: app_config.max_speed,
                particle_count: particle_count_f32,
            },
            BufferUsage {
                uniform_buffer: true,
                ..Default::default()
            },
            queue.clone(),
        )
        .unwrap();

        let render_pass = create_app_render_pass(&device, image_format);

        // Define our 2D viewspace (with normalized depth)
        let dimensions = surface.window().inner_size();
        let viewport = Viewport {
            origin: [0., 0.],
            dimensions: dimensions.into(),
            depth_range: 0.0..1.,
        };

        // Create our "objects"™️
        let fractal = Fractal::new(&device, &render_pass, viewport.clone());
        let particles = Particles::new(
            &device,
            &queue,
            &render_pass,
            viewport.clone(),
            app_config,
            &app_constants,
            app_constants_future,
        );

        // Create a framebuffer to store results of render pass
        let framebuffers = create_framebuffers(
            &device,
            &render_pass,
            dimensions.into(),
            &engine_swapchain.images(),
            image_format,
        );

        // Construct new Engine
        Self {
            app_constants,
            device,
            fences,
            fractal,
            framebuffers,
            particles,
            previous_fence_index: 0,
            queue,
            render_pass,
            surface,
            swapchain: engine_swapchain,
            viewport,
        }
    }

    // Recreate swapchain and necessary follow-up structures (often for window resizing)
    pub fn recreate_swapchain(
        &mut self,
        dimensions: PhysicalSize<u32>,
        window_resized: bool,
    ) -> RecreateSwapchainResult {
        if dimensions.width == 0 || dimensions.height == 0 {
            // Empty window detected, skipping swapchain recreation.
            // Vulkan panics if either dimensions are zero, bail here instead
            return RecreateSwapchainResult::ExtentNotSupported;
        }

        // Create new swapchain from the previous, specifying new window size
        match self.swapchain.recreate(dimensions) {
            // Continue logic
            RecreateSwapchainResult::Success => {}

            // Return that swapchain could not be recreated (often due to a resizing error)
            RecreateSwapchainResult::ExtentNotSupported => {
                return RecreateSwapchainResult::ExtentNotSupported
            }
        }

        // Framebuffer is tied to the swapchain images, must recreate as well
        self.framebuffers = create_framebuffers(
            &self.device,
            &self.render_pass,
            dimensions.into(),
            &self.swapchain.images(),
            self.swapchain.image_format(),
        );

        // If caller indicates a resize has prompted this call then adjust viewport and fixed-view pipeline
        if window_resized {
            self.viewport.dimensions = dimensions.into();

            // Since pipeline specifies viewport is fixed, entire pipeline needs to be reconstructed to account for size change
            self.particles.graphics_pipeline = pipeline::create_particle(
                self.device.clone(),
                &self.particles.vert_shader,
                &self.particles.frag_shader,
                Subpass::from(self.render_pass.clone(), 0).unwrap(),
                self.viewport.clone(),
            );
            self.fractal.pipeline = pipeline::create_fractal(
                self.device.clone(),
                &self.fractal.vert_shader,
                &self.fractal.frag_shader,
                Subpass::from(self.render_pass.clone(), 1).unwrap(),
                self.viewport.clone(),
            );
        }

        // Recreated swapchain and necessary follow-up structures without error
        RecreateSwapchainResult::Success
    }

    // Use given push constants and synchronization-primitives to render next frame in swapchain.
    // Returns whether a swapchain recreation was deemed necessary
    pub fn draw_frame(&mut self, draw_data: &DrawData) -> bool {
        // Acquire the index of the next image we should render to in this swapchain
        let (image_index, suboptimal, acquire_future) = match vulkano::swapchain::acquire_next_image(
            self.swapchain.swapchain(),
            None, /*timeout*/
        ) {
            Ok(tuple) => tuple,
            Err(vulkano::swapchain::AcquireError::OutOfDate) => return true,
            Err(e) => panic!("Failed to acquire next image: {:?}", e),
        };

        if suboptimal {
            // Acquired image will still work for rendering this frame, so we will continue.
            // However, the surface's properties no longer match the swapchain's so we will recreate next chance
            return true;
        }

        // If this image buffer already has a fence, wait for the fence to be completed, then cleanup.
        // Usually the fence for this index will have completed by the time we are rendering it again
        if let Some(image_fence) = &mut self.fences[image_index] {
            image_fence.wait(None).unwrap();
            image_fence.cleanup_finished();
        }

        // If the previous image has a fence, use it for synchronization, else create a new one
        let previous_future = match self.fences[self.previous_fence_index].clone() {
            // Ensure current frame is synchronized with previous
            Some(fence) => fence.boxed(),

            // Create new future to guarentee synchronization with (fake) previous frame
            None => vulkano::sync::now(self.device.clone()).boxed(),
        };

        // Create a one-time-submit command buffer for this frame
        let colored_sugar_commands = renderer::create_render_commands(self, image_index, draw_data);

        // Create synchronization future for rendering the current frame
        let future = previous_future
            // Wait for previous and current futures to synchronize
            .join(acquire_future)
            // Execute the one-time command buffer
            .then_execute(self.queue.clone(), colored_sugar_commands)
            .unwrap()
            // Present result to swapchain buffer
            .then_swapchain_present(
                self.queue.clone(),
                PresentInfo {
                    index: image_index,
                    ..vulkano::swapchain::PresentInfo::swapchain(self.swapchain.swapchain())
                },
            )
            .boxed()
            // Finish synchronization
            .then_signal_fence_and_flush();

        // Update this frame's future with result of current render
        let mut requires_recreate_swapchain = false;
        self.fences[image_index] = match future {
            // Success, store result into vector
            Ok(value) => Some(Arc::new(value)),

            // Swapchain is out-of-date, request its recreation next frame
            Err(vulkano::sync::FlushError::OutOfDate) => {
                requires_recreate_swapchain = true;
                None
            }

            // Unknown failure
            Err(e) => panic!("Failed to flush future: {:?}", e),
        };

        // Update the last rendered index to be this frame
        self.previous_fence_index = image_index;

        // Return whether a swapchain recreation was deemed necessary
        requires_recreate_swapchain
    }

    pub fn update_color_scheme(&mut self, scheme: Scheme) {
        let color_scheme_buffer = self.particles.scheme_buffer_pool.from_data(scheme).unwrap();
        self.particles.graphics_descriptor_set = PersistentDescriptorSet::new(
            self.particles
                .graphics_pipeline
                .layout()
                .set_layouts()
                .get(0)
                .unwrap()
                .clone(),
            [
                WriteDescriptorSet::buffer(0, color_scheme_buffer),
                WriteDescriptorSet::buffer(1, self.app_constants.clone()),
            ],
        )
        .unwrap();
    }

    // Engine getters
    pub fn compute_pipeline(&self) -> Arc<ComputePipeline> {
        self.particles.compute_pipeline.clone()
    }
    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }
    pub fn compute_descriptor_set(&self) -> Arc<PersistentDescriptorSet> {
        self.particles.compute_descriptor_set.clone()
    }
    pub fn fractal_descriptor_pool(&mut self) -> &mut SingleLayoutDescSetPool {
        &mut self.fractal.descriptor_set_pool
    }
    pub fn fractal_pipeline(&self) -> Arc<GraphicsPipeline> {
        self.fractal.pipeline.clone()
    }
    pub fn particle_descriptor_set(&self) -> Arc<PersistentDescriptorSet> {
        self.particles.graphics_descriptor_set.clone()
    }
    pub fn particle_pipeline(&self) -> Arc<GraphicsPipeline> {
        self.particles.graphics_pipeline.clone()
    }
    pub fn queue(&self) -> Arc<Queue> {
        self.queue.clone()
    }
    pub fn surface(&self) -> Arc<Surface<Window>> {
        self.surface.clone()
    }
    pub fn particle_count(&self) -> u64 {
        use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
        self.particles.vertex_buffer.len()
    }
}

// Helper for (re)creating framebuffers
fn create_framebuffers(
    device: &Arc<Device>,
    render_pass: &Arc<RenderPass>,
    dimensions: [u32; 2],
    images: &Vec<Arc<SwapchainImage<Window>>>,
    image_format: vulkano::format::Format,
) -> Vec<Arc<Framebuffer>> {
    (0..images.len())
        .map(|i| {
            // To interact with image buffers or framebuffers from shaders we create a view defining how the image will be used.
            // This view, which belongs to the swapchain, will be the destination (i.e. fractal) view
            let view = ImageView::new_default(images[i].clone()).unwrap();

            // Create image attachment for MSAA particles.
            // It is transient but cannot be used as an input
            let msaa_view = ImageView::new_default(
                AttachmentImage::transient_multisampled(
                    device.clone(),
                    dimensions,
                    vulkano::image::SampleCount::Sample8,
                    image_format,
                )
                .unwrap(),
            )
            .unwrap();

            // Create image attachment for resolved particles.
            // It is transient and will be used as an input to a later pass
            let particle_view = ImageView::new_default(
                AttachmentImage::with_usage(
                    device.clone(),
                    dimensions,
                    image_format,
                    ImageUsage {
                        transfer_dst: true,
                        input_attachment: true,
                        color_attachment: true,
                        ..ImageUsage::empty()
                    },
                )
                .unwrap(),
            )
            .unwrap();

            // Create an attachement for the particle's depth buffer
            let particle_depth = ImageView::new_default(
                AttachmentImage::transient_multisampled_input_attachment(
                    device.clone(),
                    dimensions,
                    vulkano::image::SampleCount::Sample8,
                    vulkano::format::Format::D16_UNORM,
                )
                .unwrap(),
            )
            .unwrap();

            // Create framebuffer specifying underlying renderpass and image attachments
            Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![msaa_view, particle_view, particle_depth, view], // Must add specified attachments in order
                    ..Default::default()
                },
            )
            .unwrap()
        })
        .collect()
}

// Helper for initializing the app render pass
fn create_app_render_pass(
    device: &Arc<Device>,
    image_format: vulkano::format::Format,
) -> Arc<RenderPass> {
    vulkano::ordered_passes_renderpass!(
        device.clone(),
        attachments: {
            // The first framebuffer attachment is the intermediary image
            intermediary: {
                load: Clear,
                store: DontCare,
                format: image_format,
                samples: 8, // MSAA for smooth particles. Must be resolved to non-sampled image for presentation
            },

            particle_color: {
                load: DontCare, // Resolve does not need destination image to be cleared
                store: DontCare,
                format: image_format, // Use swapchain's format since we are writing to its buffers
                samples: 1,
            },

            particle_depth: {
                load: Clear,
                store: DontCare,
                format: vulkano::format::Format::D16_UNORM,
                samples: 8, // Must match sample count of color
            },

            fractal_color: {
                load: DontCare,
                store: Store,
                format: image_format, // Use swapchain's format since we are writing to its buffers
                samples: 1, // No MSAA necessary when rendering a single quad with shaders ;)
            }
        },
        passes: [
            // Particles pass
            {
                color: [intermediary],
                depth_stencil: {particle_depth},
                input: [],

                // The `resolve` array here must contain either zero entries (if you don't use
                // multisampling), or one entry per color attachment. At the end of the pass, each
                // color attachment will be *resolved* into the given image. In other words, here, at
                // the end of the pass, the `intermediary` attachment will be copied to the attachment
                // named `particle_color`.
                resolve: [particle_color],
            },

            // Fractal pass
            {
                color: [fractal_color],
                depth_stencil: {},
                input: [particle_color, particle_depth]
            }
        ]
    )
    .unwrap()
}
