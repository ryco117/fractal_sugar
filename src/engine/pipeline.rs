/*
    fractal_sugar - An experimental audio-visualizer combining fractals and particle simulations.
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

use vulkano::device::Device;
use vulkano::pipeline::graphics::depth_stencil::DepthStencilState;
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::multisample::MultisampleState;
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
    match GraphicsPipeline::start()
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
        // Explicitly make this graphics pipeline use desired multisampling
        .multisample_state(MultisampleState {
            rasterization_samples: subpass.num_samples().unwrap(),
            ..Default::default()
        })
        // Specify the subpass that this pipeline will be used in
        .render_pass(subpass)
        // Now that everything is specified, we call `build`
        .build(device)
    {
        Ok(pipeline) => pipeline,
        Err(e) => panic!("Failed to construct particle graphics pipeline: {:?}", e),
    }
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
