#version 450

layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 vel;

layout (location = 0) out vec4 outColor;

layout (push_constant) uniform PushConstants {
	vec4 quaternion;
	float time;
	float particle_count;
	float aspect_ratio;
	bool rendering_fractal;
	bool alternate_colors;
	bool use_third_dimension;
} push;

// Define color constants
const vec4 speedConst1 = vec4(0.0, 0.425, 0.55, 0.2);
const vec4 speedConst2 = vec4(0.5, 0.725, 0.1, 0.5);
const vec4 speedConst3 = vec4(0.7, 0.2, 1.0, 3.5);
const float maxSpeed = 6.0; // Must match `max_speed` in compute shader

const vec4 indexConst0 = vec4(0.8, 0.5, 0.3, 0.25);
const vec4 indexConst1 = vec4(0.35, 0.4, 0.8, 0.5);
const vec4 indexConst2 = vec4(0.8, 0.5, 0.6, 0.75);
const vec4 indexConst3 = vec4(0.7, 0.1, 0.75, 1.0);

// Define constants for perspective rendering
const float pi = 3.14159265358;
const float verticalFov = (pi/2.5) / 2.0;	// Roughly 70 degress vertical FOV
const float far = 8.0;
const float near = 0.03125;
mat4 createPerspective(float aspectRatio) {
	float focalLength = 1.0 / tan(verticalFov);
	return mat4(
		// Column-major declaration
		vec4(focalLength / aspectRatio, 0.0, 0.0, 0.0),
		vec4(0.0, focalLength, 0.0, 0.0),
		vec4(0.0, 0.0, -(far+near)/(far - near), -1.0),
		vec4(0.0, 0.0, -2.0*far*near/(far - near), 0.0)
	);
}

vec3 rotateByQuaternion(vec3 v, vec4 q) {
	vec3 temp = cross(q.xyz, cross(q.xyz, v) + q.w * v);
	return v + temp+temp;
}

void main() {
	gl_PointSize = 2.0;

	// Calculate screen position based on desired perspective
	if(push.use_third_dimension) {
		vec4 q = push.quaternion;
		q.w = -q.w;
		vec4 temp = createPerspective(push.aspect_ratio) * vec4(rotateByQuaternion(pos, q) - vec3(0.0, 0.0, 1.85), 1.0);
		gl_Position = temp;
	} else {
		gl_Position = vec4(pos.xy, 0.0, 1.0);
	}

	float t = fract(float(gl_VertexIndex)/push.particle_count + 0.0485*push.time);
	vec3 indexColor;
	{
		vec3 indexStart;
		vec3 indexEnd;
		float indexScale;
		if(t < indexConst0.w) {
			indexStart = indexConst0.xyz;
			indexEnd = indexConst1.xyz;
			indexScale = t / indexConst0.w;
		} else if(t <  indexConst1.w) {
			indexStart = indexConst1.xyz;
			indexEnd = indexConst2.xyz;
			indexScale = (t - indexConst0.w)/(indexConst1.w - indexConst0.w);
		} else if(t <  indexConst2.w) {
			indexStart = indexConst2.xyz;
			indexEnd = indexConst3.xyz;
			indexScale = (t - indexConst1.w)/(indexConst2.w - indexConst1.w);
		} else {
			indexStart = indexConst3.xyz;
			indexEnd = indexConst0.xyz;
			indexScale = (t - indexConst2.w)/(indexConst3.w - indexConst2.w);
		}
		if(push.alternate_colors) {
			indexStart = abs(vec3(1.0) - indexStart);
			indexEnd = abs(vec3(1.0) - indexEnd);
		}
		indexColor = mix(indexStart, indexEnd, indexScale);
	}

	float speed = min(length(vel), maxSpeed);
	vec3 speedColor;
	{
		vec3 speedStart;
		vec3 speedEnd;
		float speedScale;
		if(speed < speedConst1.w) {
			vec3 basesColor = (push.use_third_dimension ? 0.3 : (push.rendering_fractal ? 0.325 : 0.575)) * indexColor;
			speedStart = basesColor;
			speedEnd = vec3(speedConst1.x, speedConst1.y * speed/speedConst1.w, speedConst1.z);
			if(push.alternate_colors) {
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = speed / speedConst1.w;
		} else if(speed < speedConst2.w) {
			speedStart = speedConst1.xyz;
			speedEnd = speedConst2.xyz;
			if(push.alternate_colors) {
				speedStart = abs(vec3(1.0) - speedStart);
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = (speed - speedConst1.w)/(speedConst2.w - speedConst1.w);
		} else if(speed < speedConst3.w) {
			speedStart = speedConst2.xyz;
			speedEnd = speedConst3.xyz;
			if(push.alternate_colors) {
				speedStart = abs(vec3(1.0) - speedStart);
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = (speed - speedConst2.w)/(speedConst3.w - speedConst2.w);
		} else {
			speedStart = speedConst3.xyz;
			speedEnd = vec3(1.0, 0.4, 0.4);
			if(push.alternate_colors) {
				speedStart = abs(vec3(1.0) - speedStart);
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = (speed - speedConst3.w)/(maxSpeed - speedConst3.w);
		}
		speedColor = mix(speedStart, speedEnd, speedScale);
	}

	outColor = vec4(mix(speedColor, indexColor, pow(max(speed - maxSpeed/100.0, 0.0)/maxSpeed, 0.35)), 1.0);
}