use std::ops::{Add, AddAssign, Mul, Neg, Sub};

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Default, Zeroable, Pod)]
pub struct Vector4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
impl Vector4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    pub fn xyz(self) -> Vector3 {
        Vector3::new(self.x, self.y, self.z)
    }
}
impl Add for Vector4 {
    type Output = Self;
    fn add(self, v: Self) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y,
            z: self.z + v.z,
            w: self.w + v.w,
        }
    }
}
impl AddAssign for Vector4 {
    fn add_assign(&mut self, v: Self) {
        self.x += v.x;
        self.y += v.y;
        self.z += v.z;
        self.w += v.w;
    }
}
impl Sub for Vector4 {
    type Output = Self;
    fn sub(self, v: Self) -> Self {
        Self {
            x: self.x - v.x,
            y: self.y - v.y,
            z: self.z - v.z,
            w: self.w - v.w,
        }
    }
}
impl Neg for Vector4 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: -self.w,
        }
    }
}
impl From<Vector4> for [f32; 4] {
    fn from(v: Vector4) -> Self {
        [v.x, v.y, v.z, v.w]
    }
}

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct Quaternion {
    pub v: Vector4,
}
impl Quaternion {
    pub fn build(v: Vector3, theta: f32) -> Self {
        let norm = v.norm();
        let sin = theta.sin();
        let cos = theta.cos();
        Self {
            v: Vector4::new(sin * norm.x, sin * norm.y, sin * norm.z, cos),
        }
    }
    pub fn rotate_by(&mut self, q: Self) {
        let p_prime = Vector3::new(self.v.x, self.v.y, self.v.z);
        let q_prime = Vector3::new(q.v.x, q.v.y, q.v.z);
        let v = Vector3::cross(p_prime, q_prime) + self.v.w * q_prime + q.v.w * p_prime;
        self.v = Vector4::new(
            v.x,
            v.y,
            v.z,
            self.v.w * q.v.w - Vector3::dot(p_prime, q_prime),
        );
    }
    pub fn rotate_point(&self, p: Vector3) -> Vector3 {
        let q = Vector3::new(self.v.x, self.v.y, self.v.z);
        let temp = Vector3::cross(q, Vector3::cross(q, p) + self.v.w * p);
        p + temp + temp
    }
}
impl Default for Quaternion {
    fn default() -> Self {
        Self {
            v: Vector4 {
                x: 0.,
                y: 0.,
                z: 0.,
                w: 1.,
            },
        }
    }
}
impl From<Quaternion> for [f32; 4] {
    fn from(q: Quaternion) -> Self {
        q.v.into()
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default, Zeroable, Pod)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    _unused: f32, // NOTE: This is needed because of how `vec3`s are aligned on the GPU. This float is only to be used to preserve alignment
}
impl Vector3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self {
            x,
            y,
            z,
            _unused: 0.,
        }
    }
    pub fn scale(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
            _unused: 0.,
        }
    }
    pub fn cross(a: Self, b: Self) -> Self {
        Self {
            x: a.y * b.z - a.z * b.y,
            y: (-a.x) * b.z + a.z * b.x,
            z: a.x * b.y - a.y * b.x,
            _unused: 0.,
        }
    }
    pub fn dot(a: Self, b: Self) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }
    pub fn norm(self) -> Self {
        let r2 = Vector3::dot(self, self);
        if r2 < 0.000_000_1 {
            Vector3::new(1., 0., 0.)
        } else {
            let r = r2.sqrt();
            Vector3::new(self.x / r, self.y / r, self.z / r)
        }
    }
}
impl Add for Vector3 {
    type Output = Self;
    fn add(self, v: Self) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y,
            z: self.z + v.z,
            _unused: 0.,
        }
    }
}
impl AddAssign for Vector3 {
    fn add_assign(&mut self, v: Self) {
        self.x += v.x;
        self.y += v.y;
        self.z += v.z;
    }
}
impl Sub for Vector3 {
    type Output = Self;
    fn sub(self, v: Self) -> Self {
        Self {
            x: self.x - v.x,
            y: self.y - v.y,
            z: self.z - v.z,
            _unused: 0.,
        }
    }
}
impl Mul<Vector3> for f32 {
    type Output = Vector3;

    fn mul(self, v: Vector3) -> Vector3 {
        v.scale(self)
    }
}
impl From<Vector3> for [f32; 3] {
    fn from(v: Vector3) -> Self {
        [v.x, v.y, v.z]
    }
}
impl From<Vector3> for [f32; 4] {
    fn from(v: Vector3) -> Self {
        [v.x, v.y, v.z, 0.]
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default, Zeroable, Pod)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}
impl Vector2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn scale_self(&mut self, s: f32) {
        self.x *= s;
        self.y *= s;
    }
    pub fn scale(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
}
impl Add for Vector2 {
    type Output = Self;
    fn add(self, v: Self) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y,
        }
    }
}
impl AddAssign for Vector2 {
    fn add_assign(&mut self, v: Self) {
        self.x += v.x;
        self.y += v.y;
    }
}
impl Sub for Vector2 {
    type Output = Self;
    fn sub(self, v: Self) -> Self {
        Self {
            x: self.x - v.x,
            y: self.y - v.y,
        }
    }
}
impl Mul<Vector2> for f32 {
    type Output = Vector2;

    fn mul(self, v: Vector2) -> Vector2 {
        v.scale(self)
    }
}
impl Neg for Vector2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl From<Vector2> for [f32; 2] {
    fn from(v: Vector2) -> Self {
        [v.x, v.y]
    }
}

pub mod helpers {
    // Helpers for exponential value interpolation
    pub fn interpolate_floats(source: &mut f32, target: f32, scale: f32) {
        let smooth = 1. - (scale).exp();
        *source += smooth * (target - *source);
    }
    pub fn interpolate_vec3(source: &mut super::Vector3, target: &super::Vector3, scale: f32) {
        let smooth = 1. - (scale).exp();
        *source += smooth * (*target - *source);
    }
}
