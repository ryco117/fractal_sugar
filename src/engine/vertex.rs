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

use bytemuck::{Pod, Zeroable};
use vulkano::pipeline::graphics::vertex_input::Vertex;

use crate::my_math::Vector3;

#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod, Vertex)]
pub struct PointParticle {
    #[format(R32G32B32A32_SFLOAT)]
    pub pos: Vector3,
    #[format(R32G32B32A32_SFLOAT)]
    pub vel: Vector3,
}
