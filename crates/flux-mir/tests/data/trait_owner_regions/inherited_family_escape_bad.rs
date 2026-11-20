trait Read<'a> {
    type Item: 'a;
}

trait Chain<'a>: Read<'a> {
    fn values(&self) -> &'a [Self::Item];
}

fn escape<'a: 'a, R: Chain<'a>>(rule: R) -> &'static [<R as Read<'a>>::Item] {
    rule.values()
}

fn main() {}
