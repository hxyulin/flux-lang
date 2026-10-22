//! Statics and constants of array types whose lengths name constants or
//! are constant expressions: in items, aliases, struct fields and items
//! a macro defines. Their data must be laid out at their full length,
//! as `core`'s stdout buffer (`[u8; CAPACITY]`) is, which the long
//! output at the end fills several times.

const N: usize = 4;
const M: usize = N * 2;
static BUF: [u8; N] = [1, 2, 3, 4];
static mut WORK: [u8; M] = [0; M];
static PLUS: [u16; N + 1] = [10; N + 1];
const TABLE: [u32; N] = [5, 6, 7, 8];
type Row = [u8; M];
static ROW: Row = [9; M];

struct Holder {
    data: [u8; N],
}

static HOLD: Holder = Holder {
    data: [4, 3, 2, 1],
};

macro_rules! table {
    ($name:ident, $len:expr) => {
        static $name: [u8; $len] = [7; $len];
    };
}

table!(MAC, N + 2);

fn work(i: usize, v: u8) -> u32 {
    unsafe {
        WORK[i] = v;
        WORK[i] as u32 + WORK[0] as u32
    }
}

fn main() {
    let total: u32 = BUF.iter().map(|&b| b as u32).sum();
    println!("buf {total} len {}", BUF.len());
    println!("work {}", work(M - 1, 5));
    println!("plus {}", PLUS.iter().map(|&x| x as u32).sum::<u32>());
    println!("table {} row {} {}", TABLE[3], ROW[7], ROW.len());
    println!("hold {:?} mac {:?}", HOLD.data, MAC);
    for line in 0..120 {
        println!("line {line:3}: {:?} {:?}", BUF, &ROW[..line % M]);
    }
}
