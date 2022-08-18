use std::error::Error;

use bytemuck::{Pod, Zeroable};
use css_color_parser::Color as CssColor;
use serde::Deserialize;

#[repr(C)]
#[derive(Copy, Clone, Default, Zeroable, Pod)]
pub struct Scheme {
    pub speed: [[f32; 4]; 4],
    pub index: [[f32; 4]; 4],
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum CustomSchemeColor {
    ColorString(String),
    ColorStringVal(String, f32),
    Vec4(Vec<f32>),
}

#[derive(Deserialize)]
pub struct CustomScheme {
    pub name: String,
    pub speed: [CustomSchemeColor; 4],
    pub index: [CustomSchemeColor; 4],
}

#[derive(Deserialize)]
pub struct CustomSchemes {
    pub color_schemes: Vec<CustomScheme>,
}

impl std::convert::From<&CustomScheme> for Scheme {
    fn from(cs: &CustomScheme) -> Self {
        fn index_or_one(arr: &[f32], i: usize) -> f32 {
            if i < arr.len() {
                arr[i]
            } else {
                1.
            }
        }
        fn u8_to_f32_color(uc: u8) -> f32 {
            f32::from(uc) / 255.
        }
        fn css_to_rgb(css_color: &str) -> (f32, f32, f32) {
            let c = css_color.parse::<CssColor>().unwrap();
            (
                u8_to_f32_color(c.r),
                u8_to_f32_color(c.g),
                u8_to_f32_color(c.b),
            )
        }
        fn custom_to_vec4(color: &CustomSchemeColor) -> [f32; 4] {
            use CustomSchemeColor::*;
            match color {
                ColorString(css_color) => {
                    let (r, g, b) = css_to_rgb(&css_color);
                    [r, g, b, 1.]
                }
                ColorStringVal(css_color, val) => {
                    let (r, g, b) = css_to_rgb(&css_color);
                    [r, g, b, *val]
                }
                Vec4(vec) => [
                    index_or_one(&vec, 0),
                    index_or_one(&vec, 1),
                    index_or_one(&vec, 2),
                    index_or_one(&vec, 3),
                ],
            }
        }

        let mut scheme = Self::default();
        for i in 0..4 {
            scheme.speed[i] = custom_to_vec4(&cs.speed[i]);
            scheme.index[i] = custom_to_vec4(&cs.index[i]);
        }

        scheme
    }
}

pub fn parse_custom_schemes(filepath: &str) -> Result<Vec<Scheme>, Box<dyn Error>> {
    let custom_schemes: CustomSchemes = toml::from_str(&std::fs::read_to_string(filepath)?)?;

    let mut schemes: Vec<Scheme> = vec![];
    for cs in custom_schemes.color_schemes.iter() {
        schemes.push(Scheme::from(cs))
    }

    if schemes.len() > 0 {
        Ok(schemes)
    } else {
        Err(Box::<dyn Error>::from("No color schemes processed"))
    }
}

pub const ORIGINAL: Scheme = Scheme {
    speed: [
        [0., 0.425, 0.55, 0.2],
        [0.5, 0.725, 0.1, 0.5],
        [0.7, 0.2, 1., 3.5],
        [1., 0.4, 0.4, 0.],
    ],
    index: [
        [0.8, 0.5, 0.3, 0.25],
        [0.35, 0.4, 0.8, 0.5],
        [0.8, 0.5, 0.6, 0.75],
        [0.7, 0.1, 0.75, 1.],
    ],
};

pub const NORTHERN_LIGHTS: Scheme = Scheme {
    speed: [
        [0.04, 0.5, 0.35, 0.2],
        [0.55, 0.2, 0.45, 0.8],
        [0.85, 0.45, 0.02, 1.5],
        [0.65, 0.08, 0.04, 0.],
    ],
    index: [
        [0., 0.3, 0.55, 0.25],
        [0.1, 0.65, 0.45, 0.5],
        [0., 0.3, 0.42, 0.75],
        [0., 0.65, 0.45, 1.],
    ],
};

pub const ARCTIC: Scheme = Scheme {
    speed: [
        [0.15, 0.375, 0.42, 0.15],
        [0.55, 0.6, 0.65, 1.],
        [0.75, 0.75, 0.8, 3.],
        [0.95, 0.95, 0.98, 0.],
    ],
    index: [
        [0.72, 0.75, 0.8, 0.25],
        [0.3, 0.35, 0.375, 0.5],
        [0.7, 0.72, 0.75, 0.75],
        [0.3, 0.375, 0.35, 1.],
    ],
};

pub const MAGMA_CORE: Scheme = Scheme {
    speed: [
        [0.575, 0.01, 0.05, 0.18],
        [0.95, 0.72, 0.02, 1.2],
        [0.95, 0.62, 0.02, 3.5],
        [0.8, 0.65, 0.5, 0.],
    ],
    index: [
        [0.5, 0., 0.05, 0.25],
        [0.22, 0.22, 0.23, 0.5],
        [0.75, 0.5, 0.01, 0.75],
        [0.6, 0.55, 0.5, 1.],
    ],
};
