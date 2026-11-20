trait StaticOwner: 'static {}
trait Select: StaticOwner {
    type Output;
}

trait Allocate {
    type Buffer<T>;
}

struct ArrayAllocator;
impl Allocate for ArrayAllocator {
    type Buffer<T> = [T; 3];
}

struct Matrix<T, Storage>(T, Storage);
type Vector<T> = Matrix<T, <ArrayAllocator as Allocate>::Buffer<T>>;

struct Scalar;
impl StaticOwner for Scalar {}
impl Select for Scalar {
    type Output = Vector<u32>;
}

struct Holder {
    value: <Scalar as Select>::Output,
}

fn main() {
    let h = Holder {
        value: Matrix(1, [13, 17, 19]),
    };
    println!("{}", h.value.0 + h.value.1[1]);
}
