#[allow(dead_code)] // TODO: Use all of my code

use std::ops::{Add, AddAssign, Sub, Neg};

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct Vector4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32
}

impl Add for Vector4 {
    type Output = Self;
    fn add(self, v: Self) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y,
            z: self.z + v.z,
            w: self.w + v.w
        }
    }
}
impl AddAssign for Vector4 {
    fn add_assign(&mut self, v: Self) {
        self.x += v.x;
        self.y += v.y;
        self.z += v.z;
        self.w += v.w
    }
}
impl Sub for Vector4 {
    type Output = Self;
    fn sub(self, v: Self) -> Self {
        Self {
            x: self.x - v.x,
            y: self.y - v.y,
            z: self.z - v.z,
            w: self.w - v.w
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
            w: -self.w
        }
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable, Pod)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32
}

impl Add for Vector2 {
    type Output = Self;
    fn add(self, v: Self) -> Self {
        Self {
            x: self.x + v.x,
            y: self.y + v.y
        }
    }
}
impl AddAssign for Vector2 {
    fn add_assign(&mut self, v: Self) {
        self.x += v.x;
        self.y += v.y
    }
}
impl Sub for Vector2 {
    type Output = Self;
    fn sub(self, v: Self) -> Self {
        Self {
            x: self.x - v.x,
            y: self.y - v.y
        }
    }
}
impl Neg for Vector2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y
        }
    }
}
impl Vector2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self{x, y}
    }
    pub fn scale_self(&mut self, s: f32) {
        self.x *= s;
        self.y *= s
    }
    pub fn scale(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s
        }
    }
}