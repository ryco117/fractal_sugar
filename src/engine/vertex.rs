use bytemuck::{Pod, Zeroable};
use vulkano::pipeline::graphics::vertex_input::{VertexMember, VertexMemberTy};

use crate::my_math::Vector2;

#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod)]
pub struct Vertex {
    pub pos: Vector2,
    pub vel: Vector2
}

// Allow us to use Vector2 as vec2 type
unsafe impl VertexMember for Vector2 {
    fn format() -> (VertexMemberTy, usize) { (VertexMemberTy::F32, 2) }
}

vulkano::impl_vertex!(Vertex, pos, vel);