#version 450

layout (location = 0) out vec2 coord;

vec2 quad[4] = vec2[] (
	vec2(-1.0, -1.0),
	vec2(-1.0,  1.0),
	vec2( 1.0, -1.0),
	vec2( 1.0,  1.0)
);

void main() {
	gl_Position = vec4(quad[gl_VertexIndex], 0.0, 1.0);
	coord = quad[gl_VertexIndex];
}