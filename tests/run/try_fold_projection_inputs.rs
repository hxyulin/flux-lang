use std::collections::BTreeMap;

enum Value {
    Object(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Number(u32),
    Null,
}
fn parse_index(s: &str) -> Option<usize> {
    if s.starts_with('+') || (s.starts_with('0') && s.len() != 1) {
        return None;
    }
    s.parse().ok()
}
impl Value {
    // These traversal bodies retain serde_json's split/map/try_fold shape.
    fn pointer(&self, pointer: &str) -> Option<&Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        pointer
            .split('/')
            .skip(1)
            .map(|x| x.replace("~1", "/").replace("~0", "~"))
            .try_fold(self, |target, token| match target {
                Value::Object(map) => map.get(&token),
                Value::Array(list) => parse_index(&token).and_then(|x| list.get(x)),
                _ => None,
            })
    }
    fn pointer_mut(&mut self, pointer: &str) -> Option<&mut Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        pointer
            .split('/')
            .skip(1)
            .map(|x| x.replace("~1", "/").replace("~0", "~"))
            .try_fold(self, |target, token| match target {
                Value::Object(map) => map.get_mut(&token),
                Value::Array(list) => parse_index(&token).and_then(move |x| list.get_mut(x)),
                _ => None,
            })
    }
    fn number(&self) -> Option<u32> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}
fn main() {
    let mut nested = BTreeMap::new();
    nested.insert(String::from("~key"), Value::Number(29));
    let mut object = BTreeMap::new();
    object.insert(
        String::from("a/b"),
        Value::Array(vec![Value::Number(17), Value::Object(nested)]),
    );
    object.insert(String::from(""), Value::Number(41));
    object.insert(String::from("雪"), Value::Number(43));
    object.insert(String::from("null"), Value::Null);
    let mut root = Value::Object(object);
    assert_eq!(
        root.pointer("").unwrap() as *const Value,
        &root as *const Value
    );
    assert_eq!(root.pointer("/a~1b/0").unwrap().number(), Some(17));
    assert_eq!(root.pointer("/a~1b/1/~0key").unwrap().number(), Some(29));
    assert_eq!(root.pointer("/").unwrap().number(), Some(41));
    assert_eq!(root.pointer("/雪").unwrap().number(), Some(43));
    for bad in [
        "missing", "/missing", "/a~1b/+0", "/a~1b/00", "/a~1b/9", "/null/x",
    ] {
        assert!(root.pointer(bad).is_none());
        assert!(root.pointer_mut(bad).is_none());
    }
    *root.pointer_mut("/a~1b/1/~0key").unwrap() = Value::Number(37);
    *root.pointer_mut("/雪").unwrap() = Value::Number(47);
    assert_eq!(root.pointer("/a~1b/1/~0key").unwrap().number(), Some(37));
    assert_eq!(root.pointer("/雪").unwrap().number(), Some(47));
    let value = 53;
    let result: Result<&u32, ()> = [(), ()].iter().try_fold(&value, |accum, _| Ok(accum));
    assert_eq!(result.unwrap() as *const u32, &value as *const u32);
    let mut calls = 0;
    let stopped: Option<&u32> = [1, 2, 3].iter().try_fold(&value, |accum, item| {
        calls += 1;
        if *item == 2 { None } else { Some(accum) }
    });
    assert!(stopped.is_none());
    assert_eq!(calls, 2);
    println!(
        "try fold {} {} {calls}",
        root.pointer("/a~1b/1/~0key").unwrap().number().unwrap(),
        root.pointer("/雪").unwrap().number().unwrap()
    );
}
