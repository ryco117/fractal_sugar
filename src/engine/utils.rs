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
use vulkano::buffer::{BufferContents, BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, CommandBufferExecFuture, CommandBufferUsage, CopyBufferInfoTyped,
    PrimaryAutoCommandBuffer, PrimaryCommandBuffer,
};
use vulkano::device::{Device, Queue};
use vulkano::sync::NowFuture;
use vulkano::DeviceSize;

// Create a device-local buffer initialized with the data from the iterator
pub fn local_buffer_from_iter<T, I>(
    device: &Arc<Device>,
    queue: &Arc<Queue>,
    data_iter: I,
    usage: BufferUsage,
) -> (
    Arc<DeviceLocalBuffer<[T]>>,
    CommandBufferExecFuture<NowFuture, PrimaryAutoCommandBuffer>,
)
where
    [T]: BufferContents,
    I: ExactSizeIterator<Item = T>,
{
    let count = data_iter.len();

    // Create simple buffer type that we can write data to
    let data_source_buffer = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::transfer_src(),
        false,
        data_iter,
    )
    .expect("Failed to create transfer-source buffer");

    // Create device-local buffer for optimal GPU access
    let local_buffer = DeviceLocalBuffer::<[T]>::array(
        device.clone(),
        count as DeviceSize,
        BufferUsage {
            transfer_dst: true,
            ..usage
        },
        device.active_queue_families(),
    )
    .expect("Failed to create device-local destination buffer");

    // Create buffer copy command
    let mut cbb = AutoCommandBufferBuilder::primary(
        device.clone(),
        queue.family(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();
    cbb.copy_buffer(CopyBufferInfoTyped::buffers(
        data_source_buffer,
        local_buffer.clone(),
    ))
    .unwrap();
    let cb = cbb.build().unwrap();

    // Create future representing execution of copy-command
    let future = cb.execute(queue.clone()).unwrap();

    // Return device-local buffer with execution future (so caller can decide how best to synchronize execution)
    (local_buffer, future)
}
