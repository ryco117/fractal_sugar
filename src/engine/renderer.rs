use std::sync::Arc;

use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, PrimaryAutoCommandBuffer, SubpassContents,
};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::format::ClearValue;
use vulkano::image::ImageViewAbstract;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, Pipeline, PipelineBindPoint};
use vulkano::render_pass::Framebuffer;

use super::vertex::Vertex;
use super::{ComputePushConstants, FractalPushConstants};

pub fn create_render_commands(
    device: Arc<Device>,
    queue: Arc<Queue>,
    compute_pipeline: Arc<ComputePipeline>,
    particles_pipeline: Arc<GraphicsPipeline>,
    fractal_pipeline: Arc<GraphicsPipeline>,
    framebuffer: Arc<Framebuffer>,
    descriptor_set: Arc<PersistentDescriptorSet>,
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    particle_data: ComputePushConstants,
    fractal_data: FractalPushConstants,
    render_particles: bool, // TODO: Make compute_data an Option and only update particles when rendered
) -> PrimaryAutoCommandBuffer {
    // Regular ol' single submit buffer
    let mut builder = AutoCommandBufferBuilder::primary(
        device.clone(),
        queue.family(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
    let buffer_count = vertex_buffer.len() as u32;

    let time = particle_data.time; // Use `time` in both pipelines

    // Build render pass commands
    builder
        // Push constants for compute shader
        .push_constants(compute_pipeline.layout().clone(), 0, particle_data)
        // Perform compute operation to update particle positions
        .bind_pipeline_compute(compute_pipeline.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            compute_pipeline.layout().clone(),
            0, // Bind this descriptor set to index 0
            descriptor_set,
        )
        .dispatch([buffer_count / 64, 1, 1])
        .unwrap()
        // Initialize rendering a frame for particles (including MSAA)
        .begin_render_pass(
            framebuffer.clone(),
            SubpassContents::Inline, // Use secondary command buffers to specify later passses
            vec![[0., 0., 0., 1.].into(), ClearValue::None, ClearValue::None], // Clear values for attachments
        )
        .unwrap();

    if render_particles {
        // Add inline commands to render particles
        inline_particles_cmds(&mut builder, &particles_pipeline, &vertex_buffer, time)
    }

    // Move to next subpass
    builder.next_subpass(SubpassContents::Inline).unwrap();

    // Add inline commands to render fractal
    inline_fractal_cmds(
        &mut builder,
        &fractal_pipeline,
        fractal_data,
        (*framebuffer.attachments())[1].clone(),
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
    time: f32,
) -> () {
    use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
    let buffer_count = vertex_buffer.len() as u32;

    // Build render pass commands
    builder
        // Draw particles
        .bind_pipeline_graphics(pipeline.clone())
        .push_constants(pipeline.layout().clone(), 0, [time, buffer_count as f32])
        .bind_vertex_buffers(0, vertex_buffer.clone())
        .draw(buffer_count, 1, 0, 0)
        .expect("Failed to draw graphics pipeline");
}

fn inline_fractal_cmds(
    builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
    pipeline: &Arc<GraphicsPipeline>,
    push_constants: FractalPushConstants,
    particle_input: Arc<dyn ImageViewAbstract>,
) -> () {
    // Need a descriptor set to use previous pass in the draw
    let layout = pipeline.layout().set_layouts().get(0).unwrap();
    let descriptor_set = PersistentDescriptorSet::new(
        layout.clone(),
        [WriteDescriptorSet::image_view(0, particle_input)],
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
