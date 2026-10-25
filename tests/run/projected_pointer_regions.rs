trait Next {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}

struct Source<'a> {
    p: &'a fn(&u32) -> &u32,
}

impl<'a> Next for Source<'a> {
    type Item = &'a fn(&u32) -> &u32;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.p)
    }
}

fn get<'a>(source: &mut Source<'a>) -> Option<&'a fn(&'static u32) -> &'static u32> {
    source.next()
}

struct Rows<'a, 'b> {
    left: &'a u32,
    p: &'a fn(&u32) -> &u32,
    right: &'b u32,
}

impl<'a, 'b> Next for Rows<'a, 'b> {
    type Item = (&'a u32, &'a fn(&u32) -> &u32, &'b u32);
    fn next(&mut self) -> Option<Self::Item> {
        Some((self.left, self.p, self.right))
    }
}

fn rows<'a, 'b>(
    source: &mut Rows<'a, 'b>,
) -> Option<(&'a u32, &'a fn(&'static u32) -> &'static u32, &'b u32)> {
    source.next()
}

struct Sig {
    params: Box<[u32]>,
    ret: u32,
}

fn compare(input: &Sig, ranges: &[usize]) -> Option<usize> {
    let norm = |sig: &Sig| Sig {
        params: sig.params.clone(),
        ret: sig.ret,
    };
    let (sig, expected) = (&norm(input), norm(input));
    let types = sig.params.iter().zip(&expected.params);
    types
        .zip(ranges.iter().copied())
        .chain([((&sig.ret, &expected.ret), 7)])
        .find(|((got, want), _)| **got != **want)
        .map(|(_, range)| range)
}

fn id(x: &u32) -> &u32 {
    x
}

fn main() {
    static VALUE: u32 = 41;
    let pointer: fn(&u32) -> &u32 = id;
    let projected = get(&mut Source { p: &pointer }).unwrap();
    assert_eq!(*projected(&VALUE), 41);
    let left = 13;
    let (a, callback, b) = {
        let right = 29;
        let mut source = Rows {
            left: &left,
            p: &pointer,
            right: &right,
        };
        let (a, callback, b) = rows(&mut source).unwrap();
        assert_eq!(*b, 29);
        (a, callback, *b)
    };
    assert_eq!(*a, 13);
    assert_eq!(*callback(&VALUE), 41);
    let signature = Sig {
        params: vec![1, 2].into_boxed_slice(),
        ret: 3,
    };
    assert!(compare(&signature, &[5, 11]).is_none());
    println!("projected rows {} {} {}", a, callback(&VALUE), b);
}
