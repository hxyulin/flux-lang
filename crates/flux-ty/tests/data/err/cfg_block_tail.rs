fn semicolon() -> u32 {
    { 7 };
    #[cfg(any())]
    { 9 }
}
fn item() -> u32 {
    { 7 }
    const VALUE: u32 = 1;
    #[cfg(any())]
    { 9 }
}
fn enabled() -> u32 {
    { 7 }
    #[cfg(all())]
    { 9 }
}
fn bare_semicolon() -> u32 {
    { 7 }
    #[cfg(any())]
    const VALUE: u32 = 1;
    ;
}
fn unit() {
    { 7 }
    #[cfg(any())]
    { 9 }
}
fn enabled_let() -> u32 {
    { 7 }
    let value = 9;
    #[cfg(any())]
    { 11 }
}
fn main() {}
