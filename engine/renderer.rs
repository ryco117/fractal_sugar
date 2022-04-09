use std::sync::Arc;

use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, PrimaryAutoCommandBuffer, SubpassContents};
use vulkano::device::{Device, Queue};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::Pipeline;
use vulkano::render_pass::Framebuffer;

// Internal bytes to be copied to GPU through push constants
#[derive(Clone)]
#[repr(C)]
pub struct PushConstantData {
    pub time: f32,
    pub width: f32,
    pub height: f32
}

pub fn onetime_cmdbuf_from_constant(
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
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Build render pass commands
    builder
        // Initialize rendering a frame
        .begin_render_pass(
            framebuffer.clone(),
            SubpassContents::Inline, // Directly use draw commands without secondary command buffer
            vec![[0., 0., 1., 1.].into()] // Clear values for attachment(s)
        ).unwrap()

        // Push constant test
        .push_constants(pipeline.layout().clone(), 0, push_constant)

        // Draw 4 static vertices (entire view quad)
        .bind_pipeline_graphics(pipeline.clone())
        .draw(4, 1, 0, 0).expect("Failed to draw graphics pipeline")

        // Mark completion of frame render
        .end_render_pass().unwrap();

    // Return new command buffer for this framebuffer
    Arc::new(builder.build().unwrap())
}