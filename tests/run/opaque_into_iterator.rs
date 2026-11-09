struct Tool(Vec<(String, String)>);

impl Tool {
    fn env(&self) -> impl IntoIterator<Item = &(String, String)> {
        &self.0
    }
}

fn convert(tool: Tool) -> Vec<(String, String)> {
    tool.env()
        .into_iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn values(input: &[u32]) -> impl Iterator<Item = &u32> {
    input.iter()
}

fn main() {
    let pairs = convert(Tool(vec![("key".into(), "value".into())]));
    assert_eq!(pairs[0].0, "key");
    assert_eq!(pairs[0].1, "value");
    let copied: Vec<_> = values(&[20, 22]).into_iter().copied().collect();
    println!("{}", copied[0] + copied[1]);
}
