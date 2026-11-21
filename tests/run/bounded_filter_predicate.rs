struct Field<'a>(&'a u32);

fn filtered<'a, 'b, F>(fields: &'b [Field<'a>], predicate: F) -> usize
where
    F: Fn(&&'b Field<'a>) -> bool,
{
    fields.iter().filter(predicate).count()
}

fn main() {
    let value = 2;
    let entries = [Field(&1), Field(&value)];
    let count = filtered(&entries, |field| *field.0 == 2);
    assert_eq!(count, 1);
    println!("{count}");
}
