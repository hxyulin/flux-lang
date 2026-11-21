fn main() {
    let slice: &[u8] = b"tail";
    let _ = [b"x", b"yz", slice];
}
