struct Field<'a> { name: &'a str }
struct Variant<'a> { fields: Vec<Field<'a>> }
enum Data<'a> { Enum(Vec<Variant<'a>>), Struct(Vec<Field<'a>>) }
impl<'a> Data<'a> {
    fn all_fields(&'a self) -> Box<dyn Iterator<Item = &'a Field<'a>> + 'a> {
        match self {
            Self::Enum(variants) => Box::new(variants.iter().flat_map(|variant| variant.fields.iter())),
            Self::Struct(fields) => Box::new(fields.iter()),
        }
    }
}
fn main() {
    let data = Data::Enum(vec![Variant { fields: vec![Field { name: "a" }, Field { name: "b" }] }]);
    let simple = Data::Struct(vec![Field { name: "c" }]);
    for field in data.all_fields().chain(simple.all_fields()) { println!("{}", field.name); }
}
