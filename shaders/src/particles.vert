#version 450

layout(location = 0) in float x;
layout(location = 1) in float y;

layout(location = 0) out vec4 outColor;

layout (push_constant) uniform Push
{
	float time;
} push;

const vec3 C0 = vec3(0.8, 0.25, 0.3);
const float c1 = 0.2;
const vec3 endC1 = vec3(0.0, 0.45, 0.55);
const float c2 = 0.5;
const vec3 endC2 = vec3(0.45, 0.75, 0.0);
const float c3 = 0.8;
const vec3 endC3 = vec3(0.7, 0.0, 1.0);

const float particleCount = 1048576.0;

void main() {
	gl_Position = vec4(x, y, 0.0, 1.0);

	float t = fract(float(gl_VertexIndex)/particleCount + 0.015*push.time);
	if(t < c1) {
		outColor = vec4(mix(C0, endC1, t / c1), 1.0);
	} else if(t < c2) {
		outColor = vec4(mix(endC1, endC2, (t - c1)/(c2 - c1)), 1.0);
	} else if(t < c3) {
		outColor = vec4(mix(endC2, endC3, (t - c2)/(c3 - c2)), 1.0);
	} else {
		outColor = vec4(mix(endC3, C0, (t - c3)/(1.0 - c3)), 1.0);
	}
}