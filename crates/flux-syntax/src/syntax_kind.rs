//! Every kind of token and node in a flux syntax tree.
//!
//! The lexer produces only single-character punctuation; the parser glues
//! adjacent ("joint") punctuation into composite tokens such as `::` or `>>`.
//! Keeping the lexer context-free means `Vec<Vec<T>>` needs no special case:
//! the `>>` is two `>` tokens until an expression asks for a shift.

/// The kind of a token or node. `#[repr(u16)]` keeps token and node arrays
/// small.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum SyntaxKind {
    // Special.
    /// A placeholder for a node that was started and then abandoned.
    TOMBSTONE = 0,
    EOF,

    // Single-character punctuation.
    SEMICOLON,
    COMMA,
    L_PAREN,
    R_PAREN,
    L_CURLY,
    R_CURLY,
    L_BRACK,
    R_BRACK,
    L_ANGLE,
    R_ANGLE,
    AT,
    POUND,
    TILDE,
    QUESTION,
    DOLLAR,
    AMP,
    PIPE,
    PLUS,
    STAR,
    SLASH,
    CARET,
    PERCENT,
    UNDERSCORE,
    DOT,
    COLON,
    EQ,
    BANG,
    MINUS,

    // Composite punctuation, glued by the parser from joint single tokens.
    DOT2,
    DOT3,
    DOT2EQ,
    COLON2,
    EQ2,
    FAT_ARROW,
    NEQ,
    THIN_ARROW,
    LTEQ,
    GTEQ,
    AMP2,
    PIPE2,
    SHL,
    SHR,
    PLUSEQ,
    MINUSEQ,
    STAREQ,
    SLASHEQ,
    PERCENTEQ,
    CARETEQ,
    AMPEQ,
    PIPEEQ,
    SHLEQ,
    SHREQ,

    // Strict keywords.
    AS_KW,
    ASYNC_KW,
    AWAIT_KW,
    BREAK_KW,
    CONST_KW,
    CONTINUE_KW,
    CRATE_KW,
    DYN_KW,
    ELSE_KW,
    ENUM_KW,
    EXTERN_KW,
    FALSE_KW,
    FN_KW,
    FOR_KW,
    IF_KW,
    IMPL_KW,
    IN_KW,
    LET_KW,
    LOOP_KW,
    MATCH_KW,
    MOD_KW,
    MOVE_KW,
    MUT_KW,
    PUB_KW,
    REF_KW,
    RETURN_KW,
    SELF_KW,
    SELF_TYPE_KW,
    STATIC_KW,
    STRUCT_KW,
    SUPER_KW,
    TRAIT_KW,
    TRUE_KW,
    TYPE_KW,
    UNSAFE_KW,
    USE_KW,
    WHERE_KW,
    WHILE_KW,
    // Reserved keywords (edition 2024): never valid, but lexed so the
    // parser can reject them instead of treating them as names.
    ABSTRACT_KW,
    BECOME_KW,
    BOX_KW,
    DO_KW,
    FINAL_KW,
    GEN_KW,
    MACRO_KW,
    OVERRIDE_KW,
    PRIV_KW,
    TRY_KW,
    TYPEOF_KW,
    UNSIZED_KW,
    VIRTUAL_KW,
    YIELD_KW,

    // Literals and names.
    INT_NUMBER,
    FLOAT_NUMBER,
    CHAR,
    BYTE,
    STRING,
    BYTE_STRING,
    C_STRING,
    IDENT,
    LIFETIME_IDENT,

    // Trivia and errors.
    WHITESPACE,
    COMMENT,
    /// An unknown character as a token, or a span the parser could not
    /// understand as a node.
    ERROR,

    // Nodes: file, attributes and items.
    SOURCE_FILE,
    ATTR,
    META,
    TOKEN_TREE,
    /// A macro call in item position; see also `MACRO_EXPR`, `MACRO_TYPE`
    /// and `MACRO_PAT`.
    MACRO_CALL,
    VISIBILITY,
    NAME,
    NAME_REF,
    LIFETIME,
    FN,
    STRUCT,
    ENUM,
    IMPL,
    TRAIT,
    TYPE_ALIAS,
    CONST,
    STATIC,
    MODULE,
    USE,
    EXTERN_CRATE,
    EXTERN_BLOCK,
    EXTERN_ITEM_LIST,
    ABI,
    ITEM_LIST,
    ASSOC_ITEM_LIST,
    USE_TREE,
    USE_TREE_LIST,
    RENAME,
    PARAM_LIST,
    PARAM,
    SELF_PARAM,
    RET_TYPE,
    RECORD_FIELD_LIST,
    RECORD_FIELD,
    TUPLE_FIELD_LIST,
    TUPLE_FIELD,
    VARIANT_LIST,
    VARIANT,

    // Generics.
    GENERIC_PARAM_LIST,
    TYPE_PARAM,
    LIFETIME_PARAM,
    CONST_PARAM,
    WHERE_CLAUSE,
    WHERE_PRED,
    TYPE_BOUND_LIST,
    TYPE_BOUND,
    USE_BOUND_GENERIC_ARGS,

    // Paths.
    PATH,
    PATH_SEGMENT,
    GENERIC_ARG_LIST,
    TYPE_ARG,
    LIFETIME_ARG,
    CONST_ARG,
    ASSOC_TYPE_ARG,
    PARENTHESIZED_ARG_LIST,

    // Types.
    PATH_TYPE,
    REF_TYPE,
    PTR_TYPE,
    ARRAY_TYPE,
    SLICE_TYPE,
    TUPLE_TYPE,
    PAREN_TYPE,
    NEVER_TYPE,
    INFER_TYPE,
    FN_PTR_TYPE,
    FOR_TYPE,
    IMPL_TRAIT_TYPE,
    DYN_TRAIT_TYPE,
    MACRO_TYPE,

    // Patterns.
    IDENT_PAT,
    WILDCARD_PAT,
    LITERAL_PAT,
    RANGE_PAT,
    PATH_PAT,
    TUPLE_STRUCT_PAT,
    RECORD_PAT,
    RECORD_PAT_FIELD_LIST,
    RECORD_PAT_FIELD,
    TUPLE_PAT,
    SLICE_PAT,
    REF_PAT,
    OR_PAT,
    REST_PAT,
    PAREN_PAT,
    MACRO_PAT,

    // Expressions.
    LITERAL,
    PATH_EXPR,
    TUPLE_EXPR,
    PAREN_EXPR,
    ARRAY_EXPR,
    BLOCK_EXPR,
    LABEL,
    LOOP_EXPR,
    WHILE_EXPR,
    FOR_EXPR,
    IF_EXPR,
    LET_EXPR,
    MATCH_EXPR,
    MATCH_ARM_LIST,
    MATCH_ARM,
    MATCH_GUARD,
    BREAK_EXPR,
    CONTINUE_EXPR,
    RETURN_EXPR,
    CLOSURE_EXPR,
    PREFIX_EXPR,
    REF_EXPR,
    BIN_EXPR,
    CAST_EXPR,
    RANGE_EXPR,
    CALL_EXPR,
    ARG_LIST,
    METHOD_CALL_EXPR,
    FIELD_EXPR,
    INDEX_EXPR,
    TRY_EXPR,
    AWAIT_EXPR,
    RECORD_EXPR,
    RECORD_EXPR_FIELD_LIST,
    RECORD_EXPR_FIELD,
    UNDERSCORE_EXPR,
    MACRO_EXPR,

    // Statements.
    LET_STMT,
    LET_ELSE,
    EXPR_STMT,

    // Roots of macro expansions.
    /// Items a macro call in item position expanded to.
    MACRO_ITEMS,
    /// Statements a macro call in statement position expanded to.
    MACRO_STMTS,
    /// An expression, pattern or type a macro call expanded to.
    MACRO_FRAGMENT,
}

use SyntaxKind::*;

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, WHITESPACE | COMMENT)
    }

    pub fn is_keyword(self) -> bool {
        (AS_KW as u16..=YIELD_KW as u16).contains(&(self as u16))
    }

    pub fn is_reserved_keyword(self) -> bool {
        (ABSTRACT_KW as u16..=YIELD_KW as u16).contains(&(self as u16))
    }

    pub fn is_punct(self) -> bool {
        (SEMICOLON as u16..=SHREQ as u16).contains(&(self as u16))
    }

    pub fn is_literal(self) -> bool {
        matches!(
            self,
            INT_NUMBER | FLOAT_NUMBER | CHAR | BYTE | STRING | BYTE_STRING | C_STRING
        )
    }

    /// The keyword spelled `ident`, in edition 2024.
    pub fn from_keyword(ident: &str) -> Option<SyntaxKind> {
        // Every keyword fits in a `u64`: look its bytes up in a perfect
        // hash table rather than comparing with each keyword in turn.
        // A trailing NUL would read as padding.
        let bytes = ident.as_bytes();
        if bytes.len() > 8 || bytes.last().is_none_or(|&b| b == 0) {
            return None;
        }
        let word = pack_word(bytes);
        let (key, kind) = KEYWORD_TABLE[keyword_slot(word)];
        (key == word).then_some(kind)
    }

    /// The single-character punctuation token for `c`.
    pub fn from_char(c: char) -> Option<SyntaxKind> {
        let kind = match c {
            ';' => SEMICOLON,
            ',' => COMMA,
            '(' => L_PAREN,
            ')' => R_PAREN,
            '{' => L_CURLY,
            '}' => R_CURLY,
            '[' => L_BRACK,
            ']' => R_BRACK,
            '<' => L_ANGLE,
            '>' => R_ANGLE,
            '@' => AT,
            '#' => POUND,
            '~' => TILDE,
            '?' => QUESTION,
            '$' => DOLLAR,
            '&' => AMP,
            '|' => PIPE,
            '+' => PLUS,
            '*' => STAR,
            '/' => SLASH,
            '^' => CARET,
            '%' => PERCENT,
            '_' => UNDERSCORE,
            '.' => DOT,
            ':' => COLON,
            '=' => EQ,
            '!' => BANG,
            '-' => MINUS,
            _ => return None,
        };
        Some(kind)
    }

    /// The single-character tokens a composite punctuation token is glued
    /// from, or `None` for a token that is not composite.
    #[inline]
    pub fn composite_parts(self) -> Option<&'static [SyntaxKind]> {
        let parts: &'static [SyntaxKind] = match self {
            DOT2 => &[DOT, DOT],
            DOT3 => &[DOT, DOT, DOT],
            DOT2EQ => &[DOT, DOT, EQ],
            COLON2 => &[COLON, COLON],
            EQ2 => &[EQ, EQ],
            FAT_ARROW => &[EQ, R_ANGLE],
            NEQ => &[BANG, EQ],
            THIN_ARROW => &[MINUS, R_ANGLE],
            LTEQ => &[L_ANGLE, EQ],
            GTEQ => &[R_ANGLE, EQ],
            AMP2 => &[AMP, AMP],
            PIPE2 => &[PIPE, PIPE],
            SHL => &[L_ANGLE, L_ANGLE],
            SHR => &[R_ANGLE, R_ANGLE],
            PLUSEQ => &[PLUS, EQ],
            MINUSEQ => &[MINUS, EQ],
            STAREQ => &[STAR, EQ],
            SLASHEQ => &[SLASH, EQ],
            PERCENTEQ => &[PERCENT, EQ],
            CARETEQ => &[CARET, EQ],
            AMPEQ => &[AMP, EQ],
            PIPEEQ => &[PIPE, EQ],
            SHLEQ => &[L_ANGLE, L_ANGLE, EQ],
            SHREQ => &[R_ANGLE, R_ANGLE, EQ],
            _ => return None,
        };
        Some(parts)
    }

    /// Items and item-like nodes that leading comments attach to, so that a
    /// doc comment belongs to the item it documents.
    pub(crate) fn takes_leading_comments(self) -> bool {
        matches!(
            self,
            FN | STRUCT
                | ENUM
                | IMPL
                | TRAIT
                | TYPE_ALIAS
                | CONST
                | STATIC
                | MODULE
                | USE
                | MACRO_CALL
                | RECORD_FIELD
                | TUPLE_FIELD
                | VARIANT
        )
    }
}

/// Every keyword of edition 2024, strict and reserved.
const KEYWORDS: [(&str, SyntaxKind); 52] = [
    ("as", AS_KW),
    ("async", ASYNC_KW),
    ("await", AWAIT_KW),
    ("break", BREAK_KW),
    ("const", CONST_KW),
    ("continue", CONTINUE_KW),
    ("crate", CRATE_KW),
    ("dyn", DYN_KW),
    ("else", ELSE_KW),
    ("enum", ENUM_KW),
    ("extern", EXTERN_KW),
    ("false", FALSE_KW),
    ("fn", FN_KW),
    ("for", FOR_KW),
    ("if", IF_KW),
    ("impl", IMPL_KW),
    ("in", IN_KW),
    ("let", LET_KW),
    ("loop", LOOP_KW),
    ("match", MATCH_KW),
    ("mod", MOD_KW),
    ("move", MOVE_KW),
    ("mut", MUT_KW),
    ("pub", PUB_KW),
    ("ref", REF_KW),
    ("return", RETURN_KW),
    ("self", SELF_KW),
    ("Self", SELF_TYPE_KW),
    ("static", STATIC_KW),
    ("struct", STRUCT_KW),
    ("super", SUPER_KW),
    ("trait", TRAIT_KW),
    ("true", TRUE_KW),
    ("type", TYPE_KW),
    ("unsafe", UNSAFE_KW),
    ("use", USE_KW),
    ("where", WHERE_KW),
    ("while", WHILE_KW),
    ("abstract", ABSTRACT_KW),
    ("become", BECOME_KW),
    ("box", BOX_KW),
    ("do", DO_KW),
    ("final", FINAL_KW),
    ("gen", GEN_KW),
    ("macro", MACRO_KW),
    ("override", OVERRIDE_KW),
    ("priv", PRIV_KW),
    ("try", TRY_KW),
    ("typeof", TYPEOF_KW),
    ("unsized", UNSIZED_KW),
    ("virtual", VIRTUAL_KW),
    ("yield", YIELD_KW),
];

/// Multiplier of the keyword hash, chosen so that no two keywords share
/// a slot: building [`KEYWORD_TABLE`] fails to compile if they do.
const KEYWORD_HASH: u64 = 0x203d_5d64_077c_f60d;

/// The slot of a keyword's bytes, as [`pack_word`] gives them.
const fn keyword_slot(word: u64) -> usize {
    (word.wrapping_mul(KEYWORD_HASH) >> 57) as usize
}

/// At most 8 bytes as a little-endian `u64`, zero-padded. Reads two
/// overlapping halves, which agree where they overlap, rather than byte
/// by byte or through `memcpy`.
#[inline]
const fn pack_word(b: &[u8]) -> u64 {
    let n = b.len();
    debug_assert!(n <= 8);
    match n {
        0 => 0,
        1 => b[0] as u64,
        2 | 3 => {
            let lo = u16::from_le_bytes([b[0], b[1]]) as u64;
            let hi = u16::from_le_bytes([b[n - 2], b[n - 1]]) as u64;
            lo | hi << (8 * (n - 2))
        }
        _ => {
            let lo = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64;
            let hi = u32::from_le_bytes([b[n - 4], b[n - 3], b[n - 2], b[n - 1]]) as u64;
            lo | hi << (8 * (n - 4))
        }
    }
}

/// Each keyword's bytes and kind, in its slot; other slots hold a key
/// no identifier has.
const KEYWORD_TABLE: [(u64, SyntaxKind); 128] = {
    let mut table = [(0, IDENT); 128];
    let mut i = 0;
    while i < KEYWORDS.len() {
        let (text, kind) = KEYWORDS[i];
        let bytes = text.as_bytes();
        assert!(bytes.len() <= 8, "keywords fit in a u64");
        let word = pack_word(bytes);
        let slot = keyword_slot(word);
        assert!(
            table[slot].0 == 0,
            "two keywords share a slot: change KEYWORD_HASH"
        );
        table[slot] = (word, kind);
        i += 1;
    }
    table
};

/// The token kind for punctuation or a keyword, by its spelling:
/// `T![;]`, `T![::]`, `T![fn]`.
#[macro_export]
macro_rules! T {
    [;] => { $crate::SyntaxKind::SEMICOLON };
    [,] => { $crate::SyntaxKind::COMMA };
    ['('] => { $crate::SyntaxKind::L_PAREN };
    [')'] => { $crate::SyntaxKind::R_PAREN };
    ['{'] => { $crate::SyntaxKind::L_CURLY };
    ['}'] => { $crate::SyntaxKind::R_CURLY };
    ['['] => { $crate::SyntaxKind::L_BRACK };
    [']'] => { $crate::SyntaxKind::R_BRACK };
    [<] => { $crate::SyntaxKind::L_ANGLE };
    [>] => { $crate::SyntaxKind::R_ANGLE };
    [@] => { $crate::SyntaxKind::AT };
    [#] => { $crate::SyntaxKind::POUND };
    [~] => { $crate::SyntaxKind::TILDE };
    [?] => { $crate::SyntaxKind::QUESTION };
    [$] => { $crate::SyntaxKind::DOLLAR };
    [&] => { $crate::SyntaxKind::AMP };
    [|] => { $crate::SyntaxKind::PIPE };
    [+] => { $crate::SyntaxKind::PLUS };
    [*] => { $crate::SyntaxKind::STAR };
    [/] => { $crate::SyntaxKind::SLASH };
    [^] => { $crate::SyntaxKind::CARET };
    [%] => { $crate::SyntaxKind::PERCENT };
    [_] => { $crate::SyntaxKind::UNDERSCORE };
    [.] => { $crate::SyntaxKind::DOT };
    [:] => { $crate::SyntaxKind::COLON };
    [=] => { $crate::SyntaxKind::EQ };
    [!] => { $crate::SyntaxKind::BANG };
    [-] => { $crate::SyntaxKind::MINUS };
    [..] => { $crate::SyntaxKind::DOT2 };
    [...] => { $crate::SyntaxKind::DOT3 };
    [..=] => { $crate::SyntaxKind::DOT2EQ };
    [::] => { $crate::SyntaxKind::COLON2 };
    [==] => { $crate::SyntaxKind::EQ2 };
    [=>] => { $crate::SyntaxKind::FAT_ARROW };
    [!=] => { $crate::SyntaxKind::NEQ };
    [->] => { $crate::SyntaxKind::THIN_ARROW };
    [<=] => { $crate::SyntaxKind::LTEQ };
    [>=] => { $crate::SyntaxKind::GTEQ };
    [&&] => { $crate::SyntaxKind::AMP2 };
    [||] => { $crate::SyntaxKind::PIPE2 };
    [<<] => { $crate::SyntaxKind::SHL };
    [>>] => { $crate::SyntaxKind::SHR };
    [+=] => { $crate::SyntaxKind::PLUSEQ };
    [-=] => { $crate::SyntaxKind::MINUSEQ };
    [*=] => { $crate::SyntaxKind::STAREQ };
    [/=] => { $crate::SyntaxKind::SLASHEQ };
    [%=] => { $crate::SyntaxKind::PERCENTEQ };
    [^=] => { $crate::SyntaxKind::CARETEQ };
    [&=] => { $crate::SyntaxKind::AMPEQ };
    [|=] => { $crate::SyntaxKind::PIPEEQ };
    [<<=] => { $crate::SyntaxKind::SHLEQ };
    [>>=] => { $crate::SyntaxKind::SHREQ };
    [as] => { $crate::SyntaxKind::AS_KW };
    [async] => { $crate::SyntaxKind::ASYNC_KW };
    [await] => { $crate::SyntaxKind::AWAIT_KW };
    [break] => { $crate::SyntaxKind::BREAK_KW };
    [const] => { $crate::SyntaxKind::CONST_KW };
    [continue] => { $crate::SyntaxKind::CONTINUE_KW };
    [crate] => { $crate::SyntaxKind::CRATE_KW };
    [dyn] => { $crate::SyntaxKind::DYN_KW };
    [else] => { $crate::SyntaxKind::ELSE_KW };
    [enum] => { $crate::SyntaxKind::ENUM_KW };
    [extern] => { $crate::SyntaxKind::EXTERN_KW };
    [false] => { $crate::SyntaxKind::FALSE_KW };
    [fn] => { $crate::SyntaxKind::FN_KW };
    [for] => { $crate::SyntaxKind::FOR_KW };
    [if] => { $crate::SyntaxKind::IF_KW };
    [impl] => { $crate::SyntaxKind::IMPL_KW };
    [in] => { $crate::SyntaxKind::IN_KW };
    [let] => { $crate::SyntaxKind::LET_KW };
    [loop] => { $crate::SyntaxKind::LOOP_KW };
    [match] => { $crate::SyntaxKind::MATCH_KW };
    [mod] => { $crate::SyntaxKind::MOD_KW };
    [move] => { $crate::SyntaxKind::MOVE_KW };
    [mut] => { $crate::SyntaxKind::MUT_KW };
    [pub] => { $crate::SyntaxKind::PUB_KW };
    [ref] => { $crate::SyntaxKind::REF_KW };
    [return] => { $crate::SyntaxKind::RETURN_KW };
    [self] => { $crate::SyntaxKind::SELF_KW };
    [Self] => { $crate::SyntaxKind::SELF_TYPE_KW };
    [static] => { $crate::SyntaxKind::STATIC_KW };
    [struct] => { $crate::SyntaxKind::STRUCT_KW };
    [super] => { $crate::SyntaxKind::SUPER_KW };
    [trait] => { $crate::SyntaxKind::TRAIT_KW };
    [true] => { $crate::SyntaxKind::TRUE_KW };
    [type] => { $crate::SyntaxKind::TYPE_KW };
    [unsafe] => { $crate::SyntaxKind::UNSAFE_KW };
    [use] => { $crate::SyntaxKind::USE_KW };
    [where] => { $crate::SyntaxKind::WHERE_KW };
    [while] => { $crate::SyntaxKind::WHILE_KW };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords() {
        for (text, kind) in KEYWORDS {
            assert_eq!(SyntaxKind::from_keyword(text), Some(kind), "{text}");
            assert!(kind.is_keyword());
        }
        let count = (AS_KW as u16..=YIELD_KW as u16).count();
        assert_eq!(KEYWORDS.len(), count, "every keyword kind has a spelling");
        for text in [
            "",
            "a",
            "As",
            "self_",
            "continues",
            "fn\0",
            "\0",
            "matches",
            "_",
        ] {
            assert_eq!(SyntaxKind::from_keyword(text), None, "{text:?}");
        }
    }

    #[test]
    fn pack_word_is_little_endian() {
        let bytes = b"abcdefgh";
        for n in 0..=8 {
            let mut word = [0; 8];
            word[..n].copy_from_slice(&bytes[..n]);
            assert_eq!(pack_word(&bytes[..n]), u64::from_le_bytes(word), "{n}");
        }
    }
}
