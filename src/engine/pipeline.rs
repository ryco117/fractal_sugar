use std::sync::Arc;

use vulkano::device::Device;
use vulkano::pipeline::graphics::depth_stencil::DepthStencilState;
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::render_pass::Subpass;
use vulkano::shader::ShaderModule;

use super::vertex::Vertex;

// Create a graphics pipeline for displaying a list of particles
pub fn create_particle(
    device: Arc<Device>,
    vert_shader: &Arc<ShaderModule>,
    frag_shader: &Arc<ShaderModule>,
    subpass: Subpass,
    viewport: Viewport,
) -> Arc<GraphicsPipeline> {
    GraphicsPipeline::start()
        // Describes the layout of the vertex input and how should it behave
        .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
        // A Vulkan shader may contain multiple entry points, so we specify which one
        .vertex_shader(vert_shader.entry_point("main").unwrap(), ())
        // Indicate the type of the primitives (the default is a list of triangles)
        .input_assembly_state(InputAssemblyState::new().topology(PrimitiveTopology::PointList))
        // Set the fixed viewport
        .viewport_state(ViewportState::viewport_fixed_scissor_irrelevant([viewport]))
        // Same as the vertex input, but this for the fragment input
        .fragment_shader(frag_shader.entry_point("main").unwrap(), ())
        // Explicitly enable depth testing
        .depth_stencil_state(DepthStencilState::simple_depth_test())
        // Specify the subpass that this pipeline will be used in
        .render_pass(subpass)
        // Now that everything is specified, we call `build`
        .build(device)
        .unwrap()
}

// Create a graphics pipeline for displaying fractals
pub fn create_fractal(
    device: Arc<Device>,
    vert_shader: &Arc<ShaderModule>,
    frag_shader: &Arc<ShaderModule>,
    subpass: Subpass,
    viewport: Viewport,
) -> Arc<GraphicsPipeline> {
    GraphicsPipeline::start()
        // A Vulkan shader may contain multiple entry points, so we specify which one
        .vertex_shader(vert_shader.entry_point("main").unwrap(), ())
        // Indicate the type of the primitives (the default is a list of triangles)
        .input_assembly_state(InputAssemblyState::new().topology(PrimitiveTopology::TriangleStrip))
        // Set the fixed viewport
        .viewport_state(ViewportState::viewport_fixed_scissor_irrelevant([viewport]))
        // Same as the vertex input, but this for the fragment input
        .fragment_shader(frag_shader.entry_point("main").unwrap(), ())
        // Specify the subpass that this pipeline will be used in
        .render_pass(subpass)
        // Now that everything is specified, we call `build`
        .build(device)
        .unwrap()
}
