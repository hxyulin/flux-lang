struct List {
    head: i32,
    tail: List,
}

struct T(i32, (T, i32));

struct Ok {
    list: Wrapper,
}

struct Wrapper(i32);

fn main() {}
