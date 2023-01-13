/*
    fractal_sugar - An experimental audio visualizer combining fractals and particle simulations.
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

use std::num::NonZeroUsize;

use bytemuck::{Pod, Zeroable};
use css_color_parser::Color as CssColor;
use serde::Deserialize;

#[repr(C)]
#[derive(Copy, Clone, Default, Zeroable, Pod)]
pub struct Scheme {
    pub index: [[f32; 4]; 4],
    pub speed: [[f32; 4]; 4],
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
    pub launch_fullscreen: Option<bool>,
    pub launch_help_visible: Option<bool>,

    pub max_speed: Option<f32>,
    pub spring_coefficient: Option<f32>,
    pub particle_count: Option<NonZeroUsize>,
    pub point_size: Option<f32>,
    pub friction_scale: Option<f32>,

    pub audio_scale: Option<f32>,

    pub vertical_fov: Option<f32>,

    #[serde(default)]
    pub color_schemes: Vec<CustomScheme>,
}

// Hardcoded default values
const DEFAULT_HELP_VISIBLE: bool = true;
const DEFAULT_MAX_SPEED: f32 = 7.;
const DEFAULT_PARTICLE_COUNT: usize = 1_250_000;
const DEFAULT_SPRING_COEFFICIENT: f32 = 75.;
const DEFAULT_PARTICLE_POINT_SIZE: f32 = 2.;
const DEFAULT_AUDIO_SCALE: f32 = -20.;
const DEFAULT_VERTICAL_FOV: f32 = 72.; // 72 degrees of vertical FOV

#[derive(Clone)]
pub struct AppConfig {
    pub launch_fullscreen: bool,
    pub launch_help_visible: bool,

    pub max_speed: f32,
    pub spring_coefficient: f32,
    pub particle_count: usize,
    pub point_size: f32,
    pub friction_scale: f32,

    pub audio_scale: f32,

    pub vertical_fov: f32,

    pub color_schemes: Vec<Scheme>,
    pub color_scheme_names: Vec<String>,
}
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            launch_fullscreen: bool::default(),
            launch_help_visible: DEFAULT_HELP_VISIBLE,

            max_speed: DEFAULT_MAX_SPEED,
            spring_coefficient: DEFAULT_SPRING_COEFFICIENT,
            particle_count: DEFAULT_PARTICLE_COUNT,
            point_size: DEFAULT_PARTICLE_POINT_SIZE,
            friction_scale: 1.,

            audio_scale: DEFAULT_AUDIO_SCALE,

            vertical_fov: DEFAULT_VERTICAL_FOV,

            color_schemes: COLOR_SCHEMES.to_vec(),
            color_scheme_names: COLOR_SCHEME_NAMES.into_iter().map(String::from).collect(),
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
            #[allow(clippy::enum_glob_use)]
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

pub fn parse_file(filepath: &str) -> anyhow::Result<AppConfig> {
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
            COLOR_SCHEME_NAMES.into_iter().map(String::from).collect(),
        )
    } else {
        (schemes, scheme_names)
    };

    let max_speed = match config.max_speed {
        Some(max_speed) => {
            if max_speed > 0. {
                max_speed
            } else {
                anyhow::bail!(
                    "`max_speed` must be a positive number, was given: {}",
                    max_speed
                );
            }
        }
        None => DEFAULT_MAX_SPEED,
    };

    let spring_coefficient = config
        .spring_coefficient
        .unwrap_or(DEFAULT_SPRING_COEFFICIENT);

    let particle_count = config
        .particle_count
        .unwrap_or(unsafe { NonZeroUsize::new_unchecked(DEFAULT_PARTICLE_COUNT) })
        .get();

    let point_size = config
        .point_size
        .unwrap_or(DEFAULT_PARTICLE_POINT_SIZE)
        .clamp(0., 16.);

    let friction_scale = config.friction_scale.unwrap_or(1.);

    let audio_scale = {
        const DECIBEL_SCALE: f32 = std::f32::consts::LN_10 / 10.;
        (DECIBEL_SCALE * config.audio_scale.unwrap_or(DEFAULT_AUDIO_SCALE)).exp()
    };

    let vertical_fov = config
        .vertical_fov
        .unwrap_or(DEFAULT_VERTICAL_FOV)
        .clamp(-180., 180.)
        * std::f32::consts::PI
        / 360.;

    Ok(AppConfig {
        launch_fullscreen: config.launch_fullscreen.unwrap_or_default(),
        launch_help_visible: config.launch_help_visible.unwrap_or(DEFAULT_HELP_VISIBLE),

        max_speed,
        particle_count,
        spring_coefficient,
        point_size,
        friction_scale,

        audio_scale,

        vertical_fov,

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
        [0.6, 0.4, 0.25, 0.25],
        [0.3, 0.25, 0.6, 0.5],
        [0.6, 0.4, 0.5, 0.75],
        [0.58, 0.08, 0.62, 1.],
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
        [0.0, 0.25, 0.45, 0.25],
        [0.08, 0.5, 0.35, 0.5],
        [0.0, 0.25, 0.35, 0.75],
        [0.0, 0.5, 0.35, 1.],
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
        [0.6, 0.65, 0.7, 0.25],
        [0.25, 0.3, 0.35, 0.5],
        [0.6, 0.6, 0.65, 0.75],
        [0.2, 0.25, 0.25, 1.],
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
        [0.4, 0., 0.04, 0.25],
        [0.2, 0.19, 0.16, 0.5],
        [0.35, 0.23, 0.06, 0.75],
        [0.22, 0.11, 0.08, 1.],
    ],
};

const COLOR_SCHEMES: [Scheme; 4] = [ORIGINAL, NORTHERN_LIGHTS, ARCTIC, MAGMA_CORE];
const COLOR_SCHEME_NAMES: [&str; 4] = ["Classic", "Northern Lights", "Arctic", "Magma Core"];
