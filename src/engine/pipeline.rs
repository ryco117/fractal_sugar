/*
    fractal_sugar - An experimental audio visualizer combining fractals and particle simulations.
    Copyright (C) 2022,2023  Ryan Andersen

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

use smallvec::smallvec;
use vulkano::device::Device;
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::depth_stencil::{DepthState, DepthStencilState};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::graphics::vertex_input::VertexInputState;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::render_pass::Subpass;
use vulkano::shader::ShaderModule;

use super::vertex::PointParticle;

// Create a graphics pipeline for displaying a list of particles.
pub fn create_particle(
    device: Arc<Device>,
    vert_shader: &Arc<ShaderModule>,
    frag_shader: &Arc<ShaderModule>,
    subpass: Subpass,
    viewport: Viewport,
) -> Arc<GraphicsPipeline> {
    // Needed for `PointParticle::per_vertex()` and `VertexBufferDescription::definition()` respectively.
    use vulkano::pipeline::graphics::vertex_input::{Vertex, VertexDefinition};

    // Setup relevant context for creating the pipeline from these shaders.
    let vs = vert_shader.entry_point("main").unwrap();
    let fs = frag_shader.entry_point("main").unwrap();
    let stages = smallvec![
        PipelineShaderStageCreateInfo::new(vs.clone()),
        PipelineShaderStageCreateInfo::new(fs),
    ];
    let layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
            .into_pipeline_layout_create_info(device.clone())
            .unwrap(),
    )
    .unwrap();

    GraphicsPipeline::new(
        device,
        None,
        GraphicsPipelineCreateInfo {
            stages,
            vertex_input_state: Some(
                PointParticle::per_vertex()
                    .definition(&vs.info().input_interface)
                    .unwrap(),
            ), // Describes the layout of the vertex input.
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::PointList,
                ..InputAssemblyState::default()
            }), // Indicate the type of the primitives (the default is a list of triangles).
            viewport_state: Some(ViewportState {
                viewports: smallvec![viewport],
                ..Default::default()
            }), // Set the fixed viewport.
            multisample_state: Some(MultisampleState {
                rasterization_samples: subpass.num_samples().unwrap(),
                ..Default::default()
            }), // Explicitly make this graphics pipeline use the desired multisampling.
            depth_stencil_state: Some(DepthStencilState {
                depth: Some(DepthState::simple()),
                ..DepthStencilState::default()
            }), // Explicitly enable depth testing.

            // Necessary defaults.
            rasterization_state: Some(RasterizationState::default()),
            color_blend_state: Some(ColorBlendState {
                attachments: (0..subpass.num_color_attachments())
                    .map(|_| ColorBlendAttachmentState::default())
                    .collect(),
                ..Default::default()
            }),

            // Specify the subpass that this pipeline will be used in.
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("Failed to construct particle graphics pipeline")
}

// Create a graphics pipeline for displaying fractals.
pub fn create_fractal(
    device: Arc<Device>,
    vert_shader: &Arc<ShaderModule>,
    frag_shader: &Arc<ShaderModule>,
    subpass: Subpass,
    viewport: Viewport,
) -> Arc<GraphicsPipeline> {
    // Setup relevant context for creating the pipeline from these shaders.
    let vs = vert_shader.entry_point("main").unwrap();
    let fs = frag_shader.entry_point("main").unwrap();
    let stages = smallvec![
        PipelineShaderStageCreateInfo::new(vs),
        PipelineShaderStageCreateInfo::new(fs),
    ];
    let layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
            .into_pipeline_layout_create_info(device.clone())
            .unwrap(),
    )
    .unwrap();

    GraphicsPipeline::new(
        device,
        None,
        GraphicsPipelineCreateInfo {
            stages,
            vertex_input_state: Some(VertexInputState::default()),

            // Indicate the type of the primitives (the default is a list of triangles).
            input_assembly_state: Some(InputAssemblyState {
                topology: PrimitiveTopology::TriangleStrip,
                ..InputAssemblyState::default()
            }),
            // Set the fixed viewport.
            viewport_state: Some(ViewportState {
                viewports: smallvec![viewport],
                ..Default::default()
            }),

            // Necessary defaults.
            rasterization_state: Some(RasterizationState::default()),
            multisample_state: Some(MultisampleState::default()),
            color_blend_state: Some(ColorBlendState {
                attachments: (0..subpass.num_color_attachments())
                    .map(|_| ColorBlendAttachmentState::default())
                    .collect(),
                ..Default::default()
            }),

            // Specify the subpass that this pipeline will be used in.
            subpass: Some(subpass.into()),
            ..GraphicsPipelineCreateInfo::layout(layout)
        },
    )
    .expect("Failed to construct fractal graphics pipeline")
}
