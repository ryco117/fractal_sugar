use std::sync::Arc;

use vulkano::device::{Device, Queue};
use vulkano::image::SwapchainImage;
use vulkano::image::view::ImageView;
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::render_pass::{RenderPass, Framebuffer, FramebufferCreateInfo};
use vulkano::shader::ShaderModule;
use vulkano::swapchain::{PresentMode, Surface};
use vulkano::sync::{FenceSignalFuture, GpuFuture};
use vulkano_win::VkSurfaceBuild;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

use swapchain::{EngineSwapchain, RecreateSwapchainResult};

mod hardware;
pub mod swapchain;
pub mod pipeline;
pub mod renderer;

type EngineFrameFuture = FenceSignalFuture<vulkano::swapchain::PresentFuture<
    vulkano::command_buffer::CommandBufferExecFuture<
        vulkano::sync::JoinFuture<Box<dyn GpuFuture>, vulkano::swapchain::SwapchainAcquireFuture<Window>>,
        Arc<vulkano::command_buffer::PrimaryAutoCommandBuffer>>,
    Window>>;

pub struct Engine {
    device: Arc<Device>,
    fences: Vec<Option<Arc<EngineFrameFuture>>>,
    frag_shader: Arc<ShaderModule>,
    framebuffers: Vec<Arc<Framebuffer>>,
    graphics_pipeline: Arc<GraphicsPipeline>,
    previous_fence_index: usize,
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    surface: Arc<Surface<Window>>,
    pub swapchain: EngineSwapchain,
    vert_shader: Arc<ShaderModule>,
    viewport: Viewport
}

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 450;

impl Engine {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
        // Create instance with extensions required for windowing
        let required_extensions = vulkano_win::required_extensions();
        let instance = Instance::new(InstanceCreateInfo {
            enabled_extensions: required_extensions,
            ..Default::default()})
            .expect("Failed to create Vulkan instance");

        // Create a window! Set some basic window properties and get a vulkan surface
        let surface = WindowBuilder::new()
            .with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
            .with_title("rust_playground")
            .build_vk_surface(event_loop, instance.clone())
            .unwrap();

        // Fetch device resources based on what is available to the system
        let (physical_device, device, queue) = hardware::select_hardware(&instance, &surface);

        // Create swapchain and associated image buffers from the relevant 
        let engine_swapchain = EngineSwapchain::new(physical_device, device.clone(), surface.clone(), PresentMode::Fifo);

        // vulkano-shaders wasn't working effortlessly for my cross-compiling needs.
        // Decided it was easier to implement this closure and continue with a minimal cross-compile
        let load_shader_bytes = |path: &str| -> Arc<ShaderModule> {
            let bytes = std::fs::read(path).expect("Failed to read bytes from compiled shader");
            assert_eq!(bytes.len() % 4, 0, "SPIR-V shader must have a byte-length which is a multiple of 4");
            unsafe { ShaderModule::from_bytes(device.clone(), &bytes).unwrap() }
        };

        // Load compiled graphics shaders into vulkan
        let frag_shader = load_shader_bytes("shaders/spirv/iq_mandelbrot.frag.spv");
        let vert_shader = load_shader_bytes("shaders/spirv/entire_view.vert.spv");

        // Create a render pass to utilize the graphics pipeline
        // Describes the inputs/outputs but not the commands used
        let render_pass = vulkano::single_pass_renderpass!(device.clone(),
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
        ).unwrap();

        // Define our 2D viewspace (with normalized depth)
        let viewport = Viewport {
            origin: [0., 0.],
            dimensions: surface.window().inner_size().into(),
            depth_range: 0. ..1.,
        };

        // Create the almighty graphics pipeline!
        let pipeline = pipeline::create_graphics_pipeline(device.clone(), vert_shader.clone(), frag_shader.clone(), render_pass.clone(), viewport.clone());

        // Create a framebuffer to store results of render pass
        let framebuffers = Engine::create_framebuffers(render_pass.clone(), engine_swapchain.get_images());

        // Create a frame-in-flight fence for each image buffer.
        // This allows CPU work to continue while GPU is busy with previous frames
        let frames_in_flight = engine_swapchain.get_images().len();
        let fences: Vec<Option<Arc<FenceSignalFuture<_>>>> = vec![None; frames_in_flight];

        // Construct new Engine
        Engine {
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
            viewport
        }
    }

    // Recreate swapchain and necessary follow-up structures (often for window resizing)
    pub fn recreate_swapchain(&mut self, dimensions: PhysicalSize<u32>, window_resized: bool) -> RecreateSwapchainResult {
        // Vulkan panics if both dimensions are zero, bail here instead
        if dimensions.width == 0 && dimensions.height == 0 {
            // Empty window detected, skipping swapchain recreation
            return RecreateSwapchainResult::ExtentNotSupported
        }

        // Create new swapchain from the previous, specifying new window size
        match self.swapchain.recreate(dimensions) {
            // Continue logic
            RecreateSwapchainResult::Success => {}

            // Return that swapchain could not be recreated (often due to a resizing error)
            RecreateSwapchainResult::ExtentNotSupported => return RecreateSwapchainResult::ExtentNotSupported,

            // Return that recreation failed for an unexpected reason
            err => return err
        }

        // Framebuffer is tied to the swapchain images, must recreate as well
        self.framebuffers = Engine::create_framebuffers(self.render_pass.clone(), self.swapchain.get_images());

        // If caller indicates a resize has prompted this call then adjust viewport and fixed-view pipeline
        if window_resized {
            self.viewport.dimensions = dimensions.into();

            // Since pipeline specifies viewport is fixed, entire pipeline needs to be reconstructed to account for size change
            self.graphics_pipeline = pipeline::create_graphics_pipeline(self.device.clone(), self.vert_shader.clone(), self.frag_shader.clone(), self.render_pass.clone(), self.viewport.clone())
        }

        // Recreated swapchain and necessary follow-up structures without error
        RecreateSwapchainResult::Success
    }

    // Use given push constants and synchronization-primitives to render next frame in swapchain.
    // Returns whether a swapchain recreation was deemed necessary
    pub fn draw_frame(&mut self, push_constants: renderer::PushConstantData) -> bool {
        // Acquire the index of the next image we should render to in this swapchain
        let (image_index, suboptimal, acquire_future) = match vulkano::swapchain::acquire_next_image(self.swapchain.get_swapchain(), None /*timeout*/) {
            Ok(tuple) => tuple,
            Err(vulkano::swapchain::AcquireError::OutOfDate) => {
                return true
            }
            Err(e) => panic!("Failed to acquire next image: {:?}", e)
        };

        if suboptimal {
            // Acquired image will still work for rendering this frame, so we will continue.
            // However, the surface's properties no longer match the swapchain's so we will recreate next chance
            return true
        }

        // If this image buffer already has a fence, wait for the fence to be ready
        if let Some(image_fence) = &self.fences[image_index] {
            image_fence.wait(None).unwrap()
        }

        // If the previous image has a fence, use it for synchronization, else create a new one
        let previous_future = match self.fences[self.previous_fence_index].clone() {
            // Create new future to assert synchronization with previous frame
            None => {
                let mut now = vulkano::sync::now(self.device.clone());

                // Manually free all not used resources (which could still be there because of an error) https://vulkano.rs/guide/windowing/event-handling
                now.cleanup_finished();

                // Box value to heap to account for the different sizes of different future types
                now.boxed()
            }

            // Use existing fence
            Some(fence) => fence.boxed()
        };
        
        // Create a one-time-submit command buffer for this push constant data
        let command_buffer_boi = renderer::onetime_cmdbuf_from_constant(
            self.device.clone(),
            self.queue.clone(),
            self.graphics_pipeline.clone(),
            self.framebuffers[image_index].clone(),
            push_constants);

        // Create synchronization future for rendering the current frame
        let future = previous_future
            // Wait for previous and current futures to synchronize
            .join(acquire_future)

            // Execute the one-time command buffer
            .then_execute(self.queue.clone(), command_buffer_boi).unwrap()

            // Present result to swapchain buffer
            .then_swapchain_present(self.queue.clone(), self.swapchain.get_swapchain(), image_index)

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
            Err(e) => {
                println!("Failed to flush future: {:?}", e);
                None
            }
        };

        // Update the last rendered index to be this frame
        self.previous_fence_index = image_index;

        // Return whether a swapchain recreation was deemed necessary
        requires_recreate_swapchain
    }

    // Helper for (re)creating framebuffers for storing results of 
    fn create_framebuffers(render_pass: Arc<RenderPass>, images: Vec<Arc<SwapchainImage<Window>>>) -> Vec<Arc<Framebuffer>> {
        images
        .iter()
        .map(|image| {
            // To interact with image buffers or framebuffers from shaders must create view defining how buffer will be used.
            let view = ImageView::new_default(image.clone()).unwrap();

            // Create framebuffer specifying underlying renderpass and view
            Framebuffer::new(
                render_pass.clone(),
                FramebufferCreateInfo {
                    attachments: vec![view],
                    ..Default::default()
                },
            ).unwrap()
        })
        .collect::<Vec<_>>()
    }

    // Engine getters
    pub fn get_surface(&self) -> Arc<Surface<Window>> { self.surface.clone() }
}