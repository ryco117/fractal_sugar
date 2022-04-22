use std::sync::Arc;

use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, PrimaryAutoCommandBuffer, SubpassContents};
use vulkano::buffer::{CpuAccessibleBuffer /*TODO: Use a better type (device local?)*/, TypedBufferAccess /*For accessing buffer array length*/};
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::device::{Device, Queue};
use vulkano::pipeline::{Pipeline, ComputePipeline, GraphicsPipeline};
use vulkano::render_pass::Framebuffer;

use crate::my_math::Vector2;

// Internal bytes to be copied to GPU through push constants
#[repr(C)]
pub struct ComputePushConstantData {
    pub big_boomer: [f32; 2],
	//vec4 curlAttractor;
	pub attractors: [f32; 4],

	pub delta_time: f32,
	pub fix_particles: bool
}
#[repr(C)]
pub struct PushConstantData {
    pub temp_data: [f32; 4],
    pub time: f32,
    pub width: f32,
    pub height: f32
}

pub fn particles_cmdbuf(
    device: Arc<Device>,
    queue: Arc<Queue>,
    graphics_pipeline: Arc<GraphicsPipeline>,
    framebuffer: Arc<Framebuffer>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[Vector2]>>,
    compute_pipeline: Arc<ComputePipeline>,
    descriptor_set: Arc<PersistentDescriptorSet>,
    push_constant: ComputePushConstantData
) -> Arc<PrimaryAutoCommandBuffer> {
    // Regular ol' single submit buffer
    let mut builder = AutoCommandBufferBuilder::primary(
        device.clone(),
        queue.family(),
        CommandBufferUsage::OneTimeSubmit
    ).unwrap();

    let buffer_count = vertex_buffer.len() as u32;

    // Build render pass commands
    builder
        // Push constants for compute shader
        .push_constants(compute_pipeline.layout().clone(), 0, push_constant)

        // Perform compute operation to update particle positions
        .bind_pipeline_compute(compute_pipeline.clone())
        .bind_descriptor_sets(
            vulkano::pipeline::PipelineBindPoint::Compute,
            compute_pipeline.layout().clone(),
            0, // Bind this descriptor set to index 0
            descriptor_set)
        .dispatch([buffer_count/128, 1, 1]).unwrap()

        // Initialize rendering a frame
        .begin_render_pass(
            framebuffer.clone(),
            SubpassContents::Inline, // Directly use draw commands without secondary command buffer
            vec![[0., 0., 1., 1.].into()] // Clear values for attachment(s)
        ).unwrap()

        .bind_pipeline_graphics(graphics_pipeline.clone())
        .bind_vertex_buffers(0, vertex_buffer.clone())
        .draw(buffer_count, 1, 0, 0).expect("Failed to draw graphics pipeline")

        // Mark completion of frame render
        .end_render_pass().unwrap();

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