use std::sync::Arc;

use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, PrimaryAutoCommandBuffer, SubpassContents,
};
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::device::{Device, Queue};
use vulkano::format::ClearValue;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, Pipeline, PipelineBindPoint};
use vulkano::render_pass::Framebuffer;

use super::vertex::Vertex;

pub fn create_particles_cmdbuf(
    device: Arc<Device>,
    queue: Arc<Queue>,
    graphics_pipeline: Arc<GraphicsPipeline>,
    framebuffer: Arc<Framebuffer>,
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    compute_pipeline: Arc<ComputePipeline>,
    descriptor_set: Arc<PersistentDescriptorSet>,
    push_constant: super::particle_shaders::cs::ty::PushConstants,
) -> Arc<PrimaryAutoCommandBuffer> {
    // Regular ol' single submit buffer
    let mut builder = AutoCommandBufferBuilder::primary(
        device.clone(),
        queue.family(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    use vulkano::buffer::TypedBufferAccess; // Trait for accessing buffer length
    let buffer_count = vertex_buffer.len() as u32;

    let time = push_constant.time; // Use `time` in both pipelines

    // Build render pass commands
    builder
        // Push constants for compute shader
        .push_constants(compute_pipeline.layout().clone(), 0, push_constant)
        // Perform compute operation to update particle positions
        .bind_pipeline_compute(compute_pipeline.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            compute_pipeline.layout().clone(),
            0, // Bind this descriptor set to index 0
            descriptor_set,
        )
        .dispatch([buffer_count / 128, 1, 1])
        .unwrap()
        // Initialize rendering a frame for particles (including MSAA)
        .begin_render_pass(
            framebuffer.clone(),
            SubpassContents::Inline, // Directly use draw commands without secondary command buffers
            vec![[0., 0., 0., 1.].into(), ClearValue::None], // Clear values for attachments
        )
        .unwrap()
        // Draw particles
        .bind_pipeline_graphics(graphics_pipeline.clone())
        .push_constants(
            graphics_pipeline.layout().clone(),
            0,
            [time, buffer_count as f32],
        )
        .bind_vertex_buffers(0, vertex_buffer.clone())
        .draw(buffer_count, 1, 0, 0)
        .expect("Failed to draw graphics pipeline")
        // Mark completion of frame rendering (for this pass)
        .end_render_pass()
        .unwrap();

    // Return new command buffer for this framebuffer
    Arc::new(builder.build().unwrap())
}

/*pub fn onetime_cmdbuf_from_constant(
    device: Arc<Device>,
    queue: Arc<Queue>,
    pipeline: Arc<GraphicsPipeline>,
    framebuffer: Arc<Framebuffer>,
    push_constant: PushConstantData
) -> Arc<PrimaryAutoCommandBuffer> {
    // Regular ol' single submit buffer
    let mut builder = AutoCommandBufferBuilder::primary(
        device.clone(),
        queue.family(),
        CommandBufferUsage::OneTimeSubmit
    ).unwrap();

    // Build render pass commands
    builder
        // Initialize rendering a frame
        .begin_render_pass(
            framebuffer.clone(),
            SubpassContents::Inline, // Directly use draw commands without secondary command buffer
            vec![[0., 0., 1., 1.].into()] // Clear values for attachment(s)
        ).unwrap()

        // Push constants
        .push_constants(pipeline.layout().clone(), 0, push_constant)

        // Draw 4 static vertices (entire view quad)
        .bind_pipeline_graphics(pipeline.clone())
        .draw(4, 1, 0, 0).expect("Failed to draw graphics pipeline")

        // Mark completion of frame render
        .end_render_pass().unwrap();

    // Return new command buffer for this framebuffer
    Arc::new(builder.build().unwrap())
}*/
