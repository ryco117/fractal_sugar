#version 450

layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 vel;

layout (location = 0) out vec4 outColor;

layout (binding = 0) uniform ParticleColorScheme {
    vec4 speedConst[4];
	vec4 indexConst[4];
} particleColors;

layout (binding = 1) uniform AppConstants {
	float max_speed;
	float particle_count;
	float spring_coefficient;

	float audio_scale;
} appConstants;

layout (push_constant) uniform PushConstants {
	vec4 quaternion;
	float time;
	float aspect_ratio;
	bool rendering_fractal;
	bool alternate_colors;
	bool use_third_dimension;
} push;

// Define constants for perspective rendering
// Distances must match those used in `ray_march.frag`
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
		vec4 temp = createPerspective(push.aspect_ratio) * vec4(rotateByQuaternion(pos, q) - vec3(0.0, 0.0, 1.75), 1.0);
		gl_Position = temp;
	} else {
		gl_Position = vec4(pos.xy, 0.0, 1.0);
	}

	float t = fract(float(gl_VertexIndex)/appConstants.particle_count + 0.0475*push.time);
	vec3 indexColor;
	{
		vec3 indexStart;
		vec3 indexEnd;
		float indexScale;
		if(t < particleColors.indexConst[0].w) {
			indexStart = particleColors.indexConst[3].xyz;
			indexEnd = particleColors.indexConst[0].xyz;
			indexScale = t / particleColors.indexConst[0].w;
		} else if(t <  particleColors.indexConst[1].w) {
			indexStart = particleColors.indexConst[0].xyz;
			indexEnd = particleColors.indexConst[1].xyz;
			indexScale = (t - particleColors.indexConst[0].w)/(particleColors.indexConst[1].w - particleColors.indexConst[0].w);
		} else if(t <  particleColors.indexConst[2].w) {
			indexStart = particleColors.indexConst[1].xyz;
			indexEnd = particleColors.indexConst[2].xyz;
			indexScale = (t - particleColors.indexConst[1].w)/(particleColors.indexConst[2].w - particleColors.indexConst[1].w);
		} else {
			indexStart = particleColors.indexConst[2].xyz;
			indexEnd = particleColors.indexConst[3].xyz;
			indexScale = (t - particleColors.indexConst[2].w)/(1.0 - particleColors.indexConst[2].w);
		}
		if(push.alternate_colors) {
			indexStart = abs(vec3(1.0) - indexStart);
			indexEnd = abs(vec3(1.0) - indexEnd);
		}
		indexColor = mix(indexStart, indexEnd, indexScale);
	}

	float speed = min(length(vel), appConstants.max_speed);
	vec3 speedColor;
	{
		vec3 speedStart;
		vec3 speedEnd;
		float speedScale;
		if(speed < particleColors.speedConst[0].w) {
			vec3 basesColor = (push.use_third_dimension ? 0.3 : (push.rendering_fractal ? 0.325 : 0.575)) * indexColor;
			speedStart = basesColor;
			speedEnd = vec3(particleColors.speedConst[0].x, particleColors.speedConst[0].y * speed/particleColors.speedConst[0].w, particleColors.speedConst[0].z);
			if(push.alternate_colors) {
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = speed / particleColors.speedConst[0].w;
		} else if(speed < particleColors.speedConst[1].w) {
			speedStart = particleColors.speedConst[0].xyz;
			speedEnd = particleColors.speedConst[1].xyz;
			if(push.alternate_colors) {
				speedStart = abs(vec3(1.0) - speedStart);
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = (speed - particleColors.speedConst[0].w)/(particleColors.speedConst[1].w - particleColors.speedConst[0].w);
		} else if(speed < particleColors.speedConst[2].w) {
			speedStart = particleColors.speedConst[1].xyz;
			speedEnd = particleColors.speedConst[2].xyz;
			if(push.alternate_colors) {
				speedStart = abs(vec3(1.0) - speedStart);
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = (speed - particleColors.speedConst[1].w)/(particleColors.speedConst[2].w - particleColors.speedConst[1].w);
		} else {
			speedStart = particleColors.speedConst[2].xyz;
			speedEnd = particleColors.speedConst[3].xyz;
			if(push.alternate_colors) {
				speedStart = abs(vec3(1.0) - speedStart);
				speedEnd = abs(vec3(1.0) - speedEnd);
			}
			speedScale = (speed - particleColors.speedConst[2].w)/(appConstants.max_speed - particleColors.speedConst[2].w);
		}
		speedColor = mix(speedStart, speedEnd, speedScale);
	}

	//outColor = vec4(mix(speedColor, indexColor, pow(max(speed - maxSpeed/100.0, 0.0)/maxSpeed, 0.35)), 1.0);
	outColor = vec4(speedColor, 1.0);
}