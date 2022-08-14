use std::error::Error;

use bytemuck::{Pod, Zeroable};
use serde::Deserialize;

#[repr(C)]
#[derive(Copy, Clone, Default, Zeroable, Pod)]
pub struct Scheme {
    pub speed: [[f32; 4]; 4],
    pub index: [[f32; 4]; 4],
}

#[derive(Deserialize)]
pub struct CustomScheme {
    pub name: String,
    pub speed: [Vec<f32>; 4],
    pub index: [Vec<f32>; 4],
}

#[derive(Deserialize)]
pub struct CustomSchemes {
    pub color_schemes: Vec<CustomScheme>,
}

impl std::convert::From<&CustomScheme> for Scheme {
    fn from(cs: &CustomScheme) -> Self {
        let mut scheme = Self::default();
        for i in 0..3 {
            assert_eq!(
                cs.speed[i].len(),
                4,
                "The first three speed values of a color scheme must have exactly 4 values"
            );
            assert_eq!(
                cs.index[i].len(),
                4,
                "The first three index values of a color scheme must have exactly 4 values"
            );

            for j in 0..4 {
                scheme.speed[i][j] = cs.speed[i][j];
                scheme.index[i][j] = cs.index[i][j];
            }
        }

        assert_eq!(
            cs.speed[3].len(),
            3,
            "The fourth/final speed value of a color scheme must have exactly 3 values"
        );
        assert_eq!(
            cs.index[3].len(),
            3,
            "The fourth/final index value of a color scheme must have exactly 3 values"
        );
        for j in 0..3 {
            scheme.speed[3][j] = cs.speed[3][j];
            scheme.index[3][j] = cs.index[3][j];
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
        [0.6, 0.2, 0.5, 0.7],
        [0.85, 0.45, 0.02, 1.8],
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
        [0.15, 0.375, 0.425, 0.1],
        [0.55, 0.6, 0.65, 1.],
        [0.75, 0.75, 0.8, 3.25],
        [0.95, 0.95, 0.98, 0.],
    ],
    index: [
        [0.72, 0.75, 0.8, 0.25],
        [0.3, 0.35, 0.375, 0.5],
        [0.7, 0.72, 0.75, 0.75],
        [0.2, 0.4, 0.4, 1.],
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
        [0.55, 0.01, 0.05, 0.25],
        [0.22, 0.22, 0.25, 0.5],
        [0.95, 0.62, 0.02, 0.75],
        [0.65, 0.58, 0.52, 1.],
    ],
};

pub const JUNGLE: Scheme = Scheme {
    speed: [
        [0.5, 0.3, 0.2, 0.15],
        [0.7, 0.7, 0.05, 0.5],
        [0.05, 0.75, 0.25, 3.],
        [0.2, 0.8, 0.3, 0.],
    ],
    index: [
        [0.8, 0.5, 0.15, 0.25],
        [0.01, 0.55, 0.24, 0.5],
        [0.65, 0.5, 0.02, 0.75],
        [0.02, 0.65, 0.22, 1.],
    ],
};

pub const BLACK_AND_YELLOW: Scheme = Scheme {
    speed: [
        [0.5, 0.4, 0., 0.15],
        [0.7, 0.6, 0.1, 0.5],
        [0.8, 0.75, 0.65, 3.],
        [0.9, 0.9, 0.9, 0.],
    ],
    index: [
        [0.2, 0.2, 0.2, 0.25],
        [0.5, 0.45, 0., 0.5],
        [0.2, 0.2, 0.2, 0.75],
        [0.5, 0.45, 0., 1.],
    ],
};
