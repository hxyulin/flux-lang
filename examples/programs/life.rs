//! Conway's game of life on a fixed toroidal grid: seeds a glider, a
//! blinker and an R-pentomino, prints a few generations as text, detects
//! still lifes and period-2 oscillation, and reports population counts.
//!
//! Rewritten for flux from `original/life.rs`: no `Eq` derive (arrays
//! do not implement `Eq`), no tuple comparison, no `matches!`, array
//! methods called through a slice, no `char` literals, no `&CStr` type.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn putchar(c: i32) -> i32;
}

const HASH: i32 = 35;
const DOT: i32 = 46;
const NEWLINE: i32 = 10;

const W: usize = 24;
const H: usize = 12;

#[derive(Clone, Copy, PartialEq)]
struct Grid {
    cells: [[bool; W]; H],
}

impl Grid {
    const fn empty() -> Self {
        Grid { cells: [[false; W]; H] }
    }

    fn set(&mut self, x: usize, y: usize) {
        self.cells[y % H][x % W] = true;
    }

    /// Places a pattern given as (dx, dy) offsets at (x, y).
    fn place(&mut self, x: usize, y: usize, pattern: &[(usize, usize)]) {
        for &(dx, dy) in pattern {
            self.set(x + dx, y + dy);
        }
    }

    fn alive(&self, x: isize, y: isize) -> bool {
        let x = x.rem_euclid(W as isize) as usize;
        let y = y.rem_euclid(H as isize) as usize;
        self.cells[y][x]
    }

    fn neighbours(&self, x: usize, y: usize) -> u32 {
        let mut n = 0;
        for dy in -1isize..=1 {
            for dx in -1isize..=1 {
                if !(dx == 0 && dy == 0) && self.alive(x as isize + dx, y as isize + dy) {
                    n += 1;
                }
            }
        }
        n
    }

    fn step(&self) -> Grid {
        let mut next = Grid::empty();
        for y in 0..H {
            for x in 0..W {
                let n = self.neighbours(x, y);
                next.cells[y][x] = match (self.cells[y][x], n) {
                    (true, 2) | (_, 3) => true,
                    _ => false,
                };
            }
        }
        next
    }

    fn population(&self) -> usize {
        let rows: &[[bool; W]] = &self.cells;
        rows.iter()
            .map(|row| {
                let row: &[bool] = row;
                row.iter().filter(|&&c| c).count()
            })
            .sum()
    }

    fn bounding_box(&self) -> Option<(usize, usize, usize, usize)> {
        let mut bbox: Option<(usize, usize, usize, usize)> = None;
        let rows: &[[bool; W]] = &self.cells;
        for (y, row) in rows.iter().enumerate() {
            let row: &[bool] = row;
            for (x, &cell) in row.iter().enumerate() {
                if !cell {
                    continue;
                }
                bbox = Some(match bbox {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
        bbox
    }

    fn print(&self) {
        for row in &self.cells {
            for &cell in row {
                unsafe {
                    putchar(if cell { HASH } else { DOT });
                }
            }
            unsafe {
                putchar(NEWLINE);
            }
        }
    }
}

const GLIDER: [(usize, usize); 5] = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
const BLINKER: [(usize, usize); 3] = [(0, 0), (1, 0), (2, 0)];
const BLOCK: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];
const R_PENTOMINO: [(usize, usize); 5] = [(1, 0), (2, 0), (0, 1), (1, 1), (1, 2)];

fn run(name: *const i8, mut grid: Grid, generations: u32, show: bool) -> Grid {
    unsafe {
        printf(c"== %s ==\n".as_ptr(), name);
    }
    let mut prev = grid;
    for generation in 0..generations {
        if show {
            unsafe {
                printf(c"generation %u, population %zu\n".as_ptr(), generation, grid.population());
            }
            grid.print();
        }
        let next = grid.step();
        if next == grid {
            unsafe {
                printf(c"still life after %u generations\n".as_ptr(), generation);
            }
            return grid;
        }
        if next == prev && generation > 0 {
            unsafe {
                printf(c"period-2 oscillator at generation %u\n".as_ptr(), generation);
            }
            return grid;
        }
        prev = grid;
        grid = next;
    }
    grid
}

fn main() {
    let mut glider = Grid::empty();
    glider.place(1, 1, &GLIDER);
    let after = run(c"glider".as_ptr(), glider, 5, true);
    if let Some((x0, y0, x1, y1)) = after.bounding_box() {
        unsafe {
            printf(c"glider box: (%zu,%zu)-(%zu,%zu)\n".as_ptr(), x0, y0, x1, y1);
        }
    }

    let mut blinker = Grid::empty();
    blinker.place(10, 5, &BLINKER);
    run(c"blinker".as_ptr(), blinker, 10, true);

    let mut block = Grid::empty();
    block.place(3, 3, &BLOCK);
    run(c"block".as_ptr(), block, 10, false);

    let mut r = Grid::empty();
    r.place(W / 2, H / 2, &R_PENTOMINO);
    let end = run(c"r-pentomino".as_ptr(), r, 60, false);
    let pop = end.population();
    unsafe {
        printf(c"r-pentomino population after 60: %zu\n".as_ptr(), pop);
    }
    end.print();
}
