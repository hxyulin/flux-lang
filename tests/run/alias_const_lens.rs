//! Type aliases of arrays whose lengths name constants, literal or
//! evaluated, in bodies, signatures and fields, nested, and generic.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

const N: usize = 2;
const V: usize = N * 2;
type Pair = [u32; N];
type Matrix = [[u32; V]; V];
type Row<T> = [T; V];
type Plus = [u8; V + 1];

struct Paths {
    dist: Row<u32>,
    pair: Pair,
}

fn add_edge(m: &mut Matrix, a: usize, b: usize, w: u32) {
    m[a][b] = w;
    m[b][a] = w;
}

fn row_sum(m: &Matrix, r: usize) -> u32 {
    m[r].iter().sum()
}

fn degrees(m: &Matrix) -> Row<usize> {
    let mut out = [0; V];
    for (i, row) in m.iter().enumerate() {
        out[i] = row.iter().filter(|&&w| w != 0).count();
    }
    out
}

fn first(p: Pair) -> u32 {
    p[0]
}

fn main() {
    let p: Pair = [3, 4];
    show(1, (first(p) * 10 + p[1]) as i32);
    let mut m: Matrix = [[0; V]; V];
    add_edge(&mut m, 0, 1, 5);
    add_edge(&mut m, 1, 3, 7);
    show(2, row_sum(&m, 1) as i32);
    let d = degrees(&m);
    show(3, (d[0] * 100 + d[1] * 10 + d[3]) as i32);
    let paths = Paths {
        dist: [1, 2, 3, 4],
        pair: p,
    };
    show(4, (paths.dist.len() + paths.pair.len()) as i32);
    let plus: Plus = [1; 5];
    show(5, plus.iter().map(|&b| b as i32).sum());
}
