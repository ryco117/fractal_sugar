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
use super::Engine;
use super::{FractalPushConstants, ParticleComputePushConstants, ParticleVertexPushConstants};

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
    engine: &Engine,
    framebuffer: &Arc<Framebuffer>,
    particle_data: Option<(ParticleComputePushConstants, ParticleVertexPushConstants)>,
    fractal_data: FractalPushConstants,
) -> PrimaryAutoCommandBuffer {
    // Regular ol' single submit buffer
    let mut builder = AutoCommandBufferBuilder::primary(
        engine.device(),
        engine.queue().family(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Allow toggling of particle effects and avoid unnecesary computation
    if let Some((compute_push_constants, vertex_push_constants)) = particle_data {
        let compute_pipeline = engine.compute_pipeline();
        let descriptor_set = engine.compute_descriptor_set();
        let vertex_buffer = engine.vertex_buffer.clone();
        let buffer_count = engine.vertex_count() as u32;

        // Build compute commands
        builder
            // Push constants for compute shader
            .push_constants(compute_pipeline.layout().clone(), 0, compute_push_constants)
            // Perform compute operation to update particle positions
            .bind_pipeline_compute(compute_pipeline.clone())
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                compute_pipeline.layout().clone(),
                0, // Bind this descriptor set to index 0
                descriptor_set,
            )
            .dispatch([buffer_count / 128, 1, 1])
            .unwrap();

        // Start render pass
        begin_render_pass(&mut builder, framebuffer);

        // Add inline commands to render particles
        inline_particles_cmds(
            &mut builder,
            &engine.particle_pipeline(),
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
        &engine.fractal_pipeline(),
        fractal_data,
        (*framebuffer.attachments())[1].clone(),
        (*framebuffer.attachments())[2].clone(),
    );

    // Mark completion of frame rendering (for this pass)
    builder.end_render_pass().unwrap();

    // Return new command buffer for this framebuffer
    builder.build().unwrap()
}

fn inline_particles_cmds(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipeline: &Arc<GraphicsPipeline>,
    vertex_buffer: &Arc<DeviceLocalBuffer<[Vertex]>>,
    push_constants: ParticleVertexPushConstants,
    descriptor_set: Arc<PersistentDescriptorSet>,
) {
    use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
    let buffer_count = vertex_buffer.len() as u32;

    // Build render pass commands
    builder
        // Draw particles
        .bind_pipeline_graphics(pipeline.clone())
        .push_constants(pipeline.layout().clone(), 0, push_constants)
        .bind_vertex_buffers(0, vertex_buffer.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Graphics,
            pipeline.layout().clone(),
            0, // Bind this descriptor set to index 0
            descriptor_set,
        )
        .draw(buffer_count, 1, 0, 0)
        .expect("Failed to draw particle subpass");
}

fn inline_fractal_cmds(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipeline: &Arc<GraphicsPipeline>,
    push_constants: FractalPushConstants,
    particle_input: Arc<dyn ImageViewAbstract>,
    particle_depth: Arc<dyn ImageViewAbstract>,
) {
    // Need a descriptor set to use previous pass in the draw
    let layout = pipeline.layout().set_layouts().get(0).unwrap();
    // TODO: Use https://docs.rs/vulkano/latest/vulkano/descriptor_set/single_layout_pool/struct.SingleLayoutDescSetPool.html ?
    let descriptor_set = PersistentDescriptorSet::new(
        layout.clone(),
        [
            WriteDescriptorSet::image_view(0, particle_input),
            WriteDescriptorSet::image_view(1, particle_depth),
        ],
    )
    .unwrap();

    // Build render pass commands
    builder
        .bind_pipeline_graphics(pipeline.clone())
        // Push constants
        .push_constants(pipeline.layout().clone(), 0, push_constants)
        .bind_descriptor_sets(
            PipelineBindPoint::Graphics,
            pipeline.layout().clone(),
            0,
            descriptor_set,
        )
        // Draw 4 static vertices (entire view quad)
        .draw(4, 1, 0, 0)
        .expect("Failed to draw fractal subpass");
}
