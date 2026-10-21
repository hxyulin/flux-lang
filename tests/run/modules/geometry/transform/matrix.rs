//! `modules/geometry/transform/matrix.rs`: three levels of files down.

pub const IDENTITY_SCALE: i32 = 1;

pub struct Matrix {
    sx: i32,
    sy: i32,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        sx: IDENTITY_SCALE,
        sy: IDENTITY_SCALE,
    };

    pub fn scale(sx: i32, sy: i32) -> Matrix {
        Matrix { sx, sy }
    }

    pub fn sx(&self) -> i32 {
        self.sx
    }

    pub fn sy(&self) -> i32 {
        self.sy
    }

    pub fn then(&self, other: &Matrix) -> Matrix {
        Matrix {
            sx: self.sx * other.sx,
            sy: self.sy * other.sy,
        }
    }
}
