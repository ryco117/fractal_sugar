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
use vulkano::buffer::{BufferUsage, CpuBufferPool, ImmutableBuffer};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, Pipeline};
use vulkano::render_pass::{RenderPass, Subpass};
use vulkano::shader::ShaderModule;
use vulkano::sync::GpuFuture;

use super::pipeline;
use super::utils;
use super::vertex::Vertex;
use super::AppConstants;
use crate::app_config::{AppConfig, Scheme};
use crate::my_math::{Vector2, Vector3};
use crate::space_filling_curves;

// Create module for the particle's shader macros
#[allow(
    clippy::expl_impl_clone_on_copy,
    clippy::needless_question_mark,
    clippy::used_underscore_binding
)]
mod particle_shaders {
    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/particles.frag"
        }
    }
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/particles.vert",
            types_meta: {
                use bytemuck::{Pod, Zeroable};
                #[derive(Clone, Copy, Zeroable, Pod)]
            },
        }
    }
    pub mod cs {
        vulkano_shaders::shader! {
            ty: "compute",
            path: "shaders/particles.comp"
        }
    }
}

// Export push constant types to callers
pub type ParticleComputePushConstants = particle_shaders::cs::ty::PushConstants;
pub type ParticleVertexPushConstants = particle_shaders::vs::ty::PushConstants;

// Create module for the fractal shader macros
#[allow(clippy::expl_impl_clone_on_copy, clippy::needless_question_mark)]
mod fractal_shaders {
    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/ray_march.frag"
        }
    }
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/entire_view.vert"
        }
    }
}

// Export Push Constant types to callers
pub type FractalPushConstants = fractal_shaders::fs::ty::PushConstants;

type ImmutableBufferFromBufferFuture = vulkano::command_buffer::CommandBufferExecFuture<
    vulkano::sync::NowFuture,
    vulkano::command_buffer::PrimaryAutoCommandBuffer,
>;

const SQUARE_FILLING_CURVE_DEPTH: usize = 6;
const CUBE_FILLING_CURVE_DEPTH: usize = 4;

pub struct Fractal {
    pub frag_shader: Arc<ShaderModule>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub vert_shader: Arc<ShaderModule>,
}
pub struct Particles {
    pub scheme_buffer_pool: CpuBufferPool<Scheme>,
    pub compute_descriptor_set: Arc<PersistentDescriptorSet>,
    pub compute_pipeline: Arc<ComputePipeline>,
    pub frag_shader: Arc<ShaderModule>,
    pub graphics_descriptor_set: Arc<PersistentDescriptorSet>,
    pub graphics_pipeline: Arc<GraphicsPipeline>,
    pub vert_shader: Arc<ShaderModule>,
    pub vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
}

// Helper type for `create_particle_buffers`
struct ParticleBuffersTriplet {
    vertex_buffer: Arc<DeviceLocalBuffer<[Vertex]>>,
    fixed_square_buffer: Arc<ImmutableBuffer<[Vector2]>>,
    fixed_cube_buffer: Arc<ImmutableBuffer<[Vector3]>>,
}

fn create_particle_buffers(
    device: &Arc<Device>,
    queue: &Arc<Queue>,
    app_config: &AppConfig,
    app_constants_future: ImmutableBufferFromBufferFuture,
) -> ParticleBuffersTriplet {
    let particle_count_f32 = app_config.particle_count as f32;

    // Create position data by mapping particle index to screen using a space filling curve
    let square_position_iter = (0..app_config.particle_count).map(|i| {
        space_filling_curves::square::curve_to_square_n(
            i as f32 / particle_count_f32,
            SQUARE_FILLING_CURVE_DEPTH,
        )
    });
    let cube_position_iter = (0..app_config.particle_count).map(|i| {
        space_filling_curves::cube::curve_to_cube_n(
            i as f32 / particle_count_f32,
            CUBE_FILLING_CURVE_DEPTH,
        )
    });

    // Create immutable fixed-position buffer for 2D perspective
    let (fixed_square_buffer, fixed_square_copy_future) = ImmutableBuffer::from_iter(
        square_position_iter,
        BufferUsage::storage_buffer(),
        queue.clone(),
    )
    .expect("Failed to create immutable fixed-position buffer");

    // Create immutable fixed-position buffer for 3D perspective
    let (fixed_cube_buffer, fixed_cube_copy_future) = ImmutableBuffer::from_iter(
        cube_position_iter,
        BufferUsage::storage_buffer(),
        queue.clone(),
    )
    .expect("Failed to create immutable fixed-position buffer");

    // Create vertex data by re-calculating position
    let vertex_iter = (0..app_config.particle_count).map(|i| Vertex {
        pos: {
            let Vector2 { x, y } = space_filling_curves::square::curve_to_square_n(
                i as f32 / particle_count_f32,
                SQUARE_FILLING_CURVE_DEPTH,
            );
            Vector3::new(x, y, 0.)
        },
        vel: Vector3::default(),
    });

    // Create position buffer
    let (vertex_buffer, vertex_future) = utils::local_buffer_from_iter(
        device,
        queue,
        vertex_iter,
        BufferUsage::storage_buffer() | BufferUsage::vertex_buffer(),
    );

    // Wait for all futures to finish before continuing
    fixed_square_copy_future
        .join(fixed_cube_copy_future)
        .join(vertex_future)
        .join(app_constants_future)
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    ParticleBuffersTriplet {
        vertex_buffer,
        fixed_square_buffer,
        fixed_cube_buffer,
    }
}

impl Particles {
    pub fn new(
        device: &Arc<Device>,
        queue: &Arc<Queue>,
        render_pass: &Arc<RenderPass>,
        viewport: Viewport,
        app_config: &AppConfig,
        app_constants: &Arc<ImmutableBuffer<AppConstants>>,
        app_constants_future: ImmutableBufferFromBufferFuture,
    ) -> Self {
        // Load particle shaders
        let frag_shader = particle_shaders::fs::load(device.clone())
            .expect("Failed to load particle fragment shader");
        let vert_shader = particle_shaders::vs::load(device.clone())
            .expect("Failed to load particle vertex shader");
        let comp_shader = particle_shaders::cs::load(device.clone())
            .expect("Failed to load particle compute shader");

        // Create compute pipeline for particles
        let compute_pipeline = ComputePipeline::new(
            device.clone(),
            comp_shader.entry_point("main").unwrap(),
            &(),
            None,
            |_| {},
        )
        .expect("Failed to create compute shader");

        // Create the almighty graphics pipelines
        let graphics_pipeline = pipeline::create_particle(
            device.clone(),
            &vert_shader,
            &frag_shader,
            Subpass::from(render_pass.clone(), 0).unwrap(),
            viewport,
        );

        // Use a buffer pool for quick switching of scheme data
        let scheme_buffer_pool = CpuBufferPool::uniform_buffer(device.clone());

        // Particle color schemes?!
        let graphics_descriptor_set = {
            let color_scheme_buffer = scheme_buffer_pool
                .next(app_config.color_schemes[0])
                .unwrap();
            PersistentDescriptorSet::new(
                graphics_pipeline
                    .layout()
                    .set_layouts()
                    .get(0)
                    .unwrap()
                    .clone(),
                [
                    WriteDescriptorSet::buffer(0, color_scheme_buffer),
                    WriteDescriptorSet::buffer(1, app_constants.clone()),
                ],
            )
            .unwrap()
        };

        // Create storage buffers for particle info
        let ParticleBuffersTriplet {
            vertex_buffer,
            fixed_square_buffer,
            fixed_cube_buffer,
        } = create_particle_buffers(device, queue, app_config, app_constants_future);

        // Create a new descriptor set for binding particle storage buffers
        // Required to access layout() method
        let compute_descriptor_set = PersistentDescriptorSet::new(
            compute_pipeline
                .layout()
                .set_layouts()
                .get(0) // 0 is the index of the descriptor set layout we want
                .unwrap()
                .clone(),
            [
                WriteDescriptorSet::buffer(0, vertex_buffer.clone()), // 0 is the binding of the data in this set
                WriteDescriptorSet::buffer(1, fixed_square_buffer),
                WriteDescriptorSet::buffer(2, fixed_cube_buffer),
                WriteDescriptorSet::buffer(3, app_constants.clone()),
            ],
        )
        .unwrap();

        Self {
            scheme_buffer_pool,
            compute_descriptor_set,
            compute_pipeline,
            frag_shader,
            graphics_descriptor_set,
            graphics_pipeline,
            vert_shader,
            vertex_buffer,
        }
    }
}

impl Fractal {
    pub fn new(device: &Arc<Device>, render_pass: &Arc<RenderPass>, viewport: Viewport) -> Self {
        // Load fractal shaders
        let frag_shader = fractal_shaders::fs::load(device.clone())
            .expect("Failed to load fractal fragment shader");
        let vert_shader = fractal_shaders::vs::load(device.clone())
            .expect("Failed to load fractal vertex shader");

        let pipeline = pipeline::create_fractal(
            device.clone(),
            &vert_shader,
            &frag_shader,
            Subpass::from(render_pass.clone(), 1).unwrap(),
            viewport,
        );

        Self {
            frag_shader,
            pipeline,
            vert_shader,
        }
    }
}
