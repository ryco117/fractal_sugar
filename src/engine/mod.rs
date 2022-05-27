use std::sync::Arc;

use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, ImmutableBuffer};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferExecFuture, CommandBufferUsage,
    PrimaryAutoCommandBuffer, PrimaryCommandBuffer,
};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::image::view::ImageView;
use vulkano::image::{AttachmentImage, ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, Pipeline};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::shader::ShaderModule;
use vulkano::swapchain::{PresentMode, Surface};
use vulkano::sync::{FenceSignalFuture, GpuFuture, NowFuture};
use vulkano_win::VkSurfaceBuild;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

use swapchain::{EngineSwapchain, RecreateSwapchainResult};

mod hardware;
pub mod pipeline;
pub mod renderer;
pub mod swapchain;
mod vertex;

use crate::my_math::Vector2;
use crate::space_filling_curves;
use vertex::Vertex;

type EngineFrameFuture = Arc<FenceSignalFuture<Box<dyn GpuFuture>>>;

pub struct Engine {
    compute_pipeline: Arc<vulkano::pipeline::ComputePipeline>,
    descriptor_set: Arc<vulkano::descriptor_set::PersistentDescriptorSet>,
    device: Arc<Device>,
    fences: Vec<Option<EngineFrameFuture>>,
    fractal_frag: Arc<ShaderModule>,
    fractal_pipeline: Arc<GraphicsPipeline>,
    fractal_vert: Arc<ShaderModule>,
    framebuffers: Vec<Arc<Framebuffer>>,
    particles_pipeline: Arc<GraphicsPipeline>,
    particle_frag: Arc<ShaderModule>,
    particle_vert: Arc<ShaderModule>,
    previous_fence_index: usize,
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    viewport: Viewport,
}

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 450;

const DEBUG_VULKAN: bool = false;

const SPACE_FILLING_CURVE_DEPTH: usize = 6;

// Create module for the particle's shader macros
mod particle_shaders {
    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/particles.frag"
        }
    }
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/particles.vert"
        }
    }
    pub mod cs {
        vulkano_shaders::shader! {
            ty: "compute",
            path: "shaders/particles.comp"
        }
    }
}

// Export Push Constant types to callers
pub type ComputePushConstants = particle_shaders::cs::ty::PushConstants;

// Create module for the fractal shader macros
mod fractal_shaders {
    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/ray_march.frag"
        }
    }
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/entire_view.vert"
        }
    }
}

// Export Push Constant types to callers
pub type FractalPushConstants = fractal_shaders::fs::ty::PushConstants;

impl Engine {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
        // Create instance with extensions required for windowing (and optional debugging extension(s) and layer(s))
        let required_extensions = vulkano::instance::InstanceExtensions {
            ext_debug_utils: DEBUG_VULKAN,
            ..vulkano_win::required_extensions()
        };
        let instance = Instance::new(InstanceCreateInfo {
            enabled_extensions: required_extensions,
            enabled_layers: if DEBUG_VULKAN {
                vec!["VK_LAYER_KHRONOS_validation".to_owned()]
            } else {
                vec![]
            },
            ..Default::default()
        })
        .expect("Failed to create Vulkan instance");

        // Create a window! Set some basic window properties and get a vulkan surface
        let surface = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
            .with_title("FractalSugar")
            .build_vk_surface(event_loop, instance.clone())
            .unwrap();

        // Fetch device resources based on what is available to the system
        let (physical_device, device, queue) = hardware::select_hardware(&instance, &surface);

        // Create swapchain and associated image buffers from the relevant parameters
        let engine_swapchain = EngineSwapchain::new(
            physical_device,
            device.clone(),
            surface.clone(),
            PresentMode::Fifo,
        );
        let image_format = engine_swapchain.swapchain().image_format();

        // Create a frame-in-flight fence for each image buffer.
        // This allows CPU work to continue while GPU is busy with previous frames
        let frames_in_flight = engine_swapchain.images().len();
        let fences: Vec<Option<EngineFrameFuture>> = vec![None; frames_in_flight];

        // Load fractal shaders
        let fractal_frag = fractal_shaders::fs::load(device.clone())
            .expect("Failed to load fractal fragment shader");
        let fractal_vert = fractal_shaders::vs::load(device.clone())
            .expect("Failed to load fractal vertex shader");

        // Load particle shaders
        let particle_frag = particle_shaders::fs::load(device.clone())
            .expect("Failed to load particle fragment shader");
        let particle_vert = particle_shaders::vs::load(device.clone())
            .expect("Failed to load particle vertex shader");
        let comp_shader = particle_shaders::cs::load(device.clone())
            .expect("Failed to load particle compute shader");

        // Create Storage Buffers for particle info
        const PARTICLE_COUNT: usize = 1_250_000;
        const PARTICLE_COUNT_F32: f32 = PARTICLE_COUNT as f32;
        fn create_buffer<T, I>(
            device: &Arc<Device>,
            queue: &Arc<Queue>,
            data_iter: I,
            usage: BufferUsage,
        ) -> (
            Arc<DeviceLocalBuffer<[T]>>,
            CommandBufferExecFuture<NowFuture, PrimaryAutoCommandBuffer>,
        )
        where
            [T]: vulkano::buffer::BufferContents,
            I: ExactSizeIterator<Item = T>,
        {
            let count = data_iter.len();

            // Create simple buffer type that we can write data to
            let data_source_buffer = CpuAccessibleBuffer::from_iter(
                device.clone(),
                BufferUsage::transfer_source(),
                false,
                data_iter,
            )
            .expect("Failed to create test compute buffer");

            // Create device-local buffer for optimal GPU access
            let local_buffer = DeviceLocalBuffer::<[T]>::array(
                device.clone(),
                count as vulkano::DeviceSize,
                BufferUsage {
                    transfer_destination: true,
                    ..usage
                },
                device.active_queue_families(),
            )
            .expect("Failed to create immutable fixed-position buffer");

            // Create buffer copy command
            let mut cbb = AutoCommandBufferBuilder::primary(
                device.clone(),
                queue.family(),
                CommandBufferUsage::OneTimeSubmit,
            )
            .unwrap();
            cbb.copy_buffer(data_source_buffer, local_buffer.clone())
                .unwrap();
            let cb = cbb.build().unwrap();

            // Create future representing execution of copy-command
            let future = cb.execute(queue.clone()).unwrap();

            // Return device-local buffer with execution future (so caller can decide how best to synchronize execution)
            (local_buffer, future)
        }
        let (vertex_buffer, fixed_position_buffer) = {
            // Create position data by mapping particle index to screen using a space filling curve
            let position_iter = (0..PARTICLE_COUNT).map(|i| {
                space_filling_curves::square::curve_to_square_n(
                    i as f32 / PARTICLE_COUNT_F32,
                    SPACE_FILLING_CURVE_DEPTH,
                )
            });

            // Create immutable fixed-position buffer
            let (fixed_position_buff, fixed_pos_copy_future) = ImmutableBuffer::from_iter(
                position_iter,
                BufferUsage::storage_buffer(),
                queue.clone(),
            )
            .expect("Failed to create immutable fixed-position buffer");

            // Create vertex data by re-calculating
            let vertex_iter = (0..PARTICLE_COUNT).map(|i| Vertex {
                pos: space_filling_curves::square::curve_to_square_n(
                    i as f32 / PARTICLE_COUNT_F32,
                    SPACE_FILLING_CURVE_DEPTH,
                ),
                vel: Vector2::new(0., 0.),
            });

            // Create position buffer
            let (vertex_buffer, vertex_future) = create_buffer(
                &device,
                &queue,
                vertex_iter.clone(),
                BufferUsage::storage_buffer() | BufferUsage::vertex_buffer(),
            );

            // Wait for all futures to finish before continuing
            fixed_pos_copy_future
                .join(vertex_future)
                .then_signal_fence_and_flush()
                .unwrap()
                .wait(None)
                .unwrap();

            (vertex_buffer, fixed_position_buff)
        };

        // Create compute pipeline for particles
        let compute_pipeline = ComputePipeline::new(
            device.clone(),
            comp_shader.entry_point("main").unwrap(),
            &(),
            None,
            |_| {},
        )
        .expect("Failed to create compute shader");

        // Create a new descriptor set for binding particle Storage Buffers
        // Required to access layout() method
        let descriptor_set = PersistentDescriptorSet::new(
            compute_pipeline
                .layout()
                .set_layouts()
                .get(0) // 0 is the index of the descriptor set layout we want
                .unwrap()
                .clone(),
            [
                WriteDescriptorSet::buffer(0, vertex_buffer.clone()), // 0 is the binding of the data in this set
                WriteDescriptorSet::buffer(1, fixed_position_buffer.clone()),
            ],
        )
        .unwrap();

        let render_pass = vulkano::ordered_passes_renderpass!(
            device.clone(),
            attachments: {
                // The first framebuffer attachment is the intermediary image
                intermediary: {
                    load: Clear,
                    store: DontCare,
                    format: image_format,
                    samples: 8, // MSAA for smooth particles
                },

                particle_color: {
                    load: DontCare, // Resolve does not need destination image to be cleared
                    store: DontCare,
                    format: image_format, // Use swapchain's format since we are writing to its buffers
                    samples: 1, // Must resolve to non-sampled image for presentation
                },

                fractal_color: {
                    load: DontCare,
                    store: Store,
                    format: image_format, // Use swapchain's format since we are writing to its buffers
                    samples: 1, // No MSAA necessary when rendering a single quad with shading ;)
                }
            },
            passes: [
                // Particles pass
                {
                    color: [intermediary],
                    depth_stencil: {},
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
                    input: [particle_color]
                }
            ]
        )
        .unwrap();

        // Define our 2D viewspace (with normalized depth)
        let dimensions = surface.window().inner_size();
        let viewport = Viewport {
            origin: [0., 0.],
            dimensions: dimensions.into(),
            depth_range: 0.0..1.,
        };

        // Create the almighty graphics pipelines
        let particles_pipeline = pipeline::create_particles_pipeline(
            device.clone(),
            particle_vert.clone(),
            particle_frag.clone(),
            Subpass::from(render_pass.clone(), 0).unwrap(),
            viewport.clone(),
        );
        let fractal_pipeline = pipeline::create_fractal_pipeline(
            device.clone(),
            fractal_vert.clone(),
            fractal_frag.clone(),
            Subpass::from(render_pass.clone(), 1).unwrap(),
            viewport.clone(),
        );

        // Create a framebuffer to store results of render pass
        let framebuffers = Self::create_framebuffers(
            &device,
            &render_pass,
            dimensions.into(),
            &engine_swapchain.images(),
            image_format,
        );

        // Construct new Engine
        Self {
            compute_pipeline,
            descriptor_set,
            device,
            fences,
            fractal_frag,
            fractal_pipeline,
            fractal_vert,
            framebuffers,
            particles_pipeline,
            particle_frag,
            particle_vert,
            previous_fence_index: 0,
            queue,
            render_pass,
            surface,
            swapchain: engine_swapchain,
            vertex_buffer,
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
        self.framebuffers = Self::create_framebuffers(
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
            self.particles_pipeline = pipeline::create_particles_pipeline(
                self.device.clone(),
                self.particle_vert.clone(),
                self.particle_frag.clone(),
                Subpass::from(self.render_pass.clone(), 0).unwrap(),
                self.viewport.clone(),
            );
            self.fractal_pipeline = pipeline::create_fractal_pipeline(
                self.device.clone(),
                self.fractal_vert.clone(),
                self.fractal_frag.clone(),
                Subpass::from(self.render_pass.clone(), 1).unwrap(),
                self.viewport.clone(),
            )
        }

        // Recreated swapchain and necessary follow-up structures without error
        RecreateSwapchainResult::Success
    }

    // Use given push constants and synchronization-primitives to render next frame in swapchain.
    // Returns whether a swapchain recreation was deemed necessary
    pub fn draw_frame(
        &mut self,
        compute_data: Option<ComputePushConstants>,
        fractal_data: FractalPushConstants,
    ) -> bool {
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
            image_fence.cleanup_finished()
        }

        // If the previous image has a fence, use it for synchronization, else create a new one
        let previous_future = match self.fences[self.previous_fence_index].clone() {
            // Ensure current frame is synchronized with previous
            Some(fence) => fence.boxed(),

            // Create new future to guarentee synchronization with (fake) previous frame
            None => vulkano::sync::now(self.device.clone()).boxed(),
        };

        // Create a one-time-submit command buffer for this frame
        let colored_sugar_commands = renderer::create_render_commands(
            self.device.clone(),
            self.queue.clone(),
            self.compute_pipeline.clone(),
            self.particles_pipeline.clone(),
            self.fractal_pipeline.clone(),
            self.framebuffers[image_index].clone(),
            self.descriptor_set.clone(),
            self.vertex_buffer.clone(),
            compute_data,
            fractal_data,
        );

        // Create synchronization future for rendering the current frame
        let future = previous_future
            // Wait for previous and current futures to synchronize
            .join(acquire_future)
            // Execute the one-time command buffer
            .then_execute(self.queue.clone(), colored_sugar_commands)
            .unwrap()
            // Present result to swapchain buffer
            .then_swapchain_present(self.queue.clone(), self.swapchain.swapchain(), image_index)
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
                // To interact with image buffers or framebuffers from shaders must create view defining how image will be used.
                // This view, which belongs to the swapchain, will be the destination view
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
                            transfer_destination: true,
                            input_attachment: true,
                            color_attachment: true,
                            ..ImageUsage::none()
                        },
                    )
                    .unwrap(),
                )
                .unwrap();

                // Create framebuffer specifying underlying renderpass and image attachments
                Framebuffer::new(
                    render_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![msaa_view, particle_view, view], // Must add specified attachments in order
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect()
    }

    // Engine getters
    pub fn get_surface(&self) -> Arc<Surface<Window>> {
        self.surface.clone()
    }
}
