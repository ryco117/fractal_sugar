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
use vulkano::pipeline::graphics::vertex_input::{VertexMember, VertexMemberTy};

use crate::my_math::Vector3;

#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod)]
pub struct Vertex {
    pub pos: Vector3,
    pub vel: Vector3,
}

// Allow us to use Vector3 as vec3 type
unsafe impl VertexMember for Vector3 {
    fn format() -> (VertexMemberTy, usize) {
        (VertexMemberTy::F32, 3) // Each member is a vec3
    }
}

vulkano::impl_vertex!(Vertex, pos, vel);
