use serde::{Deserialize, Serialize};
use std::rc::Rc;
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Record { name: Rc<String>, values: Vec<i64> }
fn main() {
    let record = Record { name: Rc::new("sample".to_string()), values: vec![-3, 0, 7] };
    let encoded = serde_json::to_string(&record).unwrap();
    let decoded: Record = serde_json::from_str(&encoded).unwrap();
    assert_eq!(record, decoded);
    let mut map = serde_json::Map::new();
    map.insert("z".into(), 1.into());
    map.insert("a".into(), 2.into());
    assert_eq!(serde_json::to_string(&map).unwrap(), "{\"z\":1,\"a\":2}");
    let raw = serde_json::value::RawValue::from_string("{\"unparsed\":[1,2]}".to_string()).unwrap();
    assert_eq!(raw.get(), "{\"unparsed\":[1,2]}");
    println!("{} {}", encoded, raw.get());
}
