use std::convert::Infallible;

fn collect<T, E>(items: impl IntoIterator<Item = Result<T, E>>) -> Result<Vec<T>, E> {
    items.into_iter().collect()
}

fn from_iter<T>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mapped = items.into_iter().map(Result::<T, Infallible>::Ok);
    match collect(mapped) {
        Ok(values) => values,
        Err(error) => match error {},
    }
}

fn main() {
    println!("{:?}", from_iter([1, 2, 3]));
}
