#version 450

layout (location = 0) in vec2 coord;

layout (location = 0) out vec4 fragColor;

layout (input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput particle_color;

layout (push_constant) uniform PushConstants
{
	vec4 quaternion;

	vec4 reactive_bass;
    vec4 reactive_mids;
    vec4 reactive_high;

	vec4 smooth_bass;
    vec4 smooth_mids;
    vec4 smooth_high;

	float time;
	float width;
	float height;
	float kaleidoscope;
	uint distance_estimator_id;
	bool render_background;
} push;

const float pi = 3.14159265358;
const float tau = 2.0*pi;
const float e = 2.718281828;
const float epsilon = 0.00005;
const vec3 dirX = vec3(1.0, 0.0, 0.0);
const vec3 dirY = vec3(0.0, 1.0, 0.0);
const vec3 dirZ = vec3(0.0, 0.0, 1.0);

mat3 buildRot3(vec3 u, float theta)
{
	float c = cos(theta);
	float cC = 1.0 - c;
	float s = sin(theta);
	float sC = 1.0 - s;
	return mat3(
		c+u.x*u.x*cC, u.y*u.x*cC+u.z*s, u.z*u.x*cC-u.y*s,
		u.x*u.y*cC-u.z*s, c+u.y*u.y*cC, u.z*u.y*cC+u.x*s,
		u.x*u.z*cC+u.y*s, u.y*u.z*cC-u.x*s, c+u.z*u.z*cC
	);
}

vec3 rotateByQuaternion(vec3 v, vec4 q)
{
	vec3 temp = cross(q.xyz, cross(q.xyz, v) + q.w * v);
	return v + temp+temp;
}

vec3 safe_normalize(vec3 t) {
	if(length(t) < 0.000001) {
		return vec3(1.0, 0.0, 0.0);
	}
	return normalize(t);
}

float getAngle(vec2 s)
{
	float theta = 0.0;
	if(s.y < 0.0)
	{
		s *= -1.0;
		theta = pi;
	}

	if(length(s) < 0.000001)
	{
		s = vec2(1.0, 0.0);
	}
	else
	{
		s = normalize(s);
	}

	if(s.x >= 0.0)
	{
		return theta + asin(s.y);
	}
	else
	{
		return theta + pi - asin(s.y);
	}
}

float boundReflect(float x, float b)
{
	float r = mod(x + b, 4.0*b);
	if(r < 2.0*b)
	{
		return r - b;
	}
	else
	{
		return 3.0*b - r;
	}
}

vec3 gradient;
vec4 orbitTrap;
float distanceEstimator(vec3 t)
{
	orbitTrap = vec4(1.0, 1.0, 1.0, 1.0);

	// Mandelbox
	if(push.distance_estimator_id == 1)
	{
		const int maxIterations = 5;
		const float reScale = 4.8;
		t *= reScale;
		vec3 s = t;
		const float mandelboxScale = 0.25*cos(0.075 * push.time) - 2.1;
		float DEfactor = 1.0;
		float r2 = 1.0;
		const float maxR2 = 12.0;
		const float BVR = sqrt(maxR2);
		for (int i = 0; i < maxIterations; i++)
		{
			if(s.x>1.0){s.x=2.0-s.x;}else if(s.x<-1.0){s.x=-2.0-s.x;}
			if(s.y>1.0){s.y=2.0-s.y;}else if(s.y<-1.0){s.y=-2.0-s.y;}
			if(s.z>1.0){s.z=2.0-s.z;}else if(s.z<-1.0){s.z=-2.0-s.z;}

			r2 = dot(s, s);
			if (r2 < 0.25) {
				s *= 4.0;
				DEfactor *= 4.0;
			} else if(r2 < 1.0) {
				s /= r2;
				DEfactor /= r2;
			}

			orbitTrap.x = min(orbitTrap.x, length(s/BVR - push.reactive_bass.xyz)/1.25);
			orbitTrap.y = min(orbitTrap.y, length(s/BVR - push.reactive_mids.xyz)/1.25);
			orbitTrap.z = min(orbitTrap.z, length(s/BVR - push.reactive_high.xyz)/1.25);

			s = s*mandelboxScale + t;
			DEfactor = DEfactor*abs(mandelboxScale) + 1.0;
		
			if(r2 > maxR2) break;
		}
		return (length(s)-BVR)/abs(DEfactor) / reScale;
	}
	// Mandelbulb
	else if(push.distance_estimator_id == 2)
	{
		const int maxIterations = 3;
		const float reScale = 1.85;
		t *= reScale;
		vec3 s = t;
		float power = 9. + 2.0*boundReflect(0.0375*push.time + 1.0, 1.0);
		float dr = 1.0;
		float r = 0.0;

		mat3 colorRotato = buildRot3(safe_normalize(push.smooth_mids.xyz), 0.325*push.time);

		for(int i = 0; i < maxIterations; i++)
		{
			r = length(s);
			const float b = 1.5;
			if (r > b) break;

			float theta = acos(s.z/r);
			float phi = atan(s.y, s.x);
			dr = pow(r, power-1.0)*power*dr + 1.0;

			r = pow(r, power);
			theta *= power;
			phi *= power;

			s = r*vec3(sin(theta)*cos(phi), sin(theta)*sin(phi), cos(theta));
			s += t;

			orbitTrap.xyz = min(orbitTrap.xyz, abs((s - (push.reactive_high.xyz + push.reactive_bass.xyz)/2.0) * colorRotato)/1.25);
		}
		return min(0.5*log(r)*r/dr, 3.5) / reScale;
	}
	else if(push.distance_estimator_id == 3)
	{
		const int maxIterations = 3;
		const float reScale = 0.8;
		t = reScale*t;
		vec3 s = t;

		float anim = 1.275 + 0.085*sin(0.2*push.time);
		float scale = 1.0;
		float theta = 0.1 * push.time;
		float ct = cos(theta);
		float st = sin(theta);
		mat2 rotato = mat2(ct, st, -st, ct);

		mat3 colorRotato = buildRot3(safe_normalize(push.smooth_mids.xyz), 0.15*push.time);

		for(int i = 0; i < maxIterations; i++)
		{
			if (i == 2)
			{
				s.xy *= rotato;
			}

			s = -1.0 + 2.0*fract(0.5*s + 0.5);

			float r2 = dot(s,s);
		
			float k = anim/r2;
			s *= k;
			scale *= k;

			orbitTrap.xyz = min(orbitTrap.xyz, abs((s - (push.reactive_high.xyz + push.reactive_bass.xyz)/2.0) * colorRotato));
		}
	
		//return max((0.25*abs(s.z)/scale), dot(t, t)-0.25) / reScale;
		//return max((0.25*abs(s.z)/scale)/reScale, length(vec3(boundReflect(t.x/reScale, 5.0), boundReflect(t.y/reScale, 5.0), boundReflect(t.z/reScale, 5.0)))-0.62);
		return max((0.25*abs(s.z)/scale)/reScale, length(t/reScale)-0.62);
	}
	else if(push.distance_estimator_id == 4)
	{
		const int maxIterations = 4;

		const float reScale = 1.32;
		t *= reScale;
		vec3 s = t;

		s = s + 0.5; //center it by changing position and scale
		float xx=abs(s.x-0.5)-0.5, yy=abs(s.y-0.5)-0.5, zz=abs(s.z-0.5)-0.5;
		float d1=max(xx,max(yy,zz)); //distance to the box
		float d=d1; //current computed distance
		float p=1.0;
		float mengerScale = 3.0;
		float halfScale = mengerScale / 2.0;

		orbitTrap.xyz = abs(vec3(xx/1.2, yy/1.2, zz/1.2));

		float theta = 0.575*sin(0.055*push.time);
		mat3 rotato = buildRot3(safe_normalize(cross(push.smooth_bass.xyz, push.smooth_mids.xyz)), theta);

		for (int i = 0; i < maxIterations; i++)
		{
			p *= mengerScale;
			float xa = mod(s.x*p, mengerScale);
			float ya = mod(s.y*p, mengerScale);
			float za = mod(s.z*p, mengerScale);

			float xx=0.5-abs(xa-halfScale), yy=0.5-abs(ya-halfScale), zz=0.5-abs(za-halfScale);
			d1=min(max(xx,zz),min(max(xx,yy),max(yy,zz))) / p; //distance inside the 3 axis-aligned square tubes

			d=max(d,d1); //intersection

			vec3 q = vec3(xx, yy, zz);
			orbitTrap.xyz = max(orbitTrap.xyz, abs(vec3(dot(q, push.reactive_bass.xyz), dot(q, push.reactive_mids.xyz), dot(q, push.reactive_high.xyz))));

			const vec3 halfVec = vec3(0.5);
			s = (s - halfVec)*rotato + halfVec;
		}
		return d/reScale;
	}
	else if(push.distance_estimator_id == 5)
	{
		const int maxIterations = 8;
		const float scale = 2.0;
		const float reScale = 1.4;

		t *= reScale;
		vec3 s = t;
		const vec3 center = vec3(sqrt(0.5), sqrt(0.3), sqrt(0.2));
		float r2 = dot(s, s);
		float DEfactor = 1.0;

		float theta = 0.08*push.time;
		mat3 rotato1 = buildRot3(safe_normalize(push.smooth_high.xyz), theta);
		theta = 0.22*sin(0.25*push.time);
		mat3 rotato2 = buildRot3(safe_normalize(push.smooth_mids.xyz), theta);

		for(int i = 0; i < maxIterations && r2 < 1000.0; i++)
		{
			s *= rotato1;

			if(s.x+s.y<0.0){float x1=-s.y;s.y=-s.x;s.x=x1;}
			if(s.x+s.z<0.0){float x1=-s.z;s.z=-s.x;s.x=x1;}
			if(s.y+s.z<0.0){float y1=-s.z;s.z=-s.y;s.y=y1;}

			s *= rotato2;

			s = scale*s - (scale - 1.0)*center;
			r2 = dot(s, s);

			orbitTrap.x = min(orbitTrap.x, length(s - push.reactive_bass.xyz)/2.0);
			orbitTrap.y = min(orbitTrap.y, length(s - push.reactive_mids.xyz)/2.0);
			orbitTrap.z = min(orbitTrap.z, length(s - push.reactive_high.xyz)/2.0);

			DEfactor *= scale;
		}
		return (sqrt(r2) - 2.0) / DEfactor / reScale;
	}

	return 10000.0;
}

const float maxBrightness = 1.6;
const float maxBrightnessR2 = maxBrightness*maxBrightness;
vec3 scaleColor(float distanceRatio, float iterationRatio, vec3 col)
{
	col *= pow(1.0 - distanceRatio, 1.2) * pow(1.0 - iterationRatio, 2.75);
	if(dot(col, col) > maxBrightnessR2)
	{
		col = maxBrightness*normalize(col);
	}
	col = min(vec3(1.0), col);
	return col;
}

vec3 castRay(vec3 position, vec3 direction, float fovX, float fovY)
{
	const int maxIterations = 130;
	const float maxDistance = 50.0;
	const float hitDistance = epsilon;
	float minTravel = 0.3;
	if(push.distance_estimator_id == 1)
	{
		minTravel = minTravel + max(0.0, -0.75*cos(0.03 * push.time));
	}

	float lastDistance = maxDistance;
	position += minTravel * direction;
	float travel = minTravel;
	for(int i = 0; i < maxIterations; i++)
	{
		float dist = distanceEstimator(position);

		if(dist <= hitDistance)
		{
			float smoothIter = float(i) - (dist - hitDistance)/(dist - lastDistance);
			return scaleColor(travel/maxDistance, smoothIter/float(maxIterations), orbitTrap.xyz);
		}

		lastDistance = dist;

		position += (0.99*dist)*direction;
		travel += dist;
		if(travel >= maxDistance)
		{
			if(push.render_background)
			{
				vec3 unmodDirection = normalize(vec3(coord.x*fovX, -coord.y*fovY, 1.0));
				unmodDirection = rotateByQuaternion(unmodDirection, push.quaternion);

				vec3 sinDir = sin(100.0*unmodDirection);
				vec3 base = vec3(exp(-2.9*length(sin(pi * push.reactive_bass.xyz + 1.0) - sinDir)), exp(-2.9*length(sin(e * push.reactive_mids.xyz + 1.3) - sinDir)), exp(-2.9*length(sin(9.6*push.reactive_high.xyz + 117.69420) - sinDir)));
				return (push.distance_estimator_id == 0 ? 0.8 : 0.575) * base;
			}
			break;
		}
	}
	return vec3(0.0, 0.0, 0.0);
}

void main(void)
{
	const float verticalFov = (pi/2.5) / 2.0;	// Roughly 70 degress vertical FOV
	const float fovY = tan(verticalFov);
	float fovX = push.width/push.height * fovY;

	float kaleidoTheta = boundReflect(getAngle(coord), push.kaleidoscope*(pi/6.0 - tau) + tau);
	vec2 newCoord = length(coord) * vec2(cos(kaleidoTheta), sin(kaleidoTheta));
	vec3 direction = normalize(vec3(newCoord.x*fovX, -newCoord.y*fovY, 1.0));
	direction = rotateByQuaternion(direction, push.quaternion);
	
	vec3 position = rotateByQuaternion(-dirZ, push.quaternion);
	vec3 tFragColor = castRay(position, direction, fovX, fovY);

	vec3 particle = subpassLoad(particle_color).rgb;
	fragColor = vec4(abs(tFragColor - particle), 1.0);
}