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

// Function to add points together
fn add_points((x, y, z): (f32, f32, f32), (xx, yy, zz): (f32, f32, f32)) -> (f32, f32, f32) {
    (x + xx, y + yy, z + zz)
}

// Function to map number `x` and vertex to a point along an edge
fn vertex_pos(x: f32, v: Vertex) -> (f32, f32, f32) {
    match v {
        Vertex::Vertex0 => add_points(V0, (0., -x, 0.)),
        Vertex::Vertex1 => add_points(V1, if x < 0.5 {(0., 1. - (x * 2.), 0.)} else {(-2. * (x - 0.5), 0., 0.)}),
        Vertex::Vertex2 => add_points(V2, if x < 0.5 {(1. - (x * 2.), 0., 0.)} else {(0., 2. * (x - 0.5), 0.)}),
        Vertex::Vertex3 => add_points(V3, if x < 0.5 {(0., (x * 2.) - 1., 0.)} else {(0., 0., 2. * (x - 0.5))}),
        Vertex::Vertex4 => add_points(V4, if x < 0.5 {(0., 0., (x * 2.) - 1.)} else {(0., -2. * (x - 0.5), 0.)}),
        Vertex::Vertex5 => add_points(V5, if x < 0.5 {(0., 1. - (x * 2.), 0.)} else {(2. * (x - 0.5), 0., 0.)}),
        Vertex::Vertex6 => add_points(V6, if x < 0.5 {((x * 2.) - 1., 0., 0.)} else {(0., 2. * (x - 0.5), 0.)}),
        Vertex::Vertex7 => add_points(V7, (0., x - 1., 0.))
    }
}

// Function to halve the distance of a point from the origin
fn scale(p: (f32, f32, f32)) -> (f32, f32, f32) {
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

// Function to map a point `p` to its location in a specified vertex cell
fn cell_transform(p: (f32, f32, f32), v: Vertex) -> (f32, f32, f32) {
    match v {
        Vertex::Vertex0 => add_points((0.5, 0.5, -0.5), scale(r0(p))),
        Vertex::Vertex1 => add_points((0.5, -0.5, -0.5), scale(r1(p))),
        Vertex::Vertex2 => add_points((-0.5, -0.5, -0.5), scale(r1(p))),
        Vertex::Vertex3 => add_points((-0.5, 0.5, -0.5), scale(r2(p))),
        Vertex::Vertex4 => add_points((-0.5, 0.5, 0.5), scale(r2(p))),
        Vertex::Vertex5 => add_points((-0.5, -0.5, 0.5), scale(r3(p))),
        Vertex::Vertex6 => add_points((0.5, -0.5, 0.5), scale(r3(p))),
        Vertex::Vertex7 => add_points((0.5, 0.5, 0.5), scale(r4(p)))
    }
}

// Function to map a floating point number `x` to a point in the cube, applying a depth of `n` inner cubes
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

// Same as `curve_to_cube_n` but with default depth of 5
pub fn default_curve_to_cube(x: f32) -> (f32, f32, f32) {
    curve_to_cube_n(x, 5)
}