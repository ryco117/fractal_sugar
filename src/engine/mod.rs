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
use vulkano::buffer::cpu_pool::CpuBufferPoolSubbuffer;
use vulkano::buffer::CpuBufferPool;
use vulkano::descriptor_set::single_layout_pool::SingleLayoutDescSetPool;
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::device::{Device, Queue};
use vulkano::image::view::ImageView;
use vulkano::image::{AttachmentImage, ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::memory::pool::StandardMemoryPool;
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::swapchain::{AcquireError, PresentMode, Surface};
use vulkano::sync::GpuFuture;
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

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 450;
const DEBUG_VULKAN: bool = false;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct AppConstants {
    pub max_speed: f32,
    pub particle_count: f32,
    pub spring_coefficient: f32,
    pub point_size: f32,

    pub audio_scale: f32,

    pub vertical_fov: f32,
}
struct AppConstantsState {
    pub buffer: Arc<CpuBufferPoolSubbuffer<AppConstants, Arc<StandardMemoryPool>>>,
    pub constants: AppConstants,
    pub pool: CpuBufferPool<AppConstants>,
}

pub struct DrawData {
    pub particle_data: Option<(
        object::ParticleComputePushConstants,
        object::ParticleVertexPushConstants,
    )>,
    pub fractal_data: object::FractalPushConstants,
}

pub struct Engine {
    app_constants: AppConstantsState,
    device: Arc<Device>,
    fractal: Fractal,
    framebuffers: Vec<Arc<Framebuffer>>,
    instance: Arc<Instance>,
    particles: Particles,
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    viewport: Viewport,
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

        // Before creating descriptor sets and other buffers, allocate app-constants buffer
        let app_constants = {
            let pool: CpuBufferPool<AppConstants> = CpuBufferPool::uniform_buffer(device.clone());
            let constants = app_config.into();
            let buffer = pool.from_data(constants).unwrap();
            AppConstantsState {
                buffer,
                constants,
                pool,
            }
        };

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
            &app_constants.buffer,
        );

        // Create a framebuffer to store results of render pass
        let framebuffers = create_framebuffers(
            &device,
            &render_pass,
            dimensions.into(),
            engine_swapchain.images(),
            image_format,
        );

        // Construct new Engine
        Self {
            app_constants,
            device,
            fractal,
            framebuffers,
            instance,
            particles,

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
            RecreateSwapchainResult::Ok => {}

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
            self.swapchain.images(),
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
        RecreateSwapchainResult::Ok
    }

    // Use given push constants and synchronization-primitives to render next frame in swapchain.
    // Returns whether a swapchain recreation was deemed necessary
    pub fn render(
        &mut self,
        draw_data: &DrawData,
    ) -> Result<(Box<dyn GpuFuture>, bool), AcquireError> {
        // Acquire the index of the next image we should render to in this swapchain
        let core::AcquiredImageData {
            image_index,
            suboptimal,
            acquire_future,
        } = match self.swapchain.acquire_next_image() {
            Ok(tuple) => tuple,
            Err(e) => return Err(e),
        };

        // Create a one-time-submit command buffer for this frame
        let colored_sugar_commands = {
            let framebuffer = self.framebuffers[image_index].clone();
            renderer::create_render_commands(self, &framebuffer, draw_data)
        };

        // Create synchronization future for rendering the current frame
        let future = acquire_future
            // Execute the one-time command buffer
            .then_execute(self.queue.clone(), colored_sugar_commands)
            .unwrap()
            .boxed();

        Ok((future, suboptimal))
    }

    pub fn present(&mut self, future: Box<dyn GpuFuture>) -> bool {
        self.swapchain.present(self.queue.clone(), future)
    }

    pub fn update_color_scheme(&mut self, scheme: Scheme) {
        self.particles
            .update_color_scheme(scheme, self.app_constants.buffer.clone());
    }
    pub fn update_app_constants(&mut self, app_constants: AppConstants) {
        self.app_constants.constants = app_constants;
        self.app_constants.buffer = self.app_constants.pool.from_data(app_constants).unwrap();
        self.particles
            .update_app_constants(self.app_constants.buffer.clone());
    }

    // Engine getters
    pub fn app_constants(&self) -> &AppConstants {
        &self.app_constants.constants
    }
    pub fn compute_descriptor_set(&self) -> Arc<PersistentDescriptorSet> {
        self.particles.compute_descriptor_set.clone()
    }
    pub fn compute_pipeline(&self) -> Arc<ComputePipeline> {
        self.particles.compute_pipeline.clone()
    }
    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }
    pub fn fractal_descriptor_pool(&mut self) -> &mut SingleLayoutDescSetPool {
        &mut self.fractal.descriptor_set_pool
    }
    pub fn fractal_pipeline(&self) -> Arc<GraphicsPipeline> {
        self.fractal.pipeline.clone()
    }
    pub fn instance(&self) -> &Arc<Instance> {
        &self.instance
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
        self.particles.vertex_buffers.vertex.len()
    }
    pub fn window(&self) -> &Window {
        self.surface.window()
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

impl From<&AppConfig> for AppConstants {
    fn from(app_config: &AppConfig) -> Self {
        Self {
            max_speed: app_config.max_speed,
            particle_count: app_config.particle_count as f32,
            spring_coefficient: app_config.spring_coefficient,
            point_size: app_config.point_size,
            audio_scale: app_config.audio_scale,
            vertical_fov: app_config.vertical_fov,
        }
    }
}
