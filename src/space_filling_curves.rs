// Contain code for mapping a number in [0, 1] to a cube
pub mod cube {
    use crate::my_math::Vector3;

    // Define vertices of a cube as a type
    enum Vertex {
        Vertex0,
        Vertex1,
        Vertex2,
        Vertex3,
        Vertex4,
        Vertex5,
        Vertex6,
        Vertex7,
    }

    // Create a point for each vertex on cube
    const V0: Vector3 = Vector3::new(1., 1., -1.);
    const V1: Vector3 = Vector3::new(1., -1., -1.);
    const V2: Vector3 = Vector3::new(-1., -1., -1.);
    const V3: Vector3 = Vector3::new(-1., 1., -1.);
    const V4: Vector3 = Vector3::new(-1., 1., 1.);
    const V5: Vector3 = Vector3::new(-1., -1., 1.);
    const V6: Vector3 = Vector3::new(1., -1., 1.);
    const V7: Vector3 = Vector3::new(1., 1., 1.);

    // Function to turn a number `x` into nearest vertex and remainder
    fn nearest_vertex(x: f32) -> (Vertex, f32) {
        if x < 0.5 {
            if x < 0.25 {
                if x < 0.125 {
                    (Vertex::Vertex0, x * 8.)
                } else {
                    (Vertex::Vertex1, (x - 0.125) * 8.)
                }
            } else {
                if x < 0.375 {
                    (Vertex::Vertex2, (x - 0.25) * 8.)
                } else {
                    (Vertex::Vertex3, (x - 0.375) * 8.)
                }
            }
        } else {
            if x < 0.75 {
                if x < 0.625 {
                    (Vertex::Vertex4, (x - 0.5) * 8.)
                } else {
                    (Vertex::Vertex5, (x - 0.625) * 8.)
                }
            } else {
                if x < 0.875 {
                    (Vertex::Vertex6, (x - 0.75) * 8.)
                } else {
                    (Vertex::Vertex7, (x - 0.875) * 8.)
                }
            }
        }
    }

    // Function to map number `x` and vertex to a point along an edge
    fn vertex_pos(x: f32, v: Vertex) -> Vector3 {
        match v {
            Vertex::Vertex0 => V0 + Vector3::new(0., -x, 0.),
            Vertex::Vertex1 => {
                V1 + if x < 0.5 {
                    Vector3::new(0., 1. - (x * 2.), 0.)
                } else {
                    Vector3::new(-2. * (x - 0.5), 0., 0.)
                }
            }
            Vertex::Vertex2 => {
                V2 + if x < 0.5 {
                    Vector3::new(1. - (x * 2.), 0., 0.)
                } else {
                    Vector3::new(0., 2. * (x - 0.5), 0.)
                }
            }
            Vertex::Vertex3 => {
                V3 + if x < 0.5 {
                    Vector3::new(0., (x * 2.) - 1., 0.)
                } else {
                    Vector3::new(0., 0., 2. * (x - 0.5))
                }
            }
            Vertex::Vertex4 => {
                V4 + if x < 0.5 {
                    Vector3::new(0., 0., (x * 2.) - 1.)
                } else {
                    Vector3::new(0., -2. * (x - 0.5), 0.)
                }
            }
            Vertex::Vertex5 => {
                V5 + if x < 0.5 {
                    Vector3::new(0., 1. - (x * 2.), 0.)
                } else {
                    Vector3::new(2. * (x - 0.5), 0., 0.)
                }
            }
            Vertex::Vertex6 => {
                V6 + if x < 0.5 {
                    Vector3::new((x * 2.) - 1., 0., 0.)
                } else {
                    Vector3::new(0., 2. * (x - 0.5), 0.)
                }
            }
            Vertex::Vertex7 => V7 + Vector3::new(0., x - 1., 0.),
        }
    }

    // Function to map a point `p` to its location in a specified vertex cell
    fn cell_transform(p: Vector3, v: Vertex) -> Vector3 {
        // Set of functions that rotate a point about the origin to align with a vertex
        fn r0(p: Vector3) -> Vector3 {
            match p {
                Vector3 { x, y, z } => Vector3::new(y, -z, -x),
            }
        }
        fn r1(p: Vector3) -> Vector3 {
            match p {
                Vector3 { x, y, z } => Vector3::new(-z, x, -y),
            }
        }
        fn r2(p: Vector3) -> Vector3 {
            match p {
                Vector3 { x, y, z } => Vector3::new(-x, -y, z),
            }
        }
        fn r3(p: Vector3) -> Vector3 {
            match p {
                Vector3 { x, y, z } => Vector3::new(z, x, y),
            }
        }
        fn r4(p: Vector3) -> Vector3 {
            match p {
                Vector3 { x, y, z } => Vector3::new(y, z, x),
            }
        }

        match v {
            Vertex::Vertex0 => Vector3::new(0.5, 0.5, -0.5) + 0.5 * r0(p),
            Vertex::Vertex1 => Vector3::new(0.5, -0.5, -0.5) + 0.5 * r1(p),
            Vertex::Vertex2 => Vector3::new(-0.5, -0.5, -0.5) + 0.5 * r1(p),
            Vertex::Vertex3 => Vector3::new(-0.5, 0.5, -0.5) + 0.5 * r2(p),
            Vertex::Vertex4 => Vector3::new(-0.5, 0.5, 0.5) + 0.5 * r2(p),
            Vertex::Vertex5 => Vector3::new(-0.5, -0.5, 0.5) + 0.5 * r3(p),
            Vertex::Vertex6 => Vector3::new(0.5, -0.5, 0.5) + 0.5 * r3(p),
            Vertex::Vertex7 => Vector3::new(0.5, 0.5, 0.5) + 0.5 * r4(p),
        }
    }

    // Function to map a floating point number `x` in range [0, 1] to a point in the cube of
    // side-length 2 that's centered at the origin, applying a depth of `n` inner cubes
    pub fn curve_to_cube_n(x: f32, n: usize) -> Vector3 {
        fn f(n: usize, x: f32) -> Vector3 {
            let (v, x_prime) = nearest_vertex(x);
            if n <= 0 {
                vertex_pos(x_prime, v)
            } else {
                let p_prime = f(n - 1, x_prime);
                cell_transform(p_prime, v)
            }
        }
        f(n, x)
    }
}

// Contain code for mapping a number in [0, 1] to a square
pub mod square {
    use crate::my_math::Vector2;

    // Define vertices of a square as a type
    enum Vertex {
        Vertex0,
        Vertex1,
        Vertex2,
        Vertex3,
    }

    // Create a point for each vertex on square
    const V0: Vector2 = Vector2::new(1., 1.);
    const V1: Vector2 = Vector2::new(1., -1.);
    const V2: Vector2 = Vector2::new(-1., -1.);
    const V3: Vector2 = Vector2::new(-1., 1.);
    const HALF_V0: Vector2 = Vector2::new(0.5, 0.5);
    const HALF_V1: Vector2 = Vector2::new(0.5, -0.5);
    const HALF_V2: Vector2 = Vector2::new(-0.5, -0.5);
    const HALF_V3: Vector2 = Vector2::new(-0.5, 0.5);

    // Function to turn a number `x` into nearest vertex and remainder
    fn nearest_vertex(x: f32) -> (Vertex, f32) {
        if x < 0.5 {
            if x < 0.25 {
                (Vertex::Vertex0, x * 4.)
            } else {
                (Vertex::Vertex1, (x - 0.25) * 4.)
            }
        } else {
            if x < 0.75 {
                (Vertex::Vertex2, (x - 0.5) * 4.)
            } else {
                (Vertex::Vertex3, (x - 0.75) * 4.)
            }
        }
    }

    // Function to map number `x` and vertex to a point along an edge
    fn vertex_pos(x: f32, v: Vertex) -> Vector2 {
        match v {
            Vertex::Vertex0 => V0 + Vector2::new(0., -x),
            Vertex::Vertex1 => {
                V1 + if x < 0.5 {
                    Vector2::new(0., 1. - (x * 2.))
                } else {
                    Vector2::new(-2. * (x - 0.5), 0.)
                }
            }
            Vertex::Vertex2 => {
                V2 + if x < 0.5 {
                    Vector2::new(1. - (x * 2.), 0.)
                } else {
                    Vector2::new(0., 2. * (x - 0.5))
                }
            }
            Vertex::Vertex3 => V3 + Vector2::new(0., x - 1.),
        }
    }

    // Function to map a point `p` to its location in a specified vertex cell
    fn cell_transform(p: &mut Vector2, v: Vertex) -> Vector2 {
        // Set of functions that rotate a point about the origin to align with a vertex
        fn r0(p: &mut Vector2) {
            std::mem::swap(&mut p.x, &mut p.y)
        }
        fn r1(p: &mut Vector2) {
            let x = p.x;
            p.x = -p.y;
            p.y = -x
        }

        match v {
            Vertex::Vertex0 => {
                r0(p);
                p.scale_self(0.5);
                HALF_V0 + *p
            }
            Vertex::Vertex1 => {
                p.scale_self(0.5);
                HALF_V1 + *p
            }
            Vertex::Vertex2 => {
                p.scale_self(0.5);
                HALF_V2 + *p
            }
            Vertex::Vertex3 => {
                r1(p);
                p.scale_self(0.5);
                HALF_V3 + *p
            }
        }
    }

    // Function to map a floating point number `x` in range [0, 1] to a point in the square of
    // side-length 2 that's centered at the origin, applying a depth of `n` inner square
    pub fn curve_to_square_n(x: f32, n: usize) -> Vector2 {
        fn f(n: usize, x: f32) -> Vector2 {
            let (v, x_prime) = nearest_vertex(x);
            if n <= 0 {
                vertex_pos(x_prime, v)
            } else {
                let mut p_prime = f(n - 1, x_prime);
                cell_transform(&mut p_prime, v)
            }
        }
        f(n, x)
    }
}
