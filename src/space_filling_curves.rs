#[allow(dead_code)] // TODO: Use all of my code

// Contain code for mapping a number in [0, 1] to a cube
pub mod cube {

    // Define vertices of a cube as a type
    enum Vertex {
        Vertex0,
        Vertex1,
        Vertex2,
        Vertex3,
        Vertex4,
        Vertex5,
        Vertex6,
        Vertex7
    }

    // Create a point for each vertex on cube
    const V0: (f32, f32, f32) = (1., 1., -1.);
    const V1: (f32, f32, f32) = (1., -1., -1.);
    const V2: (f32, f32, f32) = (-1., -1., -1.);
    const V3: (f32, f32, f32) = (-1., 1., -1.);
    const V4: (f32, f32, f32) = (-1., 1., 1.);
    const V5: (f32, f32, f32) = (-1., -1., 1.);
    const V6: (f32, f32, f32) = (1., -1., 1.);
    const V7: (f32, f32, f32) = (1., 1., 1.);

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

    // Function to add triplets together
    fn add_triplets((x, y, z): (f32, f32, f32), (xx, yy, zz): (f32, f32, f32)) -> (f32, f32, f32) {
        (x + xx, y + yy, z + zz)
    }

    // Function to map number `x` and vertex to a point along an edge
    fn vertex_pos(x: f32, v: Vertex) -> (f32, f32, f32) {
        match v {
            Vertex::Vertex0 => add_triplets(V0, (0., -x, 0.)),
            Vertex::Vertex1 => add_triplets(V1, if x < 0.5 {(0., 1. - (x * 2.), 0.)} else {(-2. * (x - 0.5), 0., 0.)}),
            Vertex::Vertex2 => add_triplets(V2, if x < 0.5 {(1. - (x * 2.), 0., 0.)} else {(0., 2. * (x - 0.5), 0.)}),
            Vertex::Vertex3 => add_triplets(V3, if x < 0.5 {(0., (x * 2.) - 1., 0.)} else {(0., 0., 2. * (x - 0.5))}),
            Vertex::Vertex4 => add_triplets(V4, if x < 0.5 {(0., 0., (x * 2.) - 1.)} else {(0., -2. * (x - 0.5), 0.)}),
            Vertex::Vertex5 => add_triplets(V5, if x < 0.5 {(0., 1. - (x * 2.), 0.)} else {(2. * (x - 0.5), 0., 0.)}),
            Vertex::Vertex6 => add_triplets(V6, if x < 0.5 {((x * 2.) - 1., 0., 0.)} else {(0., 2. * (x - 0.5), 0.)}),
            Vertex::Vertex7 => add_triplets(V7, (0., x - 1., 0.))
        }
    }

    // Function to map a point `p` to its location in a specified vertex cell
    fn cell_transform(p: (f32, f32, f32), v: Vertex) -> (f32, f32, f32) {
        // Function to halve the distance of a point from the origin
        fn halve_triplet(p: (f32, f32, f32)) -> (f32, f32, f32) {
            match p { (x, y, z) => (x/2., y/2., z/2.) }
        }

        // Set of functions that rotate a point about the origin to align with a vertex
        fn r0(p: (f32, f32, f32)) -> (f32, f32, f32) {
            match p {(x, y, z) => (y, -z, -x)}
        }
        fn r1(p: (f32, f32, f32)) -> (f32, f32, f32) {
            match p {(x, y, z) => (-z, x, -y)}
        }
        fn r2(p: (f32, f32, f32)) -> (f32, f32, f32) {
            match p {(x, y, z) => (-x, -y, z)}
        }
        fn r3(p: (f32, f32, f32)) -> (f32, f32, f32) {
            match p {(x, y, z) => (z, x, y)}
        }
        fn r4(p: (f32, f32, f32)) -> (f32, f32, f32) {
            match p {(x, y, z) => (y, z, x)}
        }

        match v {
            Vertex::Vertex0 => add_triplets((0.5, 0.5, -0.5), halve_triplet(r0(p))),
            Vertex::Vertex1 => add_triplets((0.5, -0.5, -0.5), halve_triplet(r1(p))),
            Vertex::Vertex2 => add_triplets((-0.5, -0.5, -0.5), halve_triplet(r1(p))),
            Vertex::Vertex3 => add_triplets((-0.5, 0.5, -0.5), halve_triplet(r2(p))),
            Vertex::Vertex4 => add_triplets((-0.5, 0.5, 0.5), halve_triplet(r2(p))),
            Vertex::Vertex5 => add_triplets((-0.5, -0.5, 0.5), halve_triplet(r3(p))),
            Vertex::Vertex6 => add_triplets((0.5, -0.5, 0.5), halve_triplet(r3(p))),
            Vertex::Vertex7 => add_triplets((0.5, 0.5, 0.5), halve_triplet(r4(p)))
        }
    }

    // Function to map a floating point number `x` in range [0, 1] to a point in the cube of 
    // side-length 2 that's centered at the origin, applying a depth of `n` inner cubes
    pub fn curve_to_cube_n(x: f32, n: usize) -> (f32, f32, f32) {
        fn f(n: usize, x: f32) -> (f32, f32, f32) {
            let (v, x_prime) = nearest_vertex(x);
            if n <= 0 {
                vertex_pos(x_prime, v)
            } else {
                let p_prime = f(n-1, x_prime);
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
        Vertex3
    }

    // Create a point for each vertex on square
    const V0: Vector2 = Vector2{x: 1., y: 1.};
    const V1: Vector2 = Vector2{x: 1., y: -1.};
    const V2: Vector2 = Vector2{x: -1., y: -1.};
    const V3: Vector2 = Vector2{x: -1., y: 1.,};
    const HALF_V0: Vector2 = Vector2{x: 0.5, y: 0.5};
    const HALF_V1: Vector2 = Vector2{x: 0.5, y: -0.5};
    const HALF_V2: Vector2 = Vector2{x: -0.5, y: -0.5};
    const HALF_V3: Vector2 = Vector2{x: -0.5, y: 0.5};

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
            Vertex::Vertex1 => V1 + if x < 0.5 { Vector2::new(0., 1. - (x * 2.)) } else { Vector2::new(-2. * (x - 0.5), 0.) },
            Vertex::Vertex2 => V2 + if x < 0.5 { Vector2::new(1. - (x * 2.), 0.) } else { Vector2::new(0., 2. * (x - 0.5)) },
            Vertex::Vertex3 => V3 + Vector2::new(0., x - 1.)
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
            Vertex::Vertex0 => { r0(p); p.scale_self(0.5); HALF_V0 + *p }
            Vertex::Vertex1 => { p.scale_self(0.5); HALF_V1 + *p }
            Vertex::Vertex2 => { p.scale_self(0.5); HALF_V2 + *p }
            Vertex::Vertex3 => { r1(p); p.scale_self(0.5); HALF_V3 + *p }
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
                let mut p_prime = f(n-1, x_prime);
                cell_transform(&mut p_prime, v)
            }
        }
        f(n, x)
    }
}