use std::ops::Mul;
trait Scalar {
    type Rotation;
    type Vector;
}
struct Small;
#[derive(Clone, Copy)]
struct Rot(u32);

impl Scalar for Small {
    type Rotation = Rot;
    type Vector = [u32; 2];
}
impl Mul<Rot> for Rot {
    type Output = Rot;
    fn mul(self, rhs: Rot) -> Rot {
        Rot(self.0 * rhs.0)
    }
}
impl Mul<[u32; 2]> for Rot {
    type Output = [u32; 2];
    fn mul(self, rhs: [u32; 2]) -> [u32; 2] {
        [self.0 * rhs[0], self.0 * rhs[1]]
    }
}

trait RotationOps<N: Scalar>:
    Copy + Mul<N::Rotation, Output = N::Rotation> + Mul<N::Vector, Output = N::Vector>
{
}
impl RotationOps<Small> for Rot {}

fn apply<R: RotationOps<Small>>(rotation: R) -> [u32; 2] {
    rotation * [3_u32, 5]
}

fn main() {
    let values = apply(Rot(7));
    println!("{} {}", values[0], values[1]);
}
