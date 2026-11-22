fn collect<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<Vec<&'a str>> {
    (0..2).map(|_| parts.next()).collect()
}

fn main() {
    let mut parts = ["first", "second"].into_iter();
    let mut values = collect(&mut parts).unwrap();
    let last = values.push_mut("third");
    *last = "fourth";
    println!("{values:?}");

    let mut units = Vec::new();
    let _: &mut () = units.push_mut(());
    println!("{}", units.len());
}
