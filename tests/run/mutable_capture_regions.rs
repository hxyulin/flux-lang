trait Write {
    fn write(&mut self, n: u32);
}
impl Write for Vec<u32> {
    fn write(&mut self, n: u32) {
        self.push(n)
    }
}
impl dyn Write + '_ {
    fn value<T: Copy + Into<u32>>(&mut self, v: &T) {
        self.write((*v).into())
    }
}
struct S(u32);
impl S {
    fn inc(&mut self) {
        self.0 += 1
    }
    fn with_closure(&mut self) {
        (|| self.inc())()
    }
}
fn compacted(r: &mut u32) -> Result<(), String> {
    let value = String::from("owned");
    drop(value);
    Ok::<(), String>(())?;
    for _ in 0..2 {
        let mut c = || *r += 1;
        c()
    }
    Ok(())
}
fn nested(r: &mut u32) {
    let mut outer = || {
        let mut inner = || *r += 3;
        inner()
    };
    outer()
}
fn through_object(buffer: &mut dyn Write) {
    let mut write = |n: u32| {
        buffer.value(&n);
        buffer.write(n + 1)
    };
    write(7);
    write(9)
}
fn get(v: &mut Vec<u32>, ok: bool) -> Result<&mut Vec<u32>, String> {
    if ok {
        Ok(v)
    } else {
        Err(String::from("unavailable"))
    }
}
fn append(v: &mut Vec<u32>, w: u32) -> Result<(), String> {
    v.push(w);
    Ok(())
}
fn branch(v: &mut Vec<u32>, ok: bool, words: &[u32]) -> Result<(), String> {
    match get(v, ok) {
        Ok(bytes) => {
            bytes.reserve(words.len());
            for word in words {
                bytes.push(*word)
            }
        }
        Err(_) => {
            for (i, word) in words.iter().enumerate() {
                append(v, *word).map_err(|e| format!("{} {}", i, e))?;
            }
        }
    }
    Ok(())
}
fn main() {
    let mut s = S(10);
    s.with_closure();
    nested(&mut s.0);
    compacted(&mut s.0).unwrap();
    assert_eq!(s.0, 16);
    let mut v = Vec::new();
    through_object(&mut v);
    branch(&mut v, true, &[11, 12]).unwrap();
    branch(&mut v, false, &[13, 14]).unwrap();
    assert_eq!(v.as_slice(), &[7, 8, 9, 10, 11, 12, 13, 14]);
    println!("mutable captures {} {:?}", s.0, v);
}
