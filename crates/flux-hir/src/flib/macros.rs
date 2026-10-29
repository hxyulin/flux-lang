//! Declarative macro tokens and hygiene. Matchers are rebuilt from saved tokens.
use super::{Decode, Decoder, Encode, Encoder, Error};
use crate::macros::tt::Tok;
use crate::macros::{MacroDef, MacroKind, MacroScope};
use flux_syntax::{SyntaxKind, TokenOrigin};
use std::sync::Arc;

impl Encode for TokenOrigin {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.range.encode(e)?;
        let group = if self.group == 0 {
            0
        } else {
            let next = e.groups.len() as u32 + 1;
            *e.groups.entry(self.group).or_insert(next)
        };
        group.encode(e)?;
        if let Some(krate) = crate::macros::definition_crate(self.ctxt) {
            2u8.encode(e)?;
            krate.encode(e)
        } else if self.ctxt == 0 {
            0u8.encode(e)
        } else {
            1u8.encode(e)?;
            let next = e.contexts.len() as u32 + 1;
            let ctxt = *e.contexts.entry(self.ctxt).or_insert(next);
            ctxt.encode(e)
        }
    }
}
impl Decode for TokenOrigin {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let range = Decode::decode(d)?;
        let wire = u32::decode(d)?;
        let group = if wire == 0 {
            0
        } else {
            *d.groups
                .entry(wire)
                .or_insert_with(crate::macros::fresh_group)
        };
        let ctxt = match u8::decode(d)? {
            0 => 0,
            1 => {
                let wire = u32::decode(d)?;
                *d.contexts
                    .entry(wire)
                    .or_insert_with(crate::macros::fresh_ctxt)
            }
            2 => {
                let krate = crate::CrateId::decode(d)?;
                if krate.0 >= 1 << 31 {
                    return Err(Error("invalid macro defining crate"));
                }
                krate.0 | (1 << 31)
            }
            _ => return Err(Error("invalid macro context tag")),
        };
        Ok(Self { range, group, ctxt })
    }
}
impl Encode for Tok {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.edition.encode(e)?;
        self.kind.encode(e)?;
        self.text.encode(e)?;
        self.joint.encode(e)?;
        self.origin.encode(e)
    }
}
impl Decode for Tok {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            edition: Decode::decode(d)?,
            kind: Decode::decode(d)?,
            text: Decode::decode(d)?,
            joint: Decode::decode(d)?,
            origin: Decode::decode(d)?,
        })
    }
}
fn validate_rule_tokens(tokens: &[Tok]) -> Result<(), Error> {
    let mut delimiters = Vec::new();
    for token in tokens {
        let close = match token.kind {
            SyntaxKind::L_PAREN => Some(SyntaxKind::R_PAREN),
            SyntaxKind::L_BRACK => Some(SyntaxKind::R_BRACK),
            SyntaxKind::L_CURLY => Some(SyntaxKind::R_CURLY),
            _ => None,
        };
        if let Some(close) = close {
            if delimiters.len() >= super::MAX_NESTING_DEPTH as usize {
                return Err(Error("macro token nesting limit exceeded"));
            }
            delimiters.push(close);
        } else if crate::macros::tt::is_close(token.kind) && delimiters.pop() != Some(token.kind) {
            return Err(Error("invalid macro token delimiters"));
        }
    }
    if !delimiters.is_empty() {
        return Err(Error("unclosed macro token delimiters"));
    }
    Ok(())
}

impl Encode for MacroDef {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.name.encode(e)?;
        self.krate.encode(e)?;
        self.file.encode(e)?;
        self.name_range.encode(e)?;
        match &self.kind {
            MacroKind::Rules(rules) => {
                validate_rule_tokens(&rules.toks)?;
                0u8.encode(e)?;
                rules.edition.encode(e)?;
                rules.local_inner_macros.encode(e)?;
                rules.toks.encode(e)
            }
            _ => Err(Error("macro export kind is not supported in flib yet")),
        }
    }
}
impl Decode for MacroDef {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let name = Decode::decode(d)?;
        let krate = Decode::decode(d)?;
        let file = Decode::decode(d)?;
        let name_range = Decode::decode(d)?;
        if u8::decode(d)? != 0 {
            return Err(Error("unsupported macro export kind"));
        }
        let edition = flux_syntax::Edition::decode(d)?;
        let local_inner_macros = bool::decode(d)?;
        let toks = Vec::<Tok>::decode(d)?;
        validate_rule_tokens(&toks)?;
        let (mut rules, errors) = crate::macros::mbe::parse_rules(toks, name_range, edition);
        if !errors.is_empty() || rules.invalid {
            return Err(Error("invalid exported macro tokens"));
        }
        rules.local_inner_macros = local_inner_macros;
        Ok(Self {
            name,
            krate,
            file,
            name_range,
            group: crate::macros::fresh_group(),
            kind: MacroKind::Rules(Arc::new(rules)),
        })
    }
}
impl Encode for MacroScope {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.parent.encode(e)?;
        self.name.encode(e)?;
        self.def.encode(e)?;
        self.depth.encode(e)?;
        self.jump.encode(e)
    }
}
impl Decode for MacroScope {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            parent: Decode::decode(d)?,
            name: Decode::decode(d)?,
            def: Decode::decode(d)?,
            depth: Decode::decode(d)?,
            jump: Decode::decode(d)?,
        })
    }
}

// Explicit wire tags, independent of the Rust enum representation.
impl Encode for SyntaxKind {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        let tag: u16 = match self {
            Self::TOMBSTONE => 0,
            Self::EOF => 1,
            Self::SEMICOLON => 2,
            Self::COMMA => 3,
            Self::L_PAREN => 4,
            Self::R_PAREN => 5,
            Self::L_CURLY => 6,
            Self::R_CURLY => 7,
            Self::L_BRACK => 8,
            Self::R_BRACK => 9,
            Self::L_ANGLE => 10,
            Self::R_ANGLE => 11,
            Self::AT => 12,
            Self::POUND => 13,
            Self::TILDE => 14,
            Self::QUESTION => 15,
            Self::DOLLAR => 16,
            Self::AMP => 17,
            Self::PIPE => 18,
            Self::PLUS => 19,
            Self::STAR => 20,
            Self::SLASH => 21,
            Self::CARET => 22,
            Self::PERCENT => 23,
            Self::UNDERSCORE => 24,
            Self::DOT => 25,
            Self::COLON => 26,
            Self::EQ => 27,
            Self::BANG => 28,
            Self::MINUS => 29,
            Self::DOT2 => 30,
            Self::DOT3 => 31,
            Self::DOT2EQ => 32,
            Self::COLON2 => 33,
            Self::EQ2 => 34,
            Self::FAT_ARROW => 35,
            Self::NEQ => 36,
            Self::THIN_ARROW => 37,
            Self::LTEQ => 38,
            Self::GTEQ => 39,
            Self::AMP2 => 40,
            Self::PIPE2 => 41,
            Self::SHL => 42,
            Self::SHR => 43,
            Self::PLUSEQ => 44,
            Self::MINUSEQ => 45,
            Self::STAREQ => 46,
            Self::SLASHEQ => 47,
            Self::PERCENTEQ => 48,
            Self::CARETEQ => 49,
            Self::AMPEQ => 50,
            Self::PIPEEQ => 51,
            Self::SHLEQ => 52,
            Self::SHREQ => 53,
            Self::AS_KW => 54,
            Self::ASYNC_KW => 55,
            Self::AWAIT_KW => 56,
            Self::BREAK_KW => 57,
            Self::CONST_KW => 58,
            Self::CONTINUE_KW => 59,
            Self::CRATE_KW => 60,
            Self::DYN_KW => 61,
            Self::ELSE_KW => 62,
            Self::ENUM_KW => 63,
            Self::EXTERN_KW => 64,
            Self::FALSE_KW => 65,
            Self::FN_KW => 66,
            Self::FOR_KW => 67,
            Self::IF_KW => 68,
            Self::IMPL_KW => 69,
            Self::IN_KW => 70,
            Self::LET_KW => 71,
            Self::LOOP_KW => 72,
            Self::MATCH_KW => 73,
            Self::MOD_KW => 74,
            Self::MOVE_KW => 75,
            Self::MUT_KW => 76,
            Self::PUB_KW => 77,
            Self::REF_KW => 78,
            Self::RETURN_KW => 79,
            Self::SELF_KW => 80,
            Self::SELF_TYPE_KW => 81,
            Self::STATIC_KW => 82,
            Self::STRUCT_KW => 83,
            Self::SUPER_KW => 84,
            Self::TRAIT_KW => 85,
            Self::TRUE_KW => 86,
            Self::TYPE_KW => 87,
            Self::UNSAFE_KW => 88,
            Self::USE_KW => 89,
            Self::WHERE_KW => 90,
            Self::WHILE_KW => 91,
            Self::ABSTRACT_KW => 92,
            Self::BECOME_KW => 93,
            Self::BOX_KW => 94,
            Self::DO_KW => 95,
            Self::FINAL_KW => 96,
            Self::GEN_KW => 97,
            Self::MACRO_KW => 98,
            Self::OVERRIDE_KW => 99,
            Self::PRIV_KW => 100,
            Self::TRY_KW => 101,
            Self::TYPEOF_KW => 102,
            Self::UNSIZED_KW => 103,
            Self::VIRTUAL_KW => 104,
            Self::YIELD_KW => 105,
            Self::UNION_KW => 106,
            Self::INT_NUMBER => 107,
            Self::FLOAT_NUMBER => 108,
            Self::CHAR => 109,
            Self::BYTE => 110,
            Self::STRING => 111,
            Self::BYTE_STRING => 112,
            Self::C_STRING => 113,
            Self::IDENT => 114,
            Self::LIFETIME_IDENT => 115,
            Self::WHITESPACE => 116,
            Self::COMMENT => 117,
            Self::ERROR => 118,
            Self::SOURCE_FILE => 119,
            Self::ATTR => 120,
            Self::META => 121,
            Self::TOKEN_TREE => 122,
            Self::MACRO_CALL => 123,
            Self::VISIBILITY => 124,
            Self::NAME => 125,
            Self::NAME_REF => 126,
            Self::LIFETIME => 127,
            Self::FN => 128,
            Self::STRUCT => 129,
            Self::UNION => 130,
            Self::ENUM => 131,
            Self::IMPL => 132,
            Self::TRAIT => 133,
            Self::TYPE_ALIAS => 134,
            Self::CONST => 135,
            Self::STATIC => 136,
            Self::MODULE => 137,
            Self::USE => 138,
            Self::EXTERN_CRATE => 139,
            Self::EXTERN_BLOCK => 140,
            Self::EXTERN_ITEM_LIST => 141,
            Self::ABI => 142,
            Self::ITEM_LIST => 143,
            Self::ASSOC_ITEM_LIST => 144,
            Self::USE_TREE => 145,
            Self::USE_TREE_LIST => 146,
            Self::RENAME => 147,
            Self::PARAM_LIST => 148,
            Self::PARAM => 149,
            Self::SELF_PARAM => 150,
            Self::RET_TYPE => 151,
            Self::RECORD_FIELD_LIST => 152,
            Self::RECORD_FIELD => 153,
            Self::TUPLE_FIELD_LIST => 154,
            Self::TUPLE_FIELD => 155,
            Self::VARIANT_LIST => 156,
            Self::VARIANT => 157,
            Self::GENERIC_PARAM_LIST => 158,
            Self::TYPE_PARAM => 159,
            Self::LIFETIME_PARAM => 160,
            Self::CONST_PARAM => 161,
            Self::WHERE_CLAUSE => 162,
            Self::WHERE_PRED => 163,
            Self::TYPE_BOUND_LIST => 164,
            Self::TYPE_BOUND => 165,
            Self::USE_BOUND_GENERIC_ARGS => 166,
            Self::PATH => 167,
            Self::PATH_SEGMENT => 168,
            Self::GENERIC_ARG_LIST => 169,
            Self::TYPE_ARG => 170,
            Self::LIFETIME_ARG => 171,
            Self::CONST_ARG => 172,
            Self::ASSOC_TYPE_ARG => 173,
            Self::PARENTHESIZED_ARG_LIST => 174,
            Self::PATH_TYPE => 175,
            Self::REF_TYPE => 176,
            Self::PTR_TYPE => 177,
            Self::ARRAY_TYPE => 178,
            Self::SLICE_TYPE => 179,
            Self::TUPLE_TYPE => 180,
            Self::PAREN_TYPE => 181,
            Self::NEVER_TYPE => 182,
            Self::INFER_TYPE => 183,
            Self::FN_PTR_TYPE => 184,
            Self::FOR_TYPE => 185,
            Self::IMPL_TRAIT_TYPE => 186,
            Self::DYN_TRAIT_TYPE => 187,
            Self::MACRO_TYPE => 188,
            Self::IDENT_PAT => 189,
            Self::WILDCARD_PAT => 190,
            Self::LITERAL_PAT => 191,
            Self::RANGE_PAT => 192,
            Self::PATH_PAT => 193,
            Self::TUPLE_STRUCT_PAT => 194,
            Self::RECORD_PAT => 195,
            Self::RECORD_PAT_FIELD_LIST => 196,
            Self::RECORD_PAT_FIELD => 197,
            Self::TUPLE_PAT => 198,
            Self::SLICE_PAT => 199,
            Self::REF_PAT => 200,
            Self::OR_PAT => 201,
            Self::REST_PAT => 202,
            Self::PAREN_PAT => 203,
            Self::MACRO_PAT => 204,
            Self::LITERAL => 205,
            Self::PATH_EXPR => 206,
            Self::TUPLE_EXPR => 207,
            Self::PAREN_EXPR => 208,
            Self::ARRAY_EXPR => 209,
            Self::BLOCK_EXPR => 210,
            Self::LABEL => 211,
            Self::LOOP_EXPR => 212,
            Self::WHILE_EXPR => 213,
            Self::FOR_EXPR => 214,
            Self::IF_EXPR => 215,
            Self::LET_EXPR => 216,
            Self::MATCH_EXPR => 217,
            Self::MATCH_ARM_LIST => 218,
            Self::MATCH_ARM => 219,
            Self::MATCH_GUARD => 220,
            Self::BREAK_EXPR => 221,
            Self::CONTINUE_EXPR => 222,
            Self::RETURN_EXPR => 223,
            Self::CLOSURE_EXPR => 224,
            Self::PREFIX_EXPR => 225,
            Self::REF_EXPR => 226,
            Self::BIN_EXPR => 227,
            Self::CAST_EXPR => 228,
            Self::RANGE_EXPR => 229,
            Self::CALL_EXPR => 230,
            Self::ARG_LIST => 231,
            Self::METHOD_CALL_EXPR => 232,
            Self::FIELD_EXPR => 233,
            Self::INDEX_EXPR => 234,
            Self::TRY_EXPR => 235,
            Self::AWAIT_EXPR => 236,
            Self::RECORD_EXPR => 237,
            Self::RECORD_EXPR_FIELD_LIST => 238,
            Self::RECORD_EXPR_FIELD => 239,
            Self::UNDERSCORE_EXPR => 240,
            Self::MACRO_EXPR => 241,
            Self::LET_STMT => 242,
            Self::LET_ELSE => 243,
            Self::EXPR_STMT => 244,
            Self::MACRO_ITEMS => 245,
            Self::MACRO_STMTS => 246,
            Self::MACRO_FRAGMENT => 247,
        };
        tag.encode(e)
    }
}
impl Decode for SyntaxKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u16::decode(d)? {
            0 => Ok(Self::TOMBSTONE),
            1 => Ok(Self::EOF),
            2 => Ok(Self::SEMICOLON),
            3 => Ok(Self::COMMA),
            4 => Ok(Self::L_PAREN),
            5 => Ok(Self::R_PAREN),
            6 => Ok(Self::L_CURLY),
            7 => Ok(Self::R_CURLY),
            8 => Ok(Self::L_BRACK),
            9 => Ok(Self::R_BRACK),
            10 => Ok(Self::L_ANGLE),
            11 => Ok(Self::R_ANGLE),
            12 => Ok(Self::AT),
            13 => Ok(Self::POUND),
            14 => Ok(Self::TILDE),
            15 => Ok(Self::QUESTION),
            16 => Ok(Self::DOLLAR),
            17 => Ok(Self::AMP),
            18 => Ok(Self::PIPE),
            19 => Ok(Self::PLUS),
            20 => Ok(Self::STAR),
            21 => Ok(Self::SLASH),
            22 => Ok(Self::CARET),
            23 => Ok(Self::PERCENT),
            24 => Ok(Self::UNDERSCORE),
            25 => Ok(Self::DOT),
            26 => Ok(Self::COLON),
            27 => Ok(Self::EQ),
            28 => Ok(Self::BANG),
            29 => Ok(Self::MINUS),
            30 => Ok(Self::DOT2),
            31 => Ok(Self::DOT3),
            32 => Ok(Self::DOT2EQ),
            33 => Ok(Self::COLON2),
            34 => Ok(Self::EQ2),
            35 => Ok(Self::FAT_ARROW),
            36 => Ok(Self::NEQ),
            37 => Ok(Self::THIN_ARROW),
            38 => Ok(Self::LTEQ),
            39 => Ok(Self::GTEQ),
            40 => Ok(Self::AMP2),
            41 => Ok(Self::PIPE2),
            42 => Ok(Self::SHL),
            43 => Ok(Self::SHR),
            44 => Ok(Self::PLUSEQ),
            45 => Ok(Self::MINUSEQ),
            46 => Ok(Self::STAREQ),
            47 => Ok(Self::SLASHEQ),
            48 => Ok(Self::PERCENTEQ),
            49 => Ok(Self::CARETEQ),
            50 => Ok(Self::AMPEQ),
            51 => Ok(Self::PIPEEQ),
            52 => Ok(Self::SHLEQ),
            53 => Ok(Self::SHREQ),
            54 => Ok(Self::AS_KW),
            55 => Ok(Self::ASYNC_KW),
            56 => Ok(Self::AWAIT_KW),
            57 => Ok(Self::BREAK_KW),
            58 => Ok(Self::CONST_KW),
            59 => Ok(Self::CONTINUE_KW),
            60 => Ok(Self::CRATE_KW),
            61 => Ok(Self::DYN_KW),
            62 => Ok(Self::ELSE_KW),
            63 => Ok(Self::ENUM_KW),
            64 => Ok(Self::EXTERN_KW),
            65 => Ok(Self::FALSE_KW),
            66 => Ok(Self::FN_KW),
            67 => Ok(Self::FOR_KW),
            68 => Ok(Self::IF_KW),
            69 => Ok(Self::IMPL_KW),
            70 => Ok(Self::IN_KW),
            71 => Ok(Self::LET_KW),
            72 => Ok(Self::LOOP_KW),
            73 => Ok(Self::MATCH_KW),
            74 => Ok(Self::MOD_KW),
            75 => Ok(Self::MOVE_KW),
            76 => Ok(Self::MUT_KW),
            77 => Ok(Self::PUB_KW),
            78 => Ok(Self::REF_KW),
            79 => Ok(Self::RETURN_KW),
            80 => Ok(Self::SELF_KW),
            81 => Ok(Self::SELF_TYPE_KW),
            82 => Ok(Self::STATIC_KW),
            83 => Ok(Self::STRUCT_KW),
            84 => Ok(Self::SUPER_KW),
            85 => Ok(Self::TRAIT_KW),
            86 => Ok(Self::TRUE_KW),
            87 => Ok(Self::TYPE_KW),
            88 => Ok(Self::UNSAFE_KW),
            89 => Ok(Self::USE_KW),
            90 => Ok(Self::WHERE_KW),
            91 => Ok(Self::WHILE_KW),
            92 => Ok(Self::ABSTRACT_KW),
            93 => Ok(Self::BECOME_KW),
            94 => Ok(Self::BOX_KW),
            95 => Ok(Self::DO_KW),
            96 => Ok(Self::FINAL_KW),
            97 => Ok(Self::GEN_KW),
            98 => Ok(Self::MACRO_KW),
            99 => Ok(Self::OVERRIDE_KW),
            100 => Ok(Self::PRIV_KW),
            101 => Ok(Self::TRY_KW),
            102 => Ok(Self::TYPEOF_KW),
            103 => Ok(Self::UNSIZED_KW),
            104 => Ok(Self::VIRTUAL_KW),
            105 => Ok(Self::YIELD_KW),
            106 => Ok(Self::UNION_KW),
            107 => Ok(Self::INT_NUMBER),
            108 => Ok(Self::FLOAT_NUMBER),
            109 => Ok(Self::CHAR),
            110 => Ok(Self::BYTE),
            111 => Ok(Self::STRING),
            112 => Ok(Self::BYTE_STRING),
            113 => Ok(Self::C_STRING),
            114 => Ok(Self::IDENT),
            115 => Ok(Self::LIFETIME_IDENT),
            116 => Ok(Self::WHITESPACE),
            117 => Ok(Self::COMMENT),
            118 => Ok(Self::ERROR),
            119 => Ok(Self::SOURCE_FILE),
            120 => Ok(Self::ATTR),
            121 => Ok(Self::META),
            122 => Ok(Self::TOKEN_TREE),
            123 => Ok(Self::MACRO_CALL),
            124 => Ok(Self::VISIBILITY),
            125 => Ok(Self::NAME),
            126 => Ok(Self::NAME_REF),
            127 => Ok(Self::LIFETIME),
            128 => Ok(Self::FN),
            129 => Ok(Self::STRUCT),
            130 => Ok(Self::UNION),
            131 => Ok(Self::ENUM),
            132 => Ok(Self::IMPL),
            133 => Ok(Self::TRAIT),
            134 => Ok(Self::TYPE_ALIAS),
            135 => Ok(Self::CONST),
            136 => Ok(Self::STATIC),
            137 => Ok(Self::MODULE),
            138 => Ok(Self::USE),
            139 => Ok(Self::EXTERN_CRATE),
            140 => Ok(Self::EXTERN_BLOCK),
            141 => Ok(Self::EXTERN_ITEM_LIST),
            142 => Ok(Self::ABI),
            143 => Ok(Self::ITEM_LIST),
            144 => Ok(Self::ASSOC_ITEM_LIST),
            145 => Ok(Self::USE_TREE),
            146 => Ok(Self::USE_TREE_LIST),
            147 => Ok(Self::RENAME),
            148 => Ok(Self::PARAM_LIST),
            149 => Ok(Self::PARAM),
            150 => Ok(Self::SELF_PARAM),
            151 => Ok(Self::RET_TYPE),
            152 => Ok(Self::RECORD_FIELD_LIST),
            153 => Ok(Self::RECORD_FIELD),
            154 => Ok(Self::TUPLE_FIELD_LIST),
            155 => Ok(Self::TUPLE_FIELD),
            156 => Ok(Self::VARIANT_LIST),
            157 => Ok(Self::VARIANT),
            158 => Ok(Self::GENERIC_PARAM_LIST),
            159 => Ok(Self::TYPE_PARAM),
            160 => Ok(Self::LIFETIME_PARAM),
            161 => Ok(Self::CONST_PARAM),
            162 => Ok(Self::WHERE_CLAUSE),
            163 => Ok(Self::WHERE_PRED),
            164 => Ok(Self::TYPE_BOUND_LIST),
            165 => Ok(Self::TYPE_BOUND),
            166 => Ok(Self::USE_BOUND_GENERIC_ARGS),
            167 => Ok(Self::PATH),
            168 => Ok(Self::PATH_SEGMENT),
            169 => Ok(Self::GENERIC_ARG_LIST),
            170 => Ok(Self::TYPE_ARG),
            171 => Ok(Self::LIFETIME_ARG),
            172 => Ok(Self::CONST_ARG),
            173 => Ok(Self::ASSOC_TYPE_ARG),
            174 => Ok(Self::PARENTHESIZED_ARG_LIST),
            175 => Ok(Self::PATH_TYPE),
            176 => Ok(Self::REF_TYPE),
            177 => Ok(Self::PTR_TYPE),
            178 => Ok(Self::ARRAY_TYPE),
            179 => Ok(Self::SLICE_TYPE),
            180 => Ok(Self::TUPLE_TYPE),
            181 => Ok(Self::PAREN_TYPE),
            182 => Ok(Self::NEVER_TYPE),
            183 => Ok(Self::INFER_TYPE),
            184 => Ok(Self::FN_PTR_TYPE),
            185 => Ok(Self::FOR_TYPE),
            186 => Ok(Self::IMPL_TRAIT_TYPE),
            187 => Ok(Self::DYN_TRAIT_TYPE),
            188 => Ok(Self::MACRO_TYPE),
            189 => Ok(Self::IDENT_PAT),
            190 => Ok(Self::WILDCARD_PAT),
            191 => Ok(Self::LITERAL_PAT),
            192 => Ok(Self::RANGE_PAT),
            193 => Ok(Self::PATH_PAT),
            194 => Ok(Self::TUPLE_STRUCT_PAT),
            195 => Ok(Self::RECORD_PAT),
            196 => Ok(Self::RECORD_PAT_FIELD_LIST),
            197 => Ok(Self::RECORD_PAT_FIELD),
            198 => Ok(Self::TUPLE_PAT),
            199 => Ok(Self::SLICE_PAT),
            200 => Ok(Self::REF_PAT),
            201 => Ok(Self::OR_PAT),
            202 => Ok(Self::REST_PAT),
            203 => Ok(Self::PAREN_PAT),
            204 => Ok(Self::MACRO_PAT),
            205 => Ok(Self::LITERAL),
            206 => Ok(Self::PATH_EXPR),
            207 => Ok(Self::TUPLE_EXPR),
            208 => Ok(Self::PAREN_EXPR),
            209 => Ok(Self::ARRAY_EXPR),
            210 => Ok(Self::BLOCK_EXPR),
            211 => Ok(Self::LABEL),
            212 => Ok(Self::LOOP_EXPR),
            213 => Ok(Self::WHILE_EXPR),
            214 => Ok(Self::FOR_EXPR),
            215 => Ok(Self::IF_EXPR),
            216 => Ok(Self::LET_EXPR),
            217 => Ok(Self::MATCH_EXPR),
            218 => Ok(Self::MATCH_ARM_LIST),
            219 => Ok(Self::MATCH_ARM),
            220 => Ok(Self::MATCH_GUARD),
            221 => Ok(Self::BREAK_EXPR),
            222 => Ok(Self::CONTINUE_EXPR),
            223 => Ok(Self::RETURN_EXPR),
            224 => Ok(Self::CLOSURE_EXPR),
            225 => Ok(Self::PREFIX_EXPR),
            226 => Ok(Self::REF_EXPR),
            227 => Ok(Self::BIN_EXPR),
            228 => Ok(Self::CAST_EXPR),
            229 => Ok(Self::RANGE_EXPR),
            230 => Ok(Self::CALL_EXPR),
            231 => Ok(Self::ARG_LIST),
            232 => Ok(Self::METHOD_CALL_EXPR),
            233 => Ok(Self::FIELD_EXPR),
            234 => Ok(Self::INDEX_EXPR),
            235 => Ok(Self::TRY_EXPR),
            236 => Ok(Self::AWAIT_EXPR),
            237 => Ok(Self::RECORD_EXPR),
            238 => Ok(Self::RECORD_EXPR_FIELD_LIST),
            239 => Ok(Self::RECORD_EXPR_FIELD),
            240 => Ok(Self::UNDERSCORE_EXPR),
            241 => Ok(Self::MACRO_EXPR),
            242 => Ok(Self::LET_STMT),
            243 => Ok(Self::LET_ELSE),
            244 => Ok(Self::EXPR_STMT),
            245 => Ok(Self::MACRO_ITEMS),
            246 => Ok(Self::MACRO_STMTS),
            247 => Ok(Self::MACRO_FRAGMENT),
            _ => Err(Error("invalid syntax kind tag")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_macro_tokens_cannot_overflow_matcher_reconstruction() {
        let token = |kind| Tok {
            edition: flux_syntax::Edition::E2024,
            kind,
            text: flux_intern::Symbol::intern(""),
            joint: false,
            origin: TokenOrigin {
                range: flux_syntax::TextRange::empty(0),
                group: 0,
                ctxt: 0,
            },
        };
        let depth = super::super::MAX_NESTING_DEPTH as usize;
        let mut tokens = vec![token(SyntaxKind::L_PAREN); depth];
        tokens.extend(vec![token(SyntaxKind::R_PAREN); depth]);
        assert!(validate_rule_tokens(&tokens).is_ok());
        tokens.insert(0, token(SyntaxKind::L_PAREN));
        tokens.push(token(SyntaxKind::R_PAREN));
        assert!(validate_rule_tokens(&tokens).is_err());
    }
}
