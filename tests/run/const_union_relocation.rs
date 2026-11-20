#[repr(C)]
union DescriptorPtr {
    word: *const u64,
    bytes: *const [u8; 8],
}

#[repr(C)]
struct Header {
    flags: u32,
    descriptor: DescriptorPtr,
}

// SAFETY: The sole static header points to an immutable static word.
unsafe impl Sync for Header {}

static WORD: u64 = 42;
const HEADER: Header = Header {
    flags: 0,
    descriptor: DescriptorPtr { word: &WORD },
};
static BLOCK: Header = {
    let mut header = HEADER;
    header.flags = 7;
    header
};
// SAFETY: `BLOCK` initializes the `word` field.
const WORD_PTR: &*const u64 = unsafe { &BLOCK.descriptor.word };

union Flags {
    value: u32,
}
const FLAGS: u32 = {
    let flags = Flags { value: 7 };
    // SAFETY: This reads the field initialized above.
    unsafe { flags.value }
};

fn main() {
    // SAFETY: Both headers initialize `word` with a pointer to `WORD`.
    unsafe {
        assert_eq!(HEADER.descriptor.word, &raw const WORD);
        assert_eq!(BLOCK.descriptor.word, &raw const WORD);
        assert_eq!(*BLOCK.descriptor.word, 42);
    }
    assert_eq!(*WORD_PTR, &raw const WORD);
    assert_eq!(BLOCK.flags, FLAGS);
    println!("{FLAGS}");
}
