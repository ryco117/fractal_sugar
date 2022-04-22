#version 450

layout(location = 0) in float x;
layout(location = 1) in float y;

void main() {
	gl_Position = vec4(x, y, 0.0, 1.0);
}