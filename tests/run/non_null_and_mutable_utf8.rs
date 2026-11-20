use core::num::NonZeroUsize;
use core::ptr::NonNull;

const POINTER: NonNull<u32> = match NonZeroUsize::new(32) {
    Some(addr) => NonNull::without_provenance(addr),
    None => panic!("expected a nonzero address"),
};

const TEXT_LENGTH: usize = {
    let mut bytes = *b"flux";
    match core::str::from_utf8_mut(&mut bytes) {
        Ok(text) => text.len(),
        Err(_) => panic!("expected valid utf-8"),
    }
};

fn main() {
    let address = POINTER.addr().get();
    assert_eq!(address, 32);
    let ptr = NonNull::<u32>::without_provenance(NonZeroUsize::new(address + 16).unwrap());
    assert_eq!(ptr.addr().get(), 48);

    let mut bytes = *b"flux";
    let text = core::str::from_utf8_mut(&mut bytes).unwrap();
    assert_eq!(text.len(), TEXT_LENGTH);
    text.make_ascii_uppercase();
    assert_eq!(&bytes, b"FLUX");
    let mut invalid = [b'a', 0xff];
    let error = core::str::from_utf8_mut(&mut invalid).unwrap_err();
    assert_eq!(error.valid_up_to(), 1);
    assert_eq!(error.error_len(), Some(1));
    println!("{address} {bytes:?} {error:?}");
}
