//! Symbols interned before any other, in this order, so that their numbers
//! are compile-time constants.

use crate::Symbol;

macro_rules! predefined {
    (
        keywords { $($kw:ident: $kw_str:literal,)* }
        symbols { $($sym:ident $(: $sym_str:literal)?,)* }
    ) => {
        // Keywords come first, then symbols; a symbol's number is its
        // position. Separate enums, because `kw::Fn` (`fn`) and `sym::Fn`
        // (the trait) share a name.
        #[allow(non_camel_case_types)]
        #[repr(u32)]
        enum KwIndex {
            $($kw,)*
        }

        #[allow(non_camel_case_types)]
        #[repr(u32)]
        enum SymIndex {
            $($sym,)*
        }

        const KEYWORDS: u32 = [$($kw_str),*].len() as u32;

        pub(crate) const STRINGS: &[&str] = &[
            $($kw_str,)*
            $(predefined!(@str $sym $(, $sym_str)?),)*
        ];

        /// Keywords and other words with special meaning.
        #[allow(non_upper_case_globals)]
        pub mod kw {
            use super::*;
            $(pub const $kw: Symbol = Symbol::predefined(KwIndex::$kw as u32);)*
        }

        /// Frequently used names: primitive types, core items, attributes.
        #[allow(non_upper_case_globals)]
        pub mod sym {
            use super::*;
            $(pub const $sym: Symbol = Symbol::predefined(KEYWORDS + SymIndex::$sym as u32);)*
        }
    };
    (@str $sym:ident) => { stringify!($sym) };
    (@str $sym:ident, $s:literal) => { $s };
}

predefined! {
    keywords {
        Empty: "",
        Underscore: "_",
        // Strict keywords.
        As: "as",
        Async: "async",
        Await: "await",
        Break: "break",
        Const: "const",
        Continue: "continue",
        Crate: "crate",
        Dyn: "dyn",
        Else: "else",
        Enum: "enum",
        Extern: "extern",
        False: "false",
        Fn: "fn",
        For: "for",
        If: "if",
        Impl: "impl",
        In: "in",
        Let: "let",
        Loop: "loop",
        Match: "match",
        Mod: "mod",
        Move: "move",
        Mut: "mut",
        Pub: "pub",
        Ref: "ref",
        Return: "return",
        SelfLower: "self",
        SelfUpper: "Self",
        Static: "static",
        Struct: "struct",
        Super: "super",
        Trait: "trait",
        True: "true",
        Type: "type",
        Unsafe: "unsafe",
        Use: "use",
        Where: "where",
        While: "while",
        // Reserved keywords.
        Abstract: "abstract",
        Become: "become",
        Box: "box",
        Do: "do",
        Final: "final",
        Gen: "gen",
        Macro: "macro",
        Override: "override",
        Priv: "priv",
        Try: "try",
        Typeof: "typeof",
        Unsized: "unsized",
        Virtual: "virtual",
        Yield: "yield",
        // Contextual keywords.
        Auto: "auto",
        Default: "default",
        MacroRules: "macro_rules",
        Raw: "raw",
        Safe: "safe",
        Union: "union",
        StaticLifetime: "'static",
        UnderscoreLifetime: "'_",
    }
    symbols {
        // Primitive types.
        bool,
        char,
        str,
        u8,
        u16,
        u32,
        u64,
        u128,
        usize,
        i8,
        i16,
        i32,
        i64,
        i128,
        isize,
        f32,
        f64,
        // Crates and the prelude.
        core,
        alloc,
        std,
        Option,
        Some,
        None,
        Result,
        Ok,
        Err,
        // Lang items.
        Copy,
        Clone,
        Sized,
        Send,
        Sync,
        Drop,
        Fn,
        FnMut,
        FnOnce,
        Add,
        Sub,
        Mul,
        Div,
        Rem,
        Neg,
        Not,
        BitAnd,
        BitOr,
        BitXor,
        Shl,
        Shr,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Iterator,
        IntoIterator,
        // Attributes and well-known names.
        main,
        derive,
        cfg,
        test,
        inline,
        lang,
        allow,
        doc,
        repr,
        C,
        no_mangle,
        no_std,
    }
}
