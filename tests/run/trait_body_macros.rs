//! Macro calls among a trait's items expand to its associated functions,
//! constants and types: required and provided methods both, including
//! through a path to a macro defined later and through trait objects.

macro_rules! getter {
    ($name:ident) => {
        fn $name(&self) -> u32;
    };
}

macro_rules! provided {
    ($name:ident, $base:ident, $factor:expr) => {
        fn $name(&self) -> u32 {
            self.$base() * $factor
        }
    };
}

trait Shape {
    getter!(width);
    getter!(height);
    provided!(double_width, width, 2);
    later::assoc!();
    fn area(&self) -> u32 {
        self.width() * self.height() + Self::EXTRA
    }
}

struct Rect(u32, u32);

impl Shape for Rect {
    type Unit = u8;
    const EXTRA: u32 = 1;
    fn width(&self) -> u32 {
        self.0
    }
    fn height(&self) -> u32 {
        self.1
    }
}

mod later {
    macro_rules! assoc {
        () => {
            type Unit;
            const EXTRA: u32;
        };
    }
    pub(crate) use assoc;
}

fn describe<S: Shape>(shape: &S) -> u32 {
    shape.area() + shape.double_width()
}

trait Measured {
    getter!(size);
    provided!(twice, size, 2);
}

impl Measured for Rect {
    fn size(&self) -> u32 {
        self.0 + self.1
    }
}

fn dynamic(value: &dyn Measured) -> u32 {
    value.twice()
}

fn main() {
    let rect = Rect(3, 4);
    let unit: <Rect as Shape>::Unit = 7;
    println!("{} {} {} {}", rect.area(), describe(&rect), unit, dynamic(&rect));
    std::process::exit(describe(&rect) as i32);
}
