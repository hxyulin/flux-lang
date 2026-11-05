// Derived comparisons and hashes of enums: fieldless ones with many
// variants, explicit and negative discriminants, niche-carrying ones
// (`Option` of a fieldless enum, a `bool` or reference payload, nested
// options), generic ones, and fields whose comparison is counted, to
// check that different variants never compare their fields.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::hint::black_box;

// 254 variants: `Option<Kind>` stores `None` in the 255th tag value, and
// `Option<Option<Kind>>` in the 256th, past a byte's niche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Kind {
    K0,
    K1,
    K2,
    K3,
    K4,
    K5,
    K6,
    K7,
    K8,
    K9,
    K10,
    K11,
    K12,
    K13,
    K14,
    K15,
    K16,
    K17,
    K18,
    K19,
    K20,
    K21,
    K22,
    K23,
    K24,
    K25,
    K26,
    K27,
    K28,
    K29,
    K30,
    K31,
    K32,
    K33,
    K34,
    K35,
    K36,
    K37,
    K38,
    K39,
    K40,
    K41,
    K42,
    K43,
    K44,
    K45,
    K46,
    K47,
    K48,
    K49,
    K50,
    K51,
    K52,
    K53,
    K54,
    K55,
    K56,
    K57,
    K58,
    K59,
    K60,
    K61,
    K62,
    K63,
    K64,
    K65,
    K66,
    K67,
    K68,
    K69,
    K70,
    K71,
    K72,
    K73,
    K74,
    K75,
    K76,
    K77,
    K78,
    K79,
    K80,
    K81,
    K82,
    K83,
    K84,
    K85,
    K86,
    K87,
    K88,
    K89,
    K90,
    K91,
    K92,
    K93,
    K94,
    K95,
    K96,
    K97,
    K98,
    K99,
    K100,
    K101,
    K102,
    K103,
    K104,
    K105,
    K106,
    K107,
    K108,
    K109,
    K110,
    K111,
    K112,
    K113,
    K114,
    K115,
    K116,
    K117,
    K118,
    K119,
    K120,
    K121,
    K122,
    K123,
    K124,
    K125,
    K126,
    K127,
    K128,
    K129,
    K130,
    K131,
    K132,
    K133,
    K134,
    K135,
    K136,
    K137,
    K138,
    K139,
    K140,
    K141,
    K142,
    K143,
    K144,
    K145,
    K146,
    K147,
    K148,
    K149,
    K150,
    K151,
    K152,
    K153,
    K154,
    K155,
    K156,
    K157,
    K158,
    K159,
    K160,
    K161,
    K162,
    K163,
    K164,
    K165,
    K166,
    K167,
    K168,
    K169,
    K170,
    K171,
    K172,
    K173,
    K174,
    K175,
    K176,
    K177,
    K178,
    K179,
    K180,
    K181,
    K182,
    K183,
    K184,
    K185,
    K186,
    K187,
    K188,
    K189,
    K190,
    K191,
    K192,
    K193,
    K194,
    K195,
    K196,
    K197,
    K198,
    K199,
    K200,
    K201,
    K202,
    K203,
    K204,
    K205,
    K206,
    K207,
    K208,
    K209,
    K210,
    K211,
    K212,
    K213,
    K214,
    K215,
    K216,
    K217,
    K218,
    K219,
    K220,
    K221,
    K222,
    K223,
    K224,
    K225,
    K226,
    K227,
    K228,
    K229,
    K230,
    K231,
    K232,
    K233,
    K234,
    K235,
    K236,
    K237,
    K238,
    K239,
    K240,
    K241,
    K242,
    K243,
    K244,
    K245,
    K246,
    K247,
    K248,
    K249,
    K250,
    K251,
    K252,
    K253,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(i8)]
enum Signed {
    Low = -100,
    Minus = -1,
    Zero = 0,
    High = 100,
}

const BASE: isize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Offset {
    First = BASE,
    Second,
    Third = BASE * 3,
    Fourth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Flag {
    On(bool),
    A,
    B,
    C,
    D,
    E,
}

// The variant holding the niche sits between the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Middle {
    Before,
    Held(char),
    After,
    Last,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Named {
    Text(&'static str),
    Empty,
    Pair { left: u32, right: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
enum Real {
    Value(f64),
    Many(f32, f64),
    Nothing,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Generic<T> {
    Some(T),
    Both(T, T),
    None,
}

static mut FIELD_EQS: u32 = 0;

#[derive(Debug)]
struct Counted(u32);

impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        unsafe { FIELD_EQS += 1 };
        self.0 == other.0
    }
}

#[derive(Debug, PartialEq)]
enum Tracked {
    One(Counted),
    Two(Counted, Counted),
    Three { inner: Counted },
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Single {
    Only(u8),
}

fn hash<T: Hash>(value: &T) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

fn kind(i: usize) -> Kind {
    let all = [Kind::K0, Kind::K1, Kind::K2, Kind::K100, Kind::K200, Kind::K252, Kind::K253];
    all[i % all.len()]
}

fn main() {
    let mut eqs = 0;
    let mut lts = 0;
    for i in 0..7 {
        for j in 0..7 {
            let (a, b) = (black_box(kind(i)), black_box(kind(j)));
            eqs += (a == b) as u32;
            lts += (a < b) as u32;
            let (x, y) = (black_box(Some(a)), black_box(if j == 3 { None } else { Some(b) }));
            eqs += (x == y) as u32 * 10;
            lts += (x < y) as u32 * 10;
            let (p, q) = (black_box(Some(x)), black_box(if i == 2 { None } else { Some(y) }));
            eqs += (p == q) as u32 * 100;
            lts += (p.cmp(&q) == Ordering::Less) as u32 * 100;
        }
    }
    println!("kinds {eqs} {lts} {} {}", Kind::K253 > Kind::K252, hash(&Kind::K7) == hash(&Kind::K7));
    let none: Option<Kind> = black_box(None);
    println!("{:?} {:?} {}", none, Some(Kind::K253), none == Some(Kind::K253));

    let signed = [Signed::Low, Signed::Minus, Signed::Zero, Signed::High];
    for a in signed {
        let row: Vec<String> = signed.iter().map(|b| format!("{}{:?}", (a == *b) as u8, a.cmp(b))).collect();
        println!("{:?} {} {}", a, a as i8, row.join(" "));
    }
    println!("{} {}", hash(&Signed::Low) == hash(&Signed::Low), hash(&Signed::Low) == hash(&Signed::High));
    let offsets = [Offset::First, Offset::Second, Offset::Third, Offset::Fourth];
    for a in offsets {
        println!("{:?} {} {:?} {}", a, a as isize, a.partial_cmp(&Offset::Third), a == Offset::Second);
    }

    let flags = [Flag::On(true), Flag::On(false), Flag::A, Flag::C, Flag::E];
    let mut flag_eq = 0;
    for a in flags {
        for b in flags {
            flag_eq = flag_eq * 2 + (black_box(a) == black_box(b)) as u64;
            flag_eq += (Some(a) < Some(b)) as u64;
        }
    }
    let maybe: Option<Flag> = black_box(None);
    println!("flags {flag_eq} {} {}", maybe == Some(Flag::E), maybe < Some(Flag::On(false)));

    let middles = [Middle::Before, Middle::Held('x'), Middle::Held('\u{10ffff}'), Middle::After, Middle::Last];
    for a in middles {
        let row: Vec<String> = middles
            .iter()
            .map(|b| format!("{}{:?}", (black_box(a) == *b) as u8, a.cmp(b)))
            .collect();
        println!("{:?} {}", a, row.join(" "));
    }
    let middle_none: Option<Middle> = black_box(None);
    println!("{} {}", middle_none < Some(Middle::Before), middle_none == Some(Middle::Last));

    let names = [
        Named::Text("a"),
        Named::Text("b"),
        Named::Empty,
        Named::Pair { left: 1, right: None },
        Named::Pair { left: 1, right: Some(2) },
    ];
    for a in &names {
        let row: Vec<String> = names.iter().map(|b| format!("{}{:?}", (a == b) as u8, a.cmp(b))).collect();
        println!("{}", row.join(" "));
    }
    let opt_names = [None, Some(Named::Empty), Some(Named::Text("a"))];
    println!("{} {}", opt_names[0] == opt_names[1], opt_names[2] == Some(Named::Text("a")));

    let reals = [Real::Value(f64::NAN), Real::Value(1.0), Real::Many(0.0, -0.0), Real::Nothing];
    for a in &reals {
        let row: Vec<String> = reals.iter().map(|b| format!("{}{:?}", (a == b) as u8, a.partial_cmp(b))).collect();
        println!("{}", row.join(" "));
    }
    println!("{}", Real::Many(0.0, 0.0) == Real::Many(-0.0, 0.0));

    let generic = [Generic::Some(3u64), Generic::Both(1, 2), Generic::None, Generic::Both(1, 3)];
    for a in &generic {
        let row: Vec<String> = generic.iter().map(|b| format!("{}{:?}", (a == b) as u8, a.cmp(b))).collect();
        println!("{}", row.join(" "));
    }
    let nested: [Generic<Option<Kind>>; 3] = [Generic::Some(None), Generic::Some(Some(Kind::K5)), Generic::None];
    println!("{} {}", nested[0] == nested[1], nested[1] == Generic::Some(Some(Kind::K5)));

    let tracked = [
        Tracked::One(Counted(1)),
        Tracked::Two(Counted(1), Counted(2)),
        Tracked::Three { inner: Counted(1) },
        Tracked::Two(Counted(2), Counted(2)),
    ];
    let mut tracked_eq = 0;
    for a in &tracked {
        for b in &tracked {
            tracked_eq = tracked_eq * 2 + (a == b) as u32;
        }
    }
    println!("tracked {tracked_eq} {}", unsafe { FIELD_EQS });
    println!("{} {}", Single::Only(1) == Single::Only(1), Single::Only(1) != Single::Only(2));

    std::process::exit(((eqs + lts) % 256) as i32);
}
