// Appended inside the unchanged generator/ty.rs module by the host harness.
pub(crate) fn run_selfhost_checks() {
    for ty in IntTy::ALL {
        let bits = ty.bits();
        assert!(bits == 8 || bits == 16 || bits == 32 || bits == 64 || bits == 128);
        assert_eq!(
            ty.truncate(u128::MAX),
            if bits == 128 {
                u128::MAX
            } else {
                (1u128 << bits) - 1
            }
        );
        assert_eq!(ty.value(0), 0);
        assert_eq!(ty.cmp(0, 1), std::cmp::Ordering::Less);
        if ty.signed() {
            assert_eq!(ty.value(ty.truncate(ty.min() as u128)), ty.min());
            assert_eq!(
                ty.cmp(ty.min() as u128, ty.max_bits()),
                std::cmp::Ordering::Less
            );
        } else {
            assert_eq!(ty.min(), 0);
            assert_eq!(ty.cmp(ty.max_bits(), 0), std::cmp::Ordering::Greater);
        }
        println!(
            "integer {} {bits} {} {} {}",
            ty.name(),
            ty.signed(),
            ty.min(),
            ty.max_bits()
        );
        for value in [0, 1, u128::MAX, ty.min() as u128, ty.max_bits()] {
            println!(
                "literal {} {} {} {:?}",
                int_literal(ty, value),
                int_pattern_literal(ty, value),
                int_numeric_literal(ty, value),
                ty.cmp(value, 0)
            );
        }
        // Every weighted branch, including all three delta cases near powers of two.
        for choice in [0, 6, 16, 19, 22, 25, 27, 30] {
            for draw in 0..3 {
                let mut source = Source::replay(vec![choice, draw, draw, 0x12345678, 0x87654321]);
                let value = int_value(&mut source, ty);
                println!("value {} {choice} {draw} {value}", ty.name());
            }
        }
    }
    let mut source = Source::random(0x12345678);
    for _ in 0..48 {
        let (character, literal) = char_literal(&mut source);
        assert_eq!(literal, char_text(character));
        println!("character {} {literal}", character as u32);
    }
    for character in ['\'', '\\', '\0', '\n', ' ', '😀', '\u{10ffff}'] {
        println!("escaped {} {}", character as u32, char_text(character));
    }
    let cases = vec![
        Ty::Int(IntTy::I128),
        Ty::Bool,
        Ty::Char,
        Ty::Unit,
        Ty::Tuple(vec![]),
        Ty::Tuple(vec![Ty::Bool]),
        Ty::Tuple(vec![Ty::Bool, Ty::Char]),
        Ty::Array(Box::new(Ty::Param), 3),
        Ty::Struct(7),
        Ty::Enum(4),
        Ty::Option(Box::new(Ty::Bool)),
        Ty::Result(Box::new(Ty::Char), Box::new(Ty::Param)),
        Ty::Ref(Box::new(Ty::RefMut(Box::new(Ty::Param)))),
        Ty::Param,
        Ty::Fn(vec![], Box::new(Ty::Unit)),
        Ty::Fn(vec![Ty::Bool, Ty::Char], Box::new(Ty::Int(IntTy::U8))),
        Ty::Fn(vec![Ty::Param], Box::new(Ty::Unit)),
        Ty::Fn(vec![], Box::new(Ty::Param)),
    ];
    struct Digest(u64);
    impl core::hash::Hasher for Digest {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(*byte as u64);
            }
        }
    }
    for ty in cases {
        let cloned = ty.clone();
        assert_eq!(ty, cloned);
        let mut hash = Digest(0);
        core::hash::Hash::hash(&ty, &mut hash);
        println!("hash {}", core::hash::Hasher::finish(&hash));
        println!(
            "type {} {} {:?} {} {}",
            ty.name(),
            ty.strip_refs().name(),
            ty.int(),
            ty.is_scalar(),
            ty.contains_param()
        );
    }
    assert_ne!(Ty::Tuple(vec![Ty::Bool]), Ty::Tuple(vec![Ty::Char]));
    assert_ne!(
        Ty::Array(Box::new(Ty::Bool), 2),
        Ty::Array(Box::new(Ty::Bool), 3)
    );
}
