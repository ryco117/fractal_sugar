use std::sync::Arc;

use vulkano::device::Device;
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::render_pass::{RenderPass, Subpass};
use vulkano::shader::ShaderModule;

use crate::my_math::Vector2;

vulkano::impl_vertex!(Vector2, x, y);

// Retrieve basic graphics pipeline
pub fn create_particles_pipeline(
    device: Arc<Device>,
    vert_shader: Arc<ShaderModule>,
    frag_shader: Arc<ShaderModule>,
    render_pass: Arc<RenderPass>,
    viewport: Viewport
) -> Arc<GraphicsPipeline> {
    GraphicsPipeline::start()
        // Describes the layout of the vertex input and how should it behave
        .vertex_input_state(BuffersDefinition::new().vertex::<Vector2>())

        // A Vulkan shader may contain multiple entry points, so we specify which one.
        .vertex_shader(vert_shader.entry_point("main").unwrap(), ())

        // Indicate the type of the primitives (the default is a list of triangles)
        .input_assembly_state(InputAssemblyState::new().topology(PrimitiveTopology::PointList))

        // Set the fixed viewport
        .viewport_state(ViewportState::viewport_fixed_scissor_irrelevant([viewport]))

        // Same as the vertex input, but this for the fragment input
        .fragment_shader(frag_shader.entry_point("main").unwrap(), ())

        // This graphics pipeline object concerns the first pass of the render pass.
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())

        // Now that everything is specified, we call `build`.
        .build(device.clone()).unwrap()
}

// Retrieve basic graphics pipeline
/*pub fn create_graphics_pipeline(
    device: Arc<Device>,
    vert_shader: Arc<ShaderModule>,
    frag_shader: Arc<ShaderModule>,
    render_pass: Arc<RenderPass>,
    viewport: Viewport
) -> Arc<GraphicsPipeline> {
    GraphicsPipeline::start()
        // A Vulkan shader may contain multiple entry points, so we specify which one.
        .vertex_shader(vert_shader.entry_point("main").unwrap(), ())

        // Indicate the type of the primitives (the default is a list of triangles)
        .input_assembly_state(InputAssemblyState::new().topology(PrimitiveTopology::TriangleStrip))

        // Set the fixed viewport
        .viewport_state(ViewportState::viewport_fixed_scissor_irrelevant([viewport]))

        // Same as the vertex input, but this for the fragment input
        .fragment_shader(frag_shader.entry_point("main").unwrap(), ())

        // This graphics pipeline object concerns the first pass of the render pass.
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())

        // Now that everything is specified, we call `build`.
        .build(device.clone()).unwrap()
}*/