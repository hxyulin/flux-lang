use time::{macros::datetime, format_description::well_known::Rfc3339, OffsetDateTime};
fn main() {
    let date = datetime!(2024-02-29 23:45:06 +05:30);
    let encoded = date.format(&Rfc3339).unwrap();
    let decoded = OffsetDateTime::parse(&encoded, &Rfc3339).unwrap();
    assert_eq!(date, decoded);
    println!("{} {}", encoded, date.unix_timestamp());
}
