#version 450

layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 vel;

layout(location = 0) out vec4 outColor;

layout (push_constant) uniform Push
{
	float time;
	float particleCount;
} push;

const vec4 speedConst1 = vec4(0.0, 0.45, 0.55, 0.1);
const vec4 speedConst2 = vec4(0.45, 0.75, 0.0, 0.375);
const vec4 speedConst3 = vec4(0.7, 0.0, 1.0, 3.0);
const float maxSpeed = 6.0;

const vec4 indexConst0 = vec4(0.8, 0.3, 0.3, 0.25);
const vec4 indexConst1 = vec4(0.2, 0.45, 0.65, 0.5);
const vec4 indexConst2 = vec4(0.6, 0.75, 0.4, 0.75);
const vec4 indexConst3 = vec4(0.7, 0.0, 0.75, 1.0);

void main() {
	gl_Position = vec4(pos, 0.0, 1.0);

	float t = fract(float(gl_VertexIndex)/push.particleCount + 0.0225*push.time);
	vec4 indexColor;
	if(t < indexConst0.w) {
		indexColor = vec4(mix(indexConst0.xyz, indexConst1.xyz, t / indexConst0.w), 1.0);
	} else if(t <  indexConst1.w) {
		indexColor = vec4(mix(indexConst1.xyz, indexConst2.xyz, (t - indexConst0.w)/(indexConst1.w - indexConst0.w)), 1.0);
	} else if(t <  indexConst2.w) {
		indexColor = vec4(mix(indexConst2.xyz, indexConst3.xyz, (t - indexConst2.w)/(indexConst2.w - indexConst1.w)), 1.0);
	} else {
		indexColor = vec4(mix(indexConst3.xyz, indexConst0.xyz, (t - indexConst3.w)/(indexConst3.w - indexConst2.w)), 1.0);
	}

	float speed = min(length(vel), maxSpeed);
	vec4 speedColor;
	if(speed < speedConst1.w) {
		speedColor = vec4(mix(0.12*indexColor.xyz + vec3(0.2), vec3(speedConst1.x, speedConst1.y * speed/speedConst1.w, speedConst1.z), speed / speedConst1.w), 1.0);
	} else if(speed < speedConst2.w) {
		speedColor = vec4(mix(speedConst1.xyz, speedConst2.xyz, (speed - speedConst1.w)/(speedConst2.w - speedConst1.w)), 1.0);
	} else if(speed < speedConst3.w) {
		speedColor = vec4(mix(speedConst2.xyz, speedConst3.xyz, (speed - speedConst2.w)/(speedConst3.w - speedConst2.w)), 1.0);
	} else {
		speedColor = vec4(mix(speedConst3.xyz, vec3(1.0, 0.0, 0.0), (speed - speedConst3.w)/(maxSpeed - speedConst3.w)), 1.0);
	}

	outColor = mix(speedColor, indexColor, pow(max(speed - (indexConst1.w/4.0), 0.0)/maxSpeed, 0.25));
}