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

use vulkano::buffer::cpu_pool::CpuBufferPoolSubbuffer;
use vulkano::buffer::device_local::DeviceLocalBuffer;
use vulkano::buffer::{BufferUsage, CpuBufferPool};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, PrimaryCommandBufferAbstract,
};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, Pipeline};
use vulkano::render_pass::{RenderPass, Subpass};
use vulkano::shader::ShaderModule;
use vulkano::sync::GpuFuture;

use super::vertex::Vertex;
use super::AppConstants;
use super::{pipeline, Allocators};
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
            path: "shaders/particles.frag",
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
            path: "shaders/particles.comp",
            types_meta: {
                use bytemuck::{Pod, Zeroable};
                #[derive(Clone, Copy, Zeroable, Pod)]
            },
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
            path: "shaders/ray_march.frag",
            types_meta: {
            use bytemuck::{Pod, Zeroable};
                #[derive(Clone, Copy, Zeroable, Pod)]
            },
        }
    }
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            path: "shaders/entire_view.vert",
        }
    }
}

// Export Push Constant types to callers
pub type FractalPushConstants = fractal_shaders::fs::ty::PushConstants;

const SQUARE_FILLING_CURVE_DEPTH: usize = 6;
const CUBE_FILLING_CURVE_DEPTH: usize = 4;

// Helper for containing relevant particle data
pub struct ParticleBuffersTriplet {
    pub vertex: Arc<DeviceLocalBuffer<[Vertex]>>,
    pub fixed_square: Arc<DeviceLocalBuffer<[Vector2]>>,
    pub fixed_cube: Arc<DeviceLocalBuffer<[Vector3]>>,
}

pub struct Fractal {
    pub frag_shader: Arc<ShaderModule>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub vert_shader: Arc<ShaderModule>,
}
pub struct Particles {
    pub scheme_buffer_pool: CpuBufferPool<Scheme>,
    pub scheme_buffer: Arc<CpuBufferPoolSubbuffer<Scheme>>,
    pub compute_descriptor_set: Arc<PersistentDescriptorSet>,
    pub compute_pipeline: Arc<ComputePipeline>,
    pub frag_shader: Arc<ShaderModule>,
    pub graphics_descriptor_set: Arc<PersistentDescriptorSet>,
    pub graphics_pipeline: Arc<GraphicsPipeline>,
    pub vert_shader: Arc<ShaderModule>,
    pub vertex_buffers: ParticleBuffersTriplet,
}

fn create_particle_buffers(
    allocators: &Allocators,
    queue: &Arc<Queue>,
    app_config: &AppConfig,
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

    let storage_usage = BufferUsage {
        storage_buffer: true,
        ..Default::default()
    };

    let mut cbb = AutoCommandBufferBuilder::primary(
        &allocators.command_buffer,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Create immutable fixed-position buffer for 2D perspective
    let fixed_square = DeviceLocalBuffer::from_iter(
        &allocators.memory,
        square_position_iter,
        storage_usage,
        &mut cbb,
    )
    .expect("Failed to create 2D-fixed-position buffer");

    // Create immutable fixed-position buffer for 3D perspective
    let fixed_cube = DeviceLocalBuffer::from_iter(
        &allocators.memory,
        cube_position_iter,
        storage_usage,
        &mut cbb,
    )
    .expect("Failed to create 3D-fixed-position buffer");

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
    let vertex = DeviceLocalBuffer::from_iter(
        &allocators.memory,
        vertex_iter,
        BufferUsage {
            storage_buffer: true,
            vertex_buffer: true,
            ..Default::default()
        },
        &mut cbb,
    )
    .expect("Failed to create 3D-fixed-position buffer");

    // Wait for all futures to finish before continuing
    let command_buff = cbb.build().unwrap();
    command_buff
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    ParticleBuffersTriplet {
        vertex,
        fixed_square,
        fixed_cube,
    }
}

impl Particles {
    pub fn new(
        allocators: &Allocators,
        device: &Arc<Device>,
        queue: &Arc<Queue>,
        render_pass: &Arc<RenderPass>,
        viewport: Viewport,
        app_config: &AppConfig,
        app_constants: &Arc<CpuBufferPoolSubbuffer<AppConstants>>,
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
        let scheme_buffer_pool = CpuBufferPool::uniform_buffer(allocators.memory.clone());

        // Particle color schemes?!
        let scheme_buffer = scheme_buffer_pool
            .from_data(app_config.color_schemes[0])
            .unwrap();
        let graphics_descriptor_set = Self::new_graphics_descriptor(
            &allocators.descriptor_set,
            &graphics_pipeline,
            scheme_buffer.clone(),
            app_constants.clone(),
        );

        // Create storage buffers for particle info
        let vertex_buffers = create_particle_buffers(allocators, queue, app_config);

        // Create a new descriptor set for binding particle storage buffers
        // Required to access layout() method
        let compute_descriptor_set = Self::new_compute_descriptor(
            &allocators.descriptor_set,
            &compute_pipeline,
            &vertex_buffers,
            app_constants.clone(),
        );

        Self {
            scheme_buffer_pool,
            scheme_buffer,
            compute_descriptor_set,
            compute_pipeline,
            frag_shader,
            graphics_descriptor_set,
            graphics_pipeline,
            vert_shader,
            vertex_buffers,
        }
    }

    // Update particle state when color scheme changes
    pub fn update_color_scheme(
        &mut self,
        allocator: &StandardDescriptorSetAllocator,
        scheme: Scheme,
        app_constants: Arc<CpuBufferPoolSubbuffer<AppConstants>>,
    ) {
        self.scheme_buffer = self.scheme_buffer_pool.from_data(scheme).unwrap();
        self.update_graphics_descriptor(allocator, app_constants);
    }

    // Update particle state when app constants change
    pub fn update_app_constants(
        &mut self,
        allocator: &StandardDescriptorSetAllocator,
        app_constants: Arc<CpuBufferPoolSubbuffer<AppConstants>>,
    ) {
        self.update_graphics_descriptor(allocator, app_constants.clone());
        self.compute_descriptor_set = Self::new_compute_descriptor(
            allocator,
            &self.compute_pipeline,
            &self.vertex_buffers,
            app_constants,
        );
    }

    // Helpers for creating particle desciptor sets
    fn update_graphics_descriptor(
        &mut self,
        allocator: &StandardDescriptorSetAllocator,
        app_constants: Arc<CpuBufferPoolSubbuffer<AppConstants>>,
    ) {
        self.graphics_descriptor_set = Self::new_graphics_descriptor(
            allocator,
            &self.graphics_pipeline,
            self.scheme_buffer.clone(),
            app_constants,
        );
    }
    fn new_graphics_descriptor(
        allocator: &StandardDescriptorSetAllocator,
        pipeline: &Arc<GraphicsPipeline>,
        scheme: Arc<CpuBufferPoolSubbuffer<Scheme>>,
        app_constants: Arc<CpuBufferPoolSubbuffer<AppConstants>>,
    ) -> Arc<PersistentDescriptorSet> {
        PersistentDescriptorSet::new(
            allocator,
            pipeline.layout().set_layouts().get(0).unwrap().clone(),
            [
                WriteDescriptorSet::buffer(0, scheme),
                WriteDescriptorSet::buffer(1, app_constants),
            ],
        )
        .unwrap()
    }
    fn new_compute_descriptor(
        allocator: &StandardDescriptorSetAllocator,
        pipeline: &Arc<ComputePipeline>,
        vertex_buffers: &ParticleBuffersTriplet,
        app_constants: Arc<CpuBufferPoolSubbuffer<AppConstants>>,
    ) -> Arc<PersistentDescriptorSet> {
        PersistentDescriptorSet::new(
            allocator,
            pipeline
                .layout()
                .set_layouts()
                .get(0) // 0 is the index of the descriptor set layout we want
                .unwrap()
                .clone(),
            [
                WriteDescriptorSet::buffer(0, vertex_buffers.vertex.clone()), // 0 is the binding of the data in this set
                WriteDescriptorSet::buffer(1, vertex_buffers.fixed_square.clone()),
                WriteDescriptorSet::buffer(2, vertex_buffers.fixed_cube.clone()),
                WriteDescriptorSet::buffer(3, app_constants),
            ],
        )
        .unwrap()
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
