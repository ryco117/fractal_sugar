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

use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, PrimaryAutoCommandBuffer, RenderPassBeginInfo,
    SubpassContents,
};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::format::ClearValue;
use vulkano::image::ImageViewAbstract;
use vulkano::pipeline::{GraphicsPipeline, Pipeline, PipelineBindPoint};
use vulkano::render_pass::Framebuffer;

use super::vertex::Vertex;
use super::{DrawData, Engine, FractalPushConstants, ParticleVertexPushConstants};

// Helper for initializing the rendering of a frame. Must specify clear value of each subpass
fn begin_render_pass(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    framebuffer: &Arc<Framebuffer>,
) {
    builder
        .begin_render_pass(
            RenderPassBeginInfo {
                clear_values: vec![
                    Some([0., 0., 0., 1.].into()),
                    None,
                    Some(ClearValue::Depth(1.)),
                    None,
                ], // Clear values for attachments
                ..RenderPassBeginInfo::framebuffer(framebuffer.clone())
            },
            SubpassContents::Inline, // Use secondary command buffers to specify later passses
        )
        .unwrap();
}

pub fn create_render_commands(
    engine: &mut Engine,
    framebuffer: &Arc<Framebuffer>,
    draw_data: &DrawData,
) -> PrimaryAutoCommandBuffer {
    // Regular ol' single submit buffer
    let mut builder = AutoCommandBufferBuilder::primary(
        &engine.allocators.command_buffer,
        engine.queue().queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Allow toggling of particle effects and avoid unnecesary computation
    if let Some((compute_push_constants, vertex_push_constants)) = draw_data.particle_data {
        let compute_pipeline = engine.compute_pipeline();
        let descriptor_set = engine.compute_descriptor_set();
        let vertex_buffer = engine.particles.vertex_buffers.vertex.clone();
        let buffer_count = engine.particle_count() as u32;

        // Build compute commands
        builder
            // Push constants for compute shader
            .push_constants(compute_pipeline.layout().clone(), 0, compute_push_constants)
            // Perform compute operation to update particle positions
            .bind_pipeline_compute(compute_pipeline.clone())
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                compute_pipeline.layout().clone(),
                0, // Start binding descriptor sets at index 0
                descriptor_set,
            )
            .dispatch([buffer_count / 128, 1, 1])
            .unwrap();

        // Start render pass
        begin_render_pass(&mut builder, framebuffer);

        // Add inline commands to render particles
        inline_particles_cmds(
            &mut builder,
            engine.particle_pipeline(),
            &vertex_buffer,
            vertex_push_constants,
            engine.particle_descriptor_set(),
        );
    } else {
        // Begin the same render pass as with particles, but skip commands to draw particles
        begin_render_pass(&mut builder, framebuffer);
    }

    // Move to next subpass
    builder.next_subpass(SubpassContents::Inline).unwrap();

    // Add inline commands to render fractal
    inline_fractal_cmds(
        &mut builder,
        engine,
        draw_data.fractal_data,
        framebuffer.attachments()[1].clone(),
        framebuffer.attachments()[2].clone(),
    );

    // Mark completion of frame rendering (for this pass)
    builder.end_render_pass().unwrap();

    // Return new command buffer for this framebuffer
    builder.build().unwrap()
}

fn inline_particles_cmds(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipeline: Arc<GraphicsPipeline>,
    vertex_buffer: &Arc<DeviceLocalBuffer<[Vertex]>>,
    push_constants: ParticleVertexPushConstants,
    descriptor_set: Arc<PersistentDescriptorSet>,
) {
    use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
    let buffer_count = vertex_buffer.len() as u32;
    let layout = pipeline.layout().clone();

    // Build render pass commands
    builder
        // Draw particles
        .bind_pipeline_graphics(pipeline)
        .push_constants(layout.clone(), 0, push_constants)
        .bind_vertex_buffers(0, vertex_buffer.clone())
        .bind_descriptor_sets(PipelineBindPoint::Graphics, layout, 0, descriptor_set)
        .draw(buffer_count, 1, 0, 0)
        .expect("Failed to draw particle subpass");
}

fn inline_fractal_cmds(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    engine: &mut Engine,
    push_constants: FractalPushConstants,
    particle_input: Arc<dyn ImageViewAbstract>,
    particle_depth: Arc<dyn ImageViewAbstract>,
) {
    let app_constants = engine.app_constants.buffer.clone();

    let pipeline = engine.fractal_pipeline();
    let layout = pipeline.layout().clone();
    let descriptor_set = PersistentDescriptorSet::new(
        engine.descriptor_pool(),
        layout
            .set_layouts()
            .get(0) // 0 is the index of the descriptor set layout we want
            .unwrap()
            .clone(),
        [
            WriteDescriptorSet::image_view(0, particle_input),
            WriteDescriptorSet::image_view(1, particle_depth),
            WriteDescriptorSet::buffer(2, app_constants),
        ],
    )
    .unwrap();

    // Build render pass commands
    builder
        .bind_pipeline_graphics(pipeline)
        // Push constants
        .push_constants(layout.clone(), 0, push_constants)
        .bind_descriptor_sets(PipelineBindPoint::Graphics, layout, 0, descriptor_set)
        // Draw 4 static vertices (entire view quad)
        .draw(4, 1, 0, 0)
        .expect("Failed to draw fractal subpass");
}
