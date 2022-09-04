/*
    fractal_sugar - An experimental audio-visualizer combining fractals and particle simulations.
    Copyright (C) 2022  Ryan Andersen

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

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
enum CustomSchemeColor {
    ColorString(String),
    ColorStringVal(String, f32),
    Vec4(Vec<f32>),
}

#[derive(Deserialize)]
struct CustomScheme {
    pub name: String,
    pub speed: [CustomSchemeColor; 4],
    pub index: [CustomSchemeColor; 4],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlData {
    pub max_speed: Option<f32>,
    pub particle_count: Option<usize>,

    #[serde(default)]
    pub color_schemes: Vec<CustomScheme>,
}

// Hardcoded default values
const MAX_SPEED: f32 = 7.;
const PARTICLE_COUNT: usize = 1_250_000;

#[derive(Clone)]
pub struct AppConfig {
    pub max_speed: f32,
    pub particle_count: usize,
    pub color_schemes: Vec<Scheme>,
    pub color_scheme_names: Vec<String>,
}
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            max_speed: MAX_SPEED,
            particle_count: PARTICLE_COUNT,
            color_schemes: COLOR_SCHEMES.to_vec(),
            color_scheme_names: COLOR_SCHEME_NAMES
                .iter()
                .map(|&s| String::from(s))
                .collect(),
        }
    }
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
                    let (r, g, b) = css_to_rgb(css_color);
                    [r, g, b, 1.]
                }
                ColorStringVal(css_color, val) => {
                    let (r, g, b) = css_to_rgb(css_color);
                    [r, g, b, *val]
                }
                Vec4(vec) => [
                    index_or_one(vec, 0),
                    index_or_one(vec, 1),
                    index_or_one(vec, 2),
                    index_or_one(vec, 3),
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

pub fn parse_file(filepath: &str) -> Result<AppConfig, Box<dyn Error>> {
    let config: TomlData = toml::from_str(&std::fs::read_to_string(filepath)?)?;

    let mut schemes: Vec<Scheme> = vec![];
    let mut scheme_names: Vec<String> = vec![];
    for cs in &config.color_schemes {
        schemes.push(Scheme::from(cs));
        scheme_names.push(cs.name.clone());
    }

    let (color_schemes, color_scheme_names) = if schemes.is_empty() {
        assert_eq!(
            COLOR_SCHEMES.len(),
            COLOR_SCHEME_NAMES.len(),
            "Ensure the compile-time default schemes have equal length."
        );

        // Sane default color scheme
        (
            COLOR_SCHEMES.to_vec(),
            COLOR_SCHEME_NAMES
                .iter()
                .map(|&s| String::from(s))
                .collect(),
        )
    } else {
        (schemes, scheme_names)
    };

    let particle_count = {
        let n = config.particle_count.unwrap_or(PARTICLE_COUNT);
        if n == 0 {
            return Err(Box::<dyn Error>::from(
                "The `particle_count` must be a positive integer.",
            ));
        }
        n
    };

    Ok(AppConfig {
        max_speed: config.max_speed.unwrap_or(MAX_SPEED),
        particle_count,
        color_schemes,
        color_scheme_names,
    })
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

const COLOR_SCHEMES: [Scheme; 4] = [ORIGINAL, NORTHERN_LIGHTS, ARCTIC, MAGMA_CORE];
const COLOR_SCHEME_NAMES: [&str; 4] = ["Classic", "Northern Lights", "Arctic", "Magma Core"];
