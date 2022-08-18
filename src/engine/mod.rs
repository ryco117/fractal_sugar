use std::sync::Arc;

use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, ImmutableBuffer};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferExecFuture, CommandBufferUsage, CopyBufferInfoTyped,
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

use crate::color_scheme::Scheme;
use crate::my_math::{Vector2, Vector3};
use crate::space_filling_curves;
use vertex::Vertex;

type EngineFrameFuture = Arc<FenceSignalFuture<Box<dyn GpuFuture>>>;

pub struct Engine {
    compute_descriptor_set: Arc<PersistentDescriptorSet>,
    compute_pipeline: Arc<ComputePipeline>,
    device: Arc<Device>,
    fences: Vec<Option<EngineFrameFuture>>,
    fractal_frag: Arc<ShaderModule>,
    fractal_pipeline: Arc<GraphicsPipeline>,
    fractal_vert: Arc<ShaderModule>,
    framebuffers: Vec<Arc<Framebuffer>>,
    particle_descriptor_set: Arc<PersistentDescriptorSet>,
    particle_frag: Arc<ShaderModule>,
    particle_pipeline: Arc<GraphicsPipeline>,
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

const SQUARE_FILLING_CURVE_DEPTH: usize = 6;
const CUBE_FILLING_CURVE_DEPTH: usize = 4;

const PARTICLE_COUNT: usize = 1_250_000;
const PARTICLE_COUNT_F32: f32 = PARTICLE_COUNT as f32;

// Create module for the particle's shader macros
#[allow(clippy::expl_impl_clone_on_copy)]
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
            path: "shaders/particles.vert",
            types_meta: {
                use bytemuck::{Pod, Zeroable};
                #[derive(Clone, Copy, Zeroable, Pod)]
            },
        }
    }
    pub mod cs {
        vulkano_shaders::shader! {
            ty: "compute",
            path: "shaders/particles.comp"
        }
    }
}

// Export push constant types to callers
pub type ParticleComputePushConstants = particle_shaders::cs::ty::PushConstants;
pub type ParticleVertexPushConstants = particle_shaders::vs::ty::PushConstants;

// Create module for the fractal shader macros
#[allow(clippy::expl_impl_clone_on_copy)]
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
    pub fn new(event_loop: &EventLoop<()>, initial_color_scheme: Scheme) -> Self {
        // Create instance with extensions required for windowing (and optional debugging layer(s))
        let instance = Instance::new(InstanceCreateInfo {
            enabled_extensions: vulkano_win::required_extensions(),
            enabled_layers: if DEBUG_VULKAN {
                vec!["VK_LAYER_KHRONOS_validation".to_owned()]
            } else {
                vec![]
            },
            enumerate_portability: true, // Allows for non-conformant devices to be considered when searching for the best graphics device
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
                BufferUsage::transfer_src(),
                false,
                data_iter,
            )
            .expect("Failed to create test compute buffer");

            // Create device-local buffer for optimal GPU access
            let local_buffer = DeviceLocalBuffer::<[T]>::array(
                device.clone(),
                count as vulkano::DeviceSize,
                BufferUsage {
                    transfer_dst: true,
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
            cbb.copy_buffer(CopyBufferInfoTyped::buffers(
                data_source_buffer,
                local_buffer.clone(),
            ))
            .unwrap();
            let cb = cbb.build().unwrap();

            // Create future representing execution of copy-command
            let future = cb.execute(queue.clone()).unwrap();

            // Return device-local buffer with execution future (so caller can decide how best to synchronize execution)
            (local_buffer, future)
        }
        let (vertex_buffer, fixed_square_buffer, fixed_cube_buffer) = {
            // Create position data by mapping particle index to screen using a space filling curve
            let square_position_iter = (0..PARTICLE_COUNT).map(|i| {
                space_filling_curves::square::curve_to_square_n(
                    i as f32 / PARTICLE_COUNT_F32,
                    SQUARE_FILLING_CURVE_DEPTH,
                )
            });
            let cube_position_iter = (0..PARTICLE_COUNT).map(|i| {
                space_filling_curves::cube::curve_to_cube_n(
                    i as f32 / PARTICLE_COUNT_F32,
                    CUBE_FILLING_CURVE_DEPTH,
                )
            });

            // Create immutable fixed-position buffer for 2D perspective
            let (fixed_square_position_buff, fixed_square_copy_future) =
                ImmutableBuffer::from_iter(
                    square_position_iter,
                    BufferUsage::storage_buffer(),
                    queue.clone(),
                )
                .expect("Failed to create immutable fixed-position buffer");

            // Create immutable fixed-position buffer for 3D perspective
            let (fixed_cube_position_buff, fixed_cube_copy_future) = ImmutableBuffer::from_iter(
                cube_position_iter,
                BufferUsage::storage_buffer(),
                queue.clone(),
            )
            .expect("Failed to create immutable fixed-position buffer");

            // Create vertex data by re-calculating position
            let vertex_iter = (0..PARTICLE_COUNT).map(|i| Vertex {
                pos: {
                    let Vector2 { x, y } = space_filling_curves::square::curve_to_square_n(
                        i as f32 / PARTICLE_COUNT_F32,
                        SQUARE_FILLING_CURVE_DEPTH,
                    );
                    Vector3::new(x, y, 0.)
                },
                vel: Vector3::default(),
            });

            // Create position buffer
            let (vertex_buffer, vertex_future) = create_buffer(
                &device,
                &queue,
                vertex_iter,
                BufferUsage::storage_buffer() | BufferUsage::vertex_buffer(),
            );

            // Wait for all futures to finish before continuing
            fixed_square_copy_future
                .join(fixed_cube_copy_future)
                .join(vertex_future)
                .then_signal_fence_and_flush()
                .unwrap()
                .wait(None)
                .unwrap();

            (
                vertex_buffer,
                fixed_square_position_buff,
                fixed_cube_position_buff,
            )
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

        // Create a new descriptor set for binding particle storage buffers
        // Required to access layout() method
        let compute_descriptor_set = PersistentDescriptorSet::new(
            compute_pipeline
                .layout()
                .set_layouts()
                .get(0) // 0 is the index of the descriptor set layout we want
                .unwrap()
                .clone(),
            [
                WriteDescriptorSet::buffer(0, vertex_buffer.clone()), // 0 is the binding of the data in this set
                WriteDescriptorSet::buffer(1, fixed_square_buffer),
                WriteDescriptorSet::buffer(2, fixed_cube_buffer),
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
        .unwrap();

        // Define our 2D viewspace (with normalized depth)
        let dimensions = surface.window().inner_size();
        let viewport = Viewport {
            origin: [0., 0.],
            dimensions: dimensions.into(),
            depth_range: 0.0..1.,
        };

        // Create the almighty graphics pipelines
        let particle_pipeline = pipeline::create_particle(
            device.clone(),
            &particle_vert,
            &particle_frag,
            Subpass::from(render_pass.clone(), 0).unwrap(),
            viewport.clone(),
        );
        let fractal_pipeline = pipeline::create_fractal(
            device.clone(),
            &fractal_vert,
            &fractal_frag,
            Subpass::from(render_pass.clone(), 1).unwrap(),
            viewport.clone(),
        );

        // Particle color schemes?!
        let color_scheme_buffer = CpuAccessibleBuffer::from_data(
            device.clone(),
            BufferUsage::uniform_buffer(),
            false,
            initial_color_scheme,
        )
        .unwrap();
        let particle_descriptor_set = PersistentDescriptorSet::new(
            particle_pipeline
                .layout()
                .set_layouts()
                .get(0)
                .unwrap()
                .clone(),
            [WriteDescriptorSet::buffer(0, color_scheme_buffer.clone())],
        )
        .unwrap();

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
            compute_descriptor_set,
            compute_pipeline,
            device,
            fences,
            fractal_frag,
            fractal_pipeline,
            fractal_vert,
            framebuffers,
            particle_descriptor_set,
            particle_frag,
            particle_pipeline,
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
            self.particle_pipeline = pipeline::create_particle(
                self.device.clone(),
                &self.particle_vert,
                &self.particle_frag,
                Subpass::from(self.render_pass.clone(), 0).unwrap(),
                self.viewport.clone(),
            );
            self.fractal_pipeline = pipeline::create_fractal(
                self.device.clone(),
                &self.fractal_vert,
                &self.fractal_frag,
                Subpass::from(self.render_pass.clone(), 1).unwrap(),
                self.viewport.clone(),
            );
        }

        // Recreated swapchain and necessary follow-up structures without error
        RecreateSwapchainResult::Success
    }

    // Use given push constants and synchronization-primitives to render next frame in swapchain.
    // Returns whether a swapchain recreation was deemed necessary
    pub fn draw_frame(
        &mut self,
        particle_data: Option<(ParticleComputePushConstants, ParticleVertexPushConstants)>,
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
        let colored_sugar_commands = renderer::create_render_commands(
            self,
            &self.framebuffers[image_index],
            particle_data,
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
                            ..ImageUsage::none()
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

    pub fn update_color_scheme(&mut self, scheme: Scheme) -> () {
        let color_scheme_buffer = CpuAccessibleBuffer::from_data(
            self.device.clone(),
            BufferUsage::uniform_buffer(),
            false,
            scheme,
        )
        .unwrap();

        self.particle_descriptor_set = PersistentDescriptorSet::new(
            self.particle_pipeline
                .layout()
                .set_layouts()
                .get(0)
                .unwrap()
                .clone(),
            [WriteDescriptorSet::buffer(0, color_scheme_buffer.clone())],
        )
        .unwrap();
    }

    // Engine getters
    pub fn compute_pipeline(&self) -> Arc<ComputePipeline> {
        self.compute_pipeline.clone()
    }
    pub fn device(&self) -> Arc<Device> {
        self.device.clone()
    }
    pub fn compute_descriptor_set(&self) -> Arc<PersistentDescriptorSet> {
        self.compute_descriptor_set.clone()
    }
    pub fn fractal_pipeline(&self) -> Arc<GraphicsPipeline> {
        self.fractal_pipeline.clone()
    }
    pub fn particle_descriptor_set(&self) -> Arc<PersistentDescriptorSet> {
        self.particle_descriptor_set.clone()
    }
    pub fn particle_pipeline(&self) -> Arc<GraphicsPipeline> {
        self.particle_pipeline.clone()
    }
    pub fn queue(&self) -> Arc<Queue> {
        self.queue.clone()
    }
    pub fn surface(&self) -> Arc<Surface<Window>> {
        self.surface.clone()
    }
    pub fn vertex_count(&self) -> u64 {
        use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
        self.vertex_buffer.len()
    }
}
