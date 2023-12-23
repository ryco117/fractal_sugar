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

use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferUsage, CopyBufferInfo, PrimaryCommandBufferAbstract,
};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{
    ComputePipeline, GraphicsPipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo,
};
use vulkano::render_pass::{RenderPass, Subpass};
use vulkano::shader::ShaderModule;
use vulkano::sync::GpuFuture;

use super::vertex::PointParticle;
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
pub mod particle_shaders {
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
        }
    }
    pub mod cs {
        vulkano_shaders::shader! {
            ty: "compute",
            path: "shaders/particles.comp",
        }
    }
}

// Export push constant types to callers
pub type ParticleComputePushConstants = particle_shaders::cs::PushConstants;
pub type ParticleVertexPushConstants = particle_shaders::vs::PushConstants;
pub type ConfigConstants = particle_shaders::vs::ConfigConstants;
pub type RuntimeConstants = particle_shaders::vs::RuntimeConstants;

// Create module for the fractal shader macros
#[allow(clippy::expl_impl_clone_on_copy, clippy::needless_question_mark)]
mod fractal_shaders {
    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            path: "shaders/ray_march.frag",
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
pub type FractalPushConstants = fractal_shaders::fs::PushConstants;

const SQUARE_FILLING_CURVE_DEPTH: usize = 6;
const CUBE_FILLING_CURVE_DEPTH: usize = 4;

// Helper for containing relevant particle data
pub struct ParticleBuffersTriplet {
    pub vertex: Subbuffer<[PointParticle]>,
    pub fixed_square: Subbuffer<[Vector2]>,
    pub fixed_cube: Subbuffer<[Vector3]>,
}

pub struct Fractal {
    pub frag_shader: Arc<ShaderModule>,
    pub pipeline: Arc<GraphicsPipeline>,
    pub vert_shader: Arc<ShaderModule>,
}
pub struct Particles {
    pub scheme_buffer: Subbuffer<Scheme>,
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

    // Buffer usage for device-local storage buffers.
    let storage_usage = BufferCreateInfo {
        usage: BufferUsage::STORAGE_BUFFER | BufferUsage::TRANSFER_DST | BufferUsage::VERTEX_BUFFER,
        ..Default::default()
    };

    // A helper for creating device-local buffers from iterators.
    fn device_local_buffer<T: bytemuck::Pod + std::marker::Send + std::marker::Sync>(
        allocators: &Allocators,
        queue: &Arc<Queue>,
        storage_usage: BufferCreateInfo,
        iter: impl ExactSizeIterator<Item = T>,
    ) -> Option<Subbuffer<[T]>> {
        // Buffer usage for temporary transfer buffers into device-local memory.
        let temp_usage = BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        };
        // Memory type filter for temporary transfer buffers.
        let temp_memory = AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::HOST_SEQUENTIAL_WRITE
                | MemoryTypeFilter::PREFER_HOST,
            ..Default::default()
        };

        // Memory type filter for device-local storage buffers.
        let device_memory = AllocationCreateInfo {
            // Specify this buffer will only be used by the device.
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        };

        // Create temporary buffer from the input iterator.
        let temporary_accessible_buffer =
            Buffer::from_iter(allocators.memory.clone(), temp_usage, temp_memory, iter)
                .map_err(|err| println!("Failed to create temporary buffer: {err:?}"))
                .ok()?;

        // Create a buffer in device-local memory with enough space.
        let device_local_buffer = Buffer::new_slice::<T>(
            allocators.memory.clone(),
            storage_usage,
            device_memory,
            temporary_accessible_buffer.len() as vulkano::DeviceSize,
        )
        .map_err(|err| println!("Failed to create device-local buffer: {err:?}"))
        .ok()?;

        // Create one-time command to copy between the buffers.
        let mut cbb = AutoCommandBufferBuilder::primary(
            &allocators.command_buffer,
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();
        cbb.copy_buffer(CopyBufferInfo::buffers(
            temporary_accessible_buffer,
            device_local_buffer.clone(),
        ))
        .map_err(|err| println!("Failed to create buffer-copy command: {err:?}"))
        .ok()?;
        let cb = cbb.build().unwrap();

        // Execute copy and wait for copy to complete before proceeding.
        cb.execute(queue.clone())
            .map_err(|err| println!("Failed to execute buffer-copy command: {err:?}"))
            .ok()?
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None /* timeout */)
            .unwrap();

        Some(device_local_buffer)
    }

    // Create position data by mapping particle index to screen using a space filling curve
    let square_position_iter = (0..app_config.particle_count).map(|i| {
        space_filling_curves::square::curve_to_square_n(
            i as f32 / particle_count_f32,
            SQUARE_FILLING_CURVE_DEPTH,
        )
    });

    // Create immutable fixed-position buffer for 2D perspective
    let fixed_square = device_local_buffer(
        &allocators,
        queue,
        storage_usage.clone(),
        square_position_iter,
    )
    .expect("Failed to create 2D-fixed-position buffer");

    // Create position data by mapping particle index to screen using a space filling curve
    let cube_position_iter = (0..app_config.particle_count).map(|i| {
        space_filling_curves::cube::curve_to_cube_n(
            i as f32 / particle_count_f32,
            CUBE_FILLING_CURVE_DEPTH,
        )
    });

    // Create immutable fixed-position buffer for 3D perspective
    let fixed_cube = device_local_buffer(
        &allocators,
        queue,
        storage_usage.clone(),
        cube_position_iter,
    )
    .expect("Failed to create 3D-fixed-position buffer");

    // Create vertex data by re-calculating position
    let vertex_iter = (0..app_config.particle_count).map(|i| PointParticle {
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
    let vertex = device_local_buffer(&allocators, queue, storage_usage.clone(), vertex_iter)
        .expect("Failed to create 3D-fixed-position buffer");

    ParticleBuffersTriplet {
        vertex,
        fixed_square,
        fixed_cube,
    }
}

impl Particles {
    pub fn new(
        allocators: &Allocators,
        queue: &Arc<Queue>,
        render_pass: &Arc<RenderPass>,
        viewport: Viewport,
        app_config: &AppConfig,
        config_constants: Subbuffer<ConfigConstants>,
        runtime_constants: Subbuffer<RuntimeConstants>,
    ) -> Self {
        // Load particle shaders
        let device = queue.device();
        let frag_shader = particle_shaders::fs::load(device.clone())
            .expect("Failed to load particle fragment shader");
        let vert_shader = particle_shaders::vs::load(device.clone())
            .expect("Failed to load particle vertex shader");
        let comp_shader = particle_shaders::cs::load(device.clone())
            .expect("Failed to load particle compute shader")
            .entry_point("main")
            .unwrap();

        // Create compute pipeline for particles
        let compute_stage = PipelineShaderStageCreateInfo::new(comp_shader);
        let compute_layout = PipelineLayout::new(
            device.clone(),
            PipelineDescriptorSetLayoutCreateInfo::from_stages([&compute_stage])
                .into_pipeline_layout_create_info(device.clone())
                .unwrap(),
        )
        .unwrap();
        let compute_pipeline = ComputePipeline::new(
            device.clone(),
            None, // comp_shader.entry_point("main").unwrap()
            ComputePipelineCreateInfo::stage_layout(compute_stage, compute_layout),
        )
        .expect("Failed to create compute shader");

        // Create the almighty graphics pipelines
        let graphics_pipeline = pipeline::create_particle(
            device.clone(),
            &vert_shader,
            &frag_shader,
            Subpass::from(render_pass.clone(), 0).expect("Failed to create subpass"),
            viewport,
        );

        // Particle color schemes?!
        let scheme_buffer = allocators
            .uniform_buffer
            .allocate_sized::<Scheme>()
            .expect("Failed to allocate color scheme buffer");
        *scheme_buffer
            .write()
            .expect("Failed to initialize color scheme buffer") = app_config.color_schemes[0];
        let graphics_descriptor_set = Self::new_graphics_descriptor(
            &allocators.descriptor_set,
            &graphics_pipeline,
            scheme_buffer.clone(),
            config_constants.clone(),
            runtime_constants,
        );

        // Create storage buffers for particle info
        let vertex_buffers = create_particle_buffers(allocators, queue, app_config);

        // Create a new descriptor set for binding particle storage buffers
        // Required to access layout() method
        let compute_descriptor_set = Self::new_compute_descriptor(
            &allocators.descriptor_set,
            &compute_pipeline,
            &vertex_buffers,
            config_constants,
        );

        Self {
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
    pub fn update_color_scheme(&mut self, scheme: Scheme) {
        *self.scheme_buffer.write().expect("Update color buffer") = scheme;
    }

    // Helpers for creating particle desciptor sets
    fn new_graphics_descriptor(
        allocator: &StandardDescriptorSetAllocator,
        pipeline: &Arc<GraphicsPipeline>,
        scheme: Subbuffer<Scheme>,
        config_constants: Subbuffer<ConfigConstants>,
        runtime_constants: Subbuffer<RuntimeConstants>,
    ) -> Arc<PersistentDescriptorSet> {
        PersistentDescriptorSet::new(
            allocator,
            pipeline.layout().set_layouts().get(0).unwrap().clone(),
            [
                WriteDescriptorSet::buffer(0, scheme),
                WriteDescriptorSet::buffer(1, config_constants),
                WriteDescriptorSet::buffer(2, runtime_constants),
            ],
            [],
        )
        .expect("Failed to create particle graphics descriptor set")
    }
    fn new_compute_descriptor(
        allocator: &StandardDescriptorSetAllocator,
        pipeline: &Arc<ComputePipeline>,
        vertex_buffers: &ParticleBuffersTriplet,
        config_constants: Subbuffer<ConfigConstants>,
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
                WriteDescriptorSet::buffer(3, config_constants),
            ],
            [],
        )
        .expect("Failed to create particle compute descriptor set")
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
            Subpass::from(render_pass.clone(), 1).expect("Failed to create fractal subpass"),
            viewport,
        );

        Self {
            frag_shader,
            pipeline,
            vert_shader,
        }
    }
}
