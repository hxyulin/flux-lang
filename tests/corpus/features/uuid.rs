use uuid::Uuid;
fn main() {
    let generated = Uuid::new_v4();
    assert_eq!(generated.get_version_num(), 4);
    let encoded = serde_json::to_string(&generated).unwrap();
    let decoded: Uuid = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, generated);
    let known = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    println!("{} {}", known.hyphenated(), generated.get_version_num());
}
