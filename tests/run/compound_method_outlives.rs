enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R, T> AsRef<[T]> for Either<L, R>
where
    L: AsRef<[T]>,
    R: AsRef<[T]>,
{
    fn as_ref(&self) -> &[T] {
        match self {
            Self::Left(value) => value.as_ref(),
            Self::Right(value) => value.as_ref(),
        }
    }
}

impl<L, R, T> AsMut<[T]> for Either<L, R>
where
    L: AsMut<[T]>,
    R: AsMut<[T]>,
{
    fn as_mut(&mut self) -> &mut [T] {
        match self {
            Self::Left(value) => value.as_mut(),
            Self::Right(value) => value.as_mut(),
        }
    }
}

fn main() {
    let a = 20;
    let b = 22;
    let mut left: Either<Box<[&i32]>, Box<[&i32]>> = Either::Left(vec![&a, &b].into_boxed_slice());
    let mut right: Either<Box<[&i32]>, Box<[&i32]>> = Either::Right(vec![&b].into_boxed_slice());
    left.as_mut().swap(0, 1);
    let values = right.as_mut();
    values[0] = &a;
    assert_eq!(left.as_ref(), [&b, &a]);
    assert_eq!(right.as_ref(), [&a]);
    println!("{}", *left.as_ref()[0] + *right.as_ref()[0]);
}
