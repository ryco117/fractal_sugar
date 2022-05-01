use std::sync::Arc;

use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, ImmutableBuffer};
use vulkano::command_buffer::{
    CommandBufferExecFuture, PrimaryAutoCommandBuffer, PrimaryCommandBuffer,
};
use vulkano::device::{Device, Queue};
use vulkano::image::view::ImageView;
use vulkano::image::{AttachmentImage, SwapchainImage};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass};
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

// It's possible to remove this typedef and use a `Box`ed GpuFuture;
// however, doing so would force using dynamic dispatch of methods,
// as well as require extra code for (un)boxing
type EngineFrameFuture = Arc<
    FenceSignalFuture<
        vulkano::swapchain::PresentFuture<
            vulkano::command_buffer::CommandBufferExecFuture<
                vulkano::sync::JoinFuture<
                    Box<dyn GpuFuture>,
                    vulkano::swapchain::SwapchainAcquireFuture<Window>,
                >,
                Arc<vulkano::command_buffer::PrimaryAutoCommandBuffer>,
            >,
            Window,
        >,
    >,
>;

pub struct Engine {
    compute_pipeline: Arc<vulkano::pipeline::ComputePipeline>,
    descriptor_set: Arc<vulkano::descriptor_set::PersistentDescriptorSet>,
    device: Arc<Device>,
    fences: Vec<Option<EngineFrameFuture>>,
    frag_shader: Arc<ShaderModule>,
    framebuffers: Vec<Arc<Framebuffer>>,
    graphics_pipeline: Arc<GraphicsPipeline>,
    previous_fence_index: usize,
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    surface: Arc<Surface<Window>>,
    swapchain: EngineSwapchain,
    vert_shader: Arc<ShaderModule>,
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    viewport: Viewport,
}

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 450;

const DEBUG_VULKAN: bool = true;

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
            .with_title("rust_playground")
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
        let image_format = engine_swapchain.get_swapchain().image_format();

        // Create a frame-in-flight fence for each image buffer.
        // This allows CPU work to continue while GPU is busy with previous frames
        let frames_in_flight = engine_swapchain.get_images().len();
        let fences: Vec<Option<EngineFrameFuture>> = vec![None; frames_in_flight];

        // Load particle shaders
        let frag_shader = particle_shaders::fs::load(device.clone())
            .expect("Failed to load particle fragment shader");
        let vert_shader = particle_shaders::vs::load(device.clone())
            .expect("Failed to load particle vertex shader");
        let comp_shader = particle_shaders::cs::load(device.clone())
            .expect("Failed to load particle compute shader");

        // Create compute pipeline for particles
        let compute_pipeline = vulkano::pipeline::ComputePipeline::new(
            device.clone(),
            comp_shader.entry_point("main").unwrap(),
            &(),
            None,
            |_| {},
        )
        .expect("Failed to create compute shader");

        // Create Storage Buffers for particle info
        const PARTICLE_COUNT: usize = 1_000_000;
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
            let mut cbb = vulkano::command_buffer::AutoCommandBufferBuilder::primary(
                device.clone(),
                queue.family(),
                vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
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
            const SPACE_FILLING_CURVE_DEPTH: usize = 6;

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

        // Create a new descriptor set for binding particle Storage Buffers
        use vulkano::pipeline::Pipeline; // Required to access layout() method
        let descriptor_set = vulkano::descriptor_set::PersistentDescriptorSet::new(
            compute_pipeline
                .layout()
                .set_layouts()
                .get(0) // 0 is the index of the descriptor set layout we want
                .unwrap()
                .clone(),
            [
                vulkano::descriptor_set::WriteDescriptorSet::buffer(0, vertex_buffer.clone()), // 0 is the binding of the data in this set
                vulkano::descriptor_set::WriteDescriptorSet::buffer(
                    1,
                    fixed_position_buffer.clone(),
                ),
            ],
        )
        .unwrap();

        // Create a render pass to utilize the graphics pipeline
        // Describes the inputs/outputs but not the commands used
        /*let render_pass = vulkano::single_pass_renderpass!(device.clone(),
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    format: engine_swapchain.get_swapchain().image_format(), // Use swapchain's format since we are writing to its buffers
                    samples: 1, // No MSAA necessary when rendering a single quad with shading ;)
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        ).unwrap();*/

        // Create particle renderpass with MSAA
        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                // The first framebuffer attachment is the intermediary image
                intermediary: {
                    load: Clear,
                    store: DontCare,
                    format: image_format,
                    samples: 8, // MSAA for smooth particles
                },

                // The second framebuffer attachment is the final image
                color: {
                    load: DontCare, // Don't require two clear calls for this renderpass
                    store: Store,
                    format: image_format, // Use swapchain's format since we are writing to its buffers
                    samples: 1, // Must resolve to non-sampled image for presentation
                }
            },
            pass: {
                // When drawing, there is only one output which is the intermediary image
                color: [intermediary],
                depth_stencil: {},

                // The `resolve` array here must contain either zero entries (if you don't use
                // multisampling), or one entry per color attachment. At the end of the pass, each
                // color attachment will be *resolved* into the given image. In other words, here, at
                // the end of the pass, the `intermediary` attachment will be copied to the attachment
                // named `color`.
                resolve: [color]
            }
        )
        .unwrap();

        // Define our 2D viewspace (with normalized depth)
        let viewport = Viewport {
            origin: [0., 0.],
            dimensions: surface.window().inner_size().into(),
            depth_range: 0.0..1.,
        };

        // Create the almighty graphics pipeline!
        let pipeline = pipeline::create_particles_pipeline(
            device.clone(),
            vert_shader.clone(),
            frag_shader.clone(),
            render_pass.clone(),
            viewport.clone(),
        );

        // Create a framebuffer to store results of render pass
        let framebuffers = Self::create_particles_framebuffers(
            render_pass.clone(),
            &engine_swapchain.get_images(),
            &engine_swapchain.get_msaa_images(),
        );

        // Construct new Engine
        Self {
            compute_pipeline,
            descriptor_set,
            device,
            fences,
            frag_shader,
            framebuffers,
            graphics_pipeline: pipeline,
            previous_fence_index: 0,
            queue,
            render_pass,
            surface,
            swapchain: engine_swapchain,
            vert_shader,
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
        // Vulkan panics if both dimensions are zero, bail here instead
        if dimensions.width == 0 && dimensions.height == 0 {
            // Empty window detected, skipping swapchain recreation
            return RecreateSwapchainResult::ExtentNotSupported;
        }

        // Create new swapchain from the previous, specifying new window size
        match self.swapchain.recreate(dimensions, window_resized) {
            // Continue logic
            RecreateSwapchainResult::Success => {}

            // Return that swapchain could not be recreated (often due to a resizing error)
            RecreateSwapchainResult::ExtentNotSupported => {
                return RecreateSwapchainResult::ExtentNotSupported
            }
        }

        // Framebuffer is tied to the swapchain images, must recreate as well
        self.framebuffers = Self::create_particles_framebuffers(
            self.render_pass.clone(),
            &self.swapchain.get_images(),
            &self.swapchain.get_msaa_images(),
        );

        // If caller indicates a resize has prompted this call then adjust viewport and fixed-view pipeline
        if window_resized {
            self.viewport.dimensions = dimensions.into();

            // Since pipeline specifies viewport is fixed, entire pipeline needs to be reconstructed to account for size change
            //self.graphics_pipeline = pipeline::create_graphics_pipeline(self.device.clone(), self.vert_shader.clone(), self.frag_shader.clone(), self.render_pass.clone(), self.viewport.clone())
            self.graphics_pipeline = pipeline::create_particles_pipeline(
                self.device.clone(),
                self.vert_shader.clone(),
                self.frag_shader.clone(),
                self.render_pass.clone(),
                self.viewport.clone(),
            )
        }

        // Recreated swapchain and necessary follow-up structures without error
        RecreateSwapchainResult::Success
    }

    // Use given push constants and synchronization-primitives to render next frame in swapchain.
    // Returns whether a swapchain recreation was deemed necessary
    pub fn draw_frame(&mut self, compute_push_constants: ComputePushConstants) -> bool {
        // Acquire the index of the next image we should render to in this swapchain
        let (image_index, suboptimal, acquire_future) = match vulkano::swapchain::acquire_next_image(
            self.swapchain.get_swapchain(),
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

        // Create a one-time-submit command buffer for this push constant data
        /*let command_buffer_boi = renderer::onetime_cmdbuf_from_constant(
            self.device.clone(),
            self.queue.clone(),
            self.graphics_pipeline.clone(),
            self.framebuffers[image_index].clone(),
            push_constants
        );*/
        let colored_sugar_commands = renderer::create_particles_cmdbuf(
            self.device.clone(),
            self.queue.clone(),
            self.graphics_pipeline.clone(),
            self.framebuffers[image_index].clone(),
            self.vertex_buffer.clone(),
            self.compute_pipeline.clone(),
            self.descriptor_set.clone(),
            compute_push_constants,
        );

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
                self.swapchain.get_swapchain(),
                image_index,
            )
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
    fn create_particles_framebuffers(
        render_pass: Arc<RenderPass>,
        images: &Vec<Arc<SwapchainImage<Window>>>,
        msaa_images: &Vec<Arc<ImageView<AttachmentImage>>>,
    ) -> Vec<Arc<Framebuffer>> {
        assert_eq!(
            images.len(),
            msaa_images.len(),
            "Must have an equal number of multi-sampled and destination images"
        );

        (0..images.len())
            .map(|i| {
                // To interact with image buffers or framebuffers from shaders must create view defining how buffer will be used.
                let view = ImageView::new_default(images[i].clone()).unwrap();

                // Create framebuffer specifying underlying renderpass and image attachments
                Framebuffer::new(
                    render_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![msaa_images[i].clone(), view], // Must add specified attachments in order
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
