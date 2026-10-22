// `?Sized` where rustc does not allow it.

fn in_where<T>(x: &T)
where
    T: ?Sized,
{
}

fn twice<T: ?Sized + ?Sized>(x: &T) {}

fn not_sized<T: ?Copy>(x: &T) {}

fn elsewhere<T>(x: &T)
where
    Option<T>: ?Sized,
{
}

fn listed_and_where<T: ?Sized>(x: &T)
where
    T: ?Sized,
{
}

trait Sup: ?Sized {}

trait Assoc {
    type Out: ?Sized;
}

fn main() {}
