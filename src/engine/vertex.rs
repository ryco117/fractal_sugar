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
