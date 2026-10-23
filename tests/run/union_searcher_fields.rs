use std::mem::{align_of, size_of};

#[derive(Copy, Clone)]
enum Shift {
    Small { period: usize },
    Large { shift: usize },
}
#[derive(Copy, Clone)]
struct Finder {
    shift: Shift,
    enabled: bool,
}
#[derive(Copy, Clone)]
union PrefilterKind {
    byte: u8,
    words: [usize; 4],
}
#[derive(Copy)]
struct Prefilter {
    call: unsafe fn(&Prefilter, &[u8], &mut usize) -> Option<usize>,
    kind: PrefilterKind,
}
// Keep this test independent of derived Clone's function pointer obligations.
impl Clone for Prefilter {
    fn clone(&self) -> Self {
        *self
    }
}
#[derive(Copy, Clone)]
struct WithPrefilter {
    finder: Finder,
    prefilter: Prefilter,
}
#[derive(Copy, Clone)]
union SearcherKind {
    empty: (),
    byte: u8,
    flag: bool,
    two_way: Finder,
    with_prefilter: WithPrefilter,
}
#[derive(Copy, Clone)]
union Function {
    call: unsafe fn(&Prefilter, &[u8], &mut usize) -> Option<usize>,
    byte: u8,
}
#[derive(Copy, Clone)]
union Niche {
    flag: Option<bool>,
    byte: u8,
}
union Empty {
    unit: (),
}
union Owned {
    finder: Finder,
    byte: u8,
}
impl Drop for Owned {
    fn drop(&mut self) {
        println!("drop {}", unsafe { self.byte });
    }
}
unsafe fn prefilter(p: &Prefilter, bytes: &[u8], seen: &mut usize) -> Option<usize> {
    *seen += bytes.len();
    let byte = unsafe { p.kind.byte };
    if bytes[0] == byte { Some(*seen) } else { None }
}
fn bounce(kind: SearcherKind) -> SearcherKind {
    kind
}
fn shift(f: Finder) -> usize {
    if f.enabled {
        match f.shift {
            Shift::Small { period } => period,
            Shift::Large { shift } => shift + 10,
        }
    } else {
        0
    }
}
fn main() {
    println!(
        "layout {} {} {} {} {} {} {} {}",
        size_of::<SearcherKind>(),
        align_of::<SearcherKind>(),
        size_of::<Option<SearcherKind>>(),
        size_of::<Function>(),
        size_of::<Option<Function>>(),
        size_of::<Niche>(),
        size_of::<Option<Niche>>(),
        size_of::<Empty>()
    );
    let unit = Empty { unit: () };
    println!("unit {:?}", unsafe { unit.unit });
    let empty = bounce(SearcherKind { empty: () }.clone());
    println!("empty {:?}", unsafe { empty.empty });
    // This byte is invalid as a bool, enum discriminant or function pointer.
    // Whole union copies must never decode any inactive field.
    let inactive = bounce(SearcherKind { byte: 255 });
    let copy = inactive.clone();
    let captured = move || unsafe { copy.byte };
    println!("inactive {} {}", unsafe { inactive.byte }, captured());
    let function = Function { byte: 255 };
    let function_copy = function.clone();
    println!("inactive function {}", unsafe { function_copy.byte });
    let flag = bounce(SearcherKind { flag: true });
    println!("flag {}", unsafe { flag.flag });
    let finder = bounce(SearcherKind {
        two_way: Finder {
            shift: Shift::Small { period: 7 },
            enabled: true,
        },
    });
    println!("small {}", shift(unsafe { finder.two_way }));
    let large = bounce(
        SearcherKind {
            two_way: Finder {
                shift: Shift::Large { shift: 13 },
                enabled: true,
            },
        }
        .clone(),
    );
    println!("large {}", shift(unsafe { large.two_way }));
    let mut selected = bounce(SearcherKind {
        with_prefilter: WithPrefilter {
            finder: Finder {
                shift: Shift::Small { period: 11 },
                enabled: false,
            },
            prefilter: Prefilter {
                call: prefilter,
                kind: PrefilterKind { byte: 5 },
            },
        },
    });
    let original = bounce(selected.clone());
    unsafe {
        selected.with_prefilter.finder.enabled = true;
        let p = &selected.with_prefilter.prefilter;
        let mut seen = 0;
        println!(
            "prefilter {:?} {} {}",
            (p.call)(p, &[5, 8, 9], &mut seen),
            seen,
            shift(selected.with_prefilter.finder)
        );
        println!(
            "original {} {}",
            shift(original.with_prefilter.finder),
            original.with_prefilter.prefilter.kind.byte
        );
        let f = Function { call: p.call };
        let fcopy = f.clone();
        println!("function {:?}", (fcopy.call)(p, &[6], &mut seen));
        let base = &raw const selected as usize;
        println!(
            "offsets {} {}",
            &raw const selected.byte as usize - base,
            &raw const selected.with_prefilter as usize - base
        );
    }
    let kinds = [selected, original];
    println!(
        "array {} {}",
        unsafe { kinds[0].with_prefilter.prefilter.kind.byte },
        unsafe { kinds[1].with_prefilter.prefilter.kind.byte }
    );
    // A union suppresses its fields' niches even when every field has one.
    let niche = Niche { flag: Some(true) };
    println!("niche {:?}", unsafe { niche.clone().flag });
    let inactive_niche = Niche { byte: 255 };
    println!("inactive niche {}", unsafe { inactive_niche.clone().byte });
    let words = PrefilterKind {
        words: [17, 19, 23, 29],
    };
    println!("nested array {}", unsafe { words.clone().words[2] });
    // Moving a union with its own destructor never drops inactive field types.
    let owned = Owned { byte: 31 };
    let moved = owned;
    println!("owned {}", unsafe { moved.byte });
}
