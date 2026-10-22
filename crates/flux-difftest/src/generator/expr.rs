//! Expressions, generated for the type they must have.

use std::fmt::Write;

use super::func::{Ex, FnGen, paren_if_brace};
use super::ty::{self, IntTy, Ty};
use super::{FnKind, FnSig, Recv};

/// Replaces `Ty::Param` with `with`.
pub fn subst(ty: &Ty, with: &Ty) -> Ty {
    match ty {
        Ty::Param => with.clone(),
        Ty::Tuple(e) => Ty::Tuple(e.iter().map(|t| subst(t, with)).collect()),
        Ty::Array(t, n) => Ty::Array(Box::new(subst(t, with)), *n),
        Ty::Option(t) => Ty::Option(Box::new(subst(t, with))),
        Ty::Result(t, e) => Ty::Result(Box::new(subst(t, with)), Box::new(subst(e, with))),
        Ty::Ref(t) => Ty::Ref(Box::new(subst(t, with))),
        Ty::RefMut(t) => Ty::RefMut(Box::new(subst(t, with))),
        Ty::Fn(p, r) => Ty::Fn(
            p.iter().map(|t| subst(t, with)).collect(),
            Box::new(subst(r, with)),
        ),
        t => t.clone(),
    }
}

/// An item type of an iterator chain being built.
struct Chain {
    text: String,
    item: Ty,
    /// `DoubleEndedIterator`, so `rev` works.
    de: bool,
    /// `ExactSizeIterator`.
    exact: bool,
    /// `DoubleEndedIterator` through adapters flux implements it for so
    /// far (see `Weights::rev_adapters`).
    simple_de: bool,
}

impl FnGen<'_> {
    /// An expression of type `ty`, at most `depth` deep.
    pub fn expr(&mut self, ty: &Ty, depth: u32) -> Ex {
        self.add_cost(0);
        if depth == 0 || self.over_budget() {
            return self.leaf(ty);
        }
        match ty {
            Ty::Int(i) => self.int_expr(*i, depth),
            Ty::Bool => self.bool_expr(depth),
            Ty::Char => self.char_expr(depth),
            _ => self.other_expr(ty, depth),
        }
    }

    /// `bb(e)`: a value rustc's lints cannot see through.
    pub fn opaque(&mut self, e: Ex) -> Ex {
        Ex::new(format!("bb({})", e.s), false)
    }

    // ----- leaves -----

    /// A variable's value, a literal or a constructor of leaves.
    pub fn leaf(&mut self, ty: &Ty) -> Ex {
        if self.g.is_copy(ty) && !matches!(ty, Ty::Fn(..)) {
            let places = self.readable_places(ty);
            if !places.is_empty() && self.g.src.chance(65) {
                let p = &places[self.g.src.below(places.len() as u32) as usize];
                let known = self.vars[p.root].known;
                return Ex::place(p.text.clone(), known, p.root);
            }
            if !self.const_mode && self.g.src.chance(20) {
                // A constant or static of this type.
                let mut globals: Vec<String> = Vec::new();
                for (i, c) in self
                    .g
                    .const_fns
                    .iter()
                    .enumerate()
                    .take(self.callable_const)
                {
                    if &c.ret == ty {
                        globals.push(format!("C{i}"));
                    }
                }
                for s in &self.g.statics {
                    if &s.ty == ty && !s.mutable {
                        globals.push(s.name.clone());
                    }
                }
                if !globals.is_empty() {
                    let g = globals.swap_remove(self.g.src.below(globals.len() as u32) as usize);
                    return Ex::new(g, true);
                }
            }
        } else if !matches!(ty, Ty::Fn(..)) {
            let vars = self.movable_vars(ty);
            if !vars.is_empty() && self.g.src.chance(60) {
                let v = vars[self.g.src.below(vars.len() as u32) as usize];
                self.vars[v].moved = true;
                return Ex::place(self.vars[v].name.clone(), false, v);
            }
        }
        self.construct(ty, 0)
    }

    /// A literal, or a constructor whose parts are at most `depth` deep.
    pub fn construct(&mut self, ty: &Ty, depth: u32) -> Ex {
        let sub = depth.saturating_sub(1);
        match ty {
            Ty::Int(i) => {
                let v = ty::int_value(&mut self.g.src, *i);
                Ex::new(ty::int_literal(*i, v), true)
            }
            Ty::Bool => Ex::new(
                if self.g.src.chance(50) {
                    "true"
                } else {
                    "false"
                },
                true,
            ),
            Ty::Char => Ex::new(ty::char_literal(&mut self.g.src).1, true),
            Ty::Unit => Ex::new("()", true),
            Ty::Tuple(elems) => {
                let parts: Vec<Ex> = elems.iter().map(|t| self.expr(t, sub)).collect();
                let konst = parts.iter().any(|p| p.konst);
                let texts: Vec<String> = parts.into_iter().map(|p| p.s).collect();
                let s = if texts.len() == 1 {
                    format!("({},)", texts[0])
                } else {
                    format!("({})", texts.join(", "))
                };
                Ex::new(s, konst)
            }
            Ty::Array(elem, n) => {
                if self.g.is_copy(elem)
                    && (*n > 2 || self.g.src.chance(30))
                    && !self.const_mode_array_repeat_blocked(elem)
                {
                    let e = self.expr(elem, sub);
                    Ex::new(format!("[{}; {n}]", e.s), e.konst)
                } else {
                    let parts: Vec<Ex> = (0..*n).map(|_| self.expr(elem, sub)).collect();
                    let konst = parts.iter().any(|p| p.konst);
                    let texts: Vec<String> = parts.into_iter().map(|p| p.s).collect();
                    Ex::new(format!("[{}]", texts.join(", ")), konst)
                }
            }
            Ty::Struct(s) => {
                let s = *s;
                let def_fields = self.g.structs[s].fields.clone();
                let tuple = self.g.structs[s].tuple;
                let copy = self.g.structs[s].copy;
                // `..base` from a variable of the same `Copy` struct.
                if copy && !tuple && def_fields.len() > 1 && self.g.src.chance(20) {
                    let bases = self.readable_places(ty);
                    if !bases.is_empty() {
                        let base = bases[self.g.src.below(bases.len() as u32) as usize].clone();
                        let k = self.g.src.range(1, def_fields.len() as u32 - 1) as usize;
                        let mut parts = Vec::new();
                        for (i, t) in def_fields.iter().enumerate().take(k) {
                            let e = self.expr(t, sub);
                            parts.push(format!("{}: {}", self.g.structs[s].field_name(i), e.s));
                        }
                        return Ex::new(
                            format!("S{s} {{ {}, ..{} }}", parts.join(", "), base.text),
                            false,
                        );
                    }
                }
                let mut konst = false;
                let mut parts = Vec::new();
                for (i, t) in def_fields.iter().enumerate() {
                    let e = self.expr(t, sub);
                    konst |= e.konst;
                    if tuple {
                        parts.push(e.s);
                    } else {
                        parts.push(format!("{}: {}", self.g.structs[s].field_name(i), e.s));
                    }
                }
                if tuple {
                    Ex::new(format!("S{s}({})", parts.join(", ")), konst)
                } else {
                    Ex::new(format!("S{s} {{ {} }}", parts.join(", ")), konst)
                }
            }
            Ty::Enum(e) => {
                let e = *e;
                let nv = self.g.enums[e].variants.len() as u32;
                let k = self.g.src.below(nv) as usize;
                let fields = self.g.enums[e].variants[k].fields.clone();
                let named = self.g.enums[e].variants[k].named;
                if fields.is_empty() {
                    return Ex::new(format!("E{e}::V{k}"), true);
                }
                let mut konst = false;
                let mut parts = Vec::new();
                for (i, t) in fields.iter().enumerate() {
                    let x = self.expr(t, sub);
                    konst |= x.konst;
                    if named {
                        parts.push(format!("{}: {}", ["a", "b", "c", "d"][i], x.s));
                    } else {
                        parts.push(x.s);
                    }
                }
                if named {
                    Ex::new(format!("E{e}::V{k} {{ {} }}", parts.join(", ")), konst)
                } else {
                    Ex::new(format!("E{e}::V{k}({})", parts.join(", ")), konst)
                }
            }
            Ty::Option(t) => {
                if self.g.src.chance(25) {
                    Ex::new(format!("None::<{}>", t.name()), true)
                } else {
                    let x = self.expr(t, sub);
                    Ex::new(format!("Some({})", x.s), x.konst)
                }
            }
            Ty::Result(t, e) => {
                if self.g.src.chance(30) {
                    let x = self.expr(e, sub);
                    Ex::new(
                        format!("Err::<{}, {}>({})", t.name(), e.name(), x.s),
                        x.konst,
                    )
                } else {
                    let x = self.expr(t, sub);
                    Ex::new(
                        format!("Ok::<{}, {}>({})", t.name(), e.name(), x.s),
                        x.konst,
                    )
                }
            }
            Ty::Param => {
                let m = self.g.tr.as_ref().unwrap().make_ty;
                let x = self.expr(&Ty::Int(m), sub);
                Ex::new(format!("T::make({})", x.s), false)
            }
            Ty::Ref(t) => {
                let x = self.expr(t, sub);
                Ex::new(format!("&{}", temp(&x)), false)
            }
            Ty::RefMut(t) => {
                let x = self.expr(t, sub);
                Ex::new(format!("&mut {}", temp(&x)), false)
            }
            Ty::Fn(params, ret) => {
                let names: Vec<String> = params.iter().map(|_| self.fresh("a")).collect();
                let body = self.closure_body(&names, params, ret, true);
                let ps: Vec<String> = names
                    .iter()
                    .zip(params.iter())
                    .map(|(n, t)| format!("{n}: {}", t.name()))
                    .collect();
                Ex::new(
                    format!("move |{}| -> {} {body}", ps.join(", "), ret.name()),
                    false,
                )
            }
        }
    }

    /// Statics may not repeat a value that is not a constant.
    fn const_mode_array_repeat_blocked(&self, _elem: &Ty) -> bool {
        false
    }

    // ----- integers -----

    fn int_expr(&mut self, i: IntTy, depth: u32) -> Ex {
        let ty = Ty::Int(i);
        let d = depth - 1;
        let w = self.g.cfg.weights.clone();
        if self.const_mode {
            return match self.g.src.weighted(&[
                10,
                8,
                6,
                6,
                3,
                3,
                if self.callable_const > 0 { 3 } else { 0 },
            ]) {
                0 => self.leaf(&ty),
                1 => {
                    let op = [
                        "wrapping_add",
                        "wrapping_sub",
                        "wrapping_mul",
                        "rotate_left",
                    ][self.g.src.below(4) as usize];
                    let a = self.expr(&ty, d);
                    let b = if op == "rotate_left" {
                        self.expr(&Ty::Int(IntTy::U32), d)
                    } else {
                        self.expr(&ty, d)
                    };
                    Ex::new(format!("{}.{op}({})", recv(&a.s), b.s), false)
                }
                2 => {
                    let op = ["&", "|", "^"][self.g.src.below(3) as usize];
                    let a = self.expr(&ty, d);
                    let b = self.expr(&ty, d);
                    Ex::new(format!("({} {op} {})", a.s, b.s), a.konst && b.konst)
                }
                3 => self.cast_to(i, d),
                4 => self.if_expr(&ty, d),
                5 => {
                    let a = self.expr(&ty, d);
                    let b = self.expr(&ty, d);
                    Ex::new(format!("{}.wrapping_div({} | 1)", recv(&a.s), b.s), false)
                }
                _ => self.const_call(&ty, d).unwrap_or_else(|| self.leaf(&ty)),
            };
        }
        let weights = [
            18,                                            // leaf
            14,                                            // + - *
            8,                                             // & | ^
            5,                                             // shifts
            5,                                             // / %
            4,                                             // unary
            10,                                            // cast
            12,                                            // methods
            4,                                             // if
            w.matches / 8,                                 // match
            3,                                             // block
            8,                                             // call
            w.side_effects / 4,                            // probe
            w.aggregates / 6,                              // index
            w.iterators / 5,                               // iterator chain
            w.closures / 4,                                // closure call
            w.options / 5,                                 // unwrap_or
            if self.in_impl { 0 } else { w.generics / 5 }, // trait items
            w.loops / 10,                                  // loop value
            if self.can_question() { 3 } else { 0 },       // ?
            w.slices / 6,                                  // slice lengths
        ];
        match self.g.src.weighted(&weights) {
            0 => self.leaf(&ty),
            1 => {
                let op = ["+", "-", "*"][self.g.src.below(3) as usize];
                let a = self.expr(&ty, d);
                let mut b = self.expr(&ty, d);
                if a.konst && b.konst {
                    b = self.opaque(b);
                }
                Ex::new(format!("({} {op} {})", a.s, b.s), false)
            }
            2 => {
                let op = ["&", "|", "^"][self.g.src.below(3) as usize];
                let a = self.expr(&ty, d);
                let b = self.expr(&ty, d);
                Ex::new(format!("({} {op} {})", a.s, b.s), a.konst && b.konst)
            }
            3 => {
                let op = ["<<", ">>"][self.g.src.below(2) as usize];
                let a = self.expr(&ty, d);
                let s = self.shift_amount(i);
                Ex::new(format!("({} {op} {})", a.s, s.s), a.konst && s.konst)
            }
            4 => self.div_expr(i, d),
            5 => {
                let a = self.expr(&ty, d);
                if i.signed() && self.g.src.chance(50) {
                    let a = if a.konst { self.opaque(a) } else { a };
                    Ex::new(format!("(-{})", a.s), false)
                } else {
                    Ex::new(format!("(!{})", a.s), a.konst)
                }
            }
            6 => self.cast_to(i, d),
            7 => self.int_method(i, d),
            8 => self.if_expr(&ty, d),
            9 => self.match_value(&ty, d),
            10 => self.block_expr(&ty, d),
            11 => self
                .call_returning(&ty, d)
                .unwrap_or_else(|| self.leaf(&ty)),
            12 => self.probe_expr(&ty, d),
            13 => self.index_expr(&ty, d).unwrap_or_else(|| self.leaf(&ty)),
            14 => self.iter_expr(&ty, d).unwrap_or_else(|| self.leaf(&ty)),
            15 => self.closure_call(&ty, d).unwrap_or_else(|| self.leaf(&ty)),
            16 => self.unwrap_or(&ty, d),
            17 => self.trait_int(i, d).unwrap_or_else(|| self.leaf(&ty)),
            18 => self.loop_expr(&ty, d),
            19 => self.question(&ty, d),
            _ => self.slice_len(i, d).unwrap_or_else(|| self.leaf(&ty)),
        }
    }

    /// A shift amount for a value of type `i`: any integer, but in range
    /// if rustc could know it.
    pub fn shift_amount(&mut self, i: IntTy) -> Ex {
        let by = self.g.int_ty();
        if !self.const_mode && self.g.src.chance(40) {
            let e = self.expr(&Ty::Int(by), 1);
            if !e.konst {
                return e;
            }
        }
        let n = self
            .g
            .src
            .below(i.bits().min(by.max_bits().min(127) as u32 + 1));
        Ex::new(ty::int_literal(by, n as u128), true)
    }

    fn div_expr(&mut self, i: IntTy, d: u32) -> Ex {
        let ty = Ty::Int(i);
        let a = self.expr(&ty, d);
        let b = self.expr(&ty, d);
        match self.g.src.below(4) {
            0 => {
                let op = ["wrapping_div", "wrapping_rem"][self.g.src.below(2) as usize];
                Ex::new(format!("{}.{op}({} | 1)", recv(&a.s), b.s), false)
            }
            1 => {
                let op = ["checked_div", "checked_rem"][self.g.src.below(2) as usize];
                let c = self.expr(&ty, d);
                Ex::new(
                    format!("{}.{op}({}).unwrap_or({})", recv(&a.s), b.s, c.s),
                    false,
                )
            }
            _ => {
                let op = ["/", "%"][self.g.src.below(2) as usize];
                if i.signed() {
                    // A positive divisor: no division by zero, no `MIN / -1`.
                    let max = ty::int_literal(i, i.max_bits());
                    let a = if a.konst && b.konst {
                        self.opaque(a)
                    } else {
                        a
                    };
                    Ex::new(format!("({} {op} (({} & {max}) | 1))", a.s, b.s), false)
                } else {
                    let a = if a.konst && b.konst {
                        self.opaque(a)
                    } else {
                        a
                    };
                    Ex::new(format!("({} {op} ({} | 1))", a.s, b.s), false)
                }
            }
        }
    }

    fn cast_to(&mut self, i: IntTy, d: u32) -> Ex {
        let from =
            match self
                .g
                .src
                .weighted(&[8, 2, if self.g.cfg.weights.chars > 0 { 1 } else { 0 }])
            {
                0 => Ty::Int(self.g.int_ty()),
                1 => Ty::Bool,
                _ => Ty::Char,
            };
        let e = self.expr(&from, d);
        Ex::new(format!("({} as {})", e.s, i.name()), e.konst)
    }

    fn int_method(&mut self, i: IntTy, d: u32) -> Ex {
        let ty = Ty::Int(i);
        let u32t = Ty::Int(IntTy::U32);
        let a = self.expr(&ty, d);
        let r = recv(&a.s);
        let unsigned_of = |i: IntTy| match i {
            IntTy::I8 => IntTy::U8,
            IntTy::I16 => IntTy::U16,
            IntTy::I32 => IntTy::U32,
            IntTy::I64 => IntTy::U64,
            IntTy::I128 => IntTy::U128,
            IntTy::Isize => IntTy::Usize,
            u => u,
        };
        match self.g.src.below(14) {
            0 => {
                let op = [
                    "wrapping_add",
                    "wrapping_sub",
                    "wrapping_mul",
                    "saturating_add",
                    "saturating_sub",
                    "saturating_mul",
                    "min",
                    "max",
                ][self.g.src.below(8) as usize];
                let b = self.expr(&ty, d);
                Ex::new(format!("{r}.{op}({})", b.s), false)
            }
            1 => {
                let op = [
                    "rotate_left",
                    "rotate_right",
                    "wrapping_shl",
                    "wrapping_shr",
                    "wrapping_pow",
                ][self.g.src.below(5) as usize];
                let b = self.expr(&u32t, d);
                Ex::new(format!("{r}.{op}({})", b.s), false)
            }
            2 => Ex::new(
                format!(
                    "{r}.{}()",
                    [
                        "swap_bytes",
                        "reverse_bits",
                        "wrapping_neg",
                        "to_be",
                        "to_le"
                    ][self.g.src.below(5) as usize]
                ),
                false,
            ),
            3 if i == IntTy::U32 => Ex::new(
                format!(
                    "{r}.{}()",
                    [
                        "count_ones",
                        "count_zeros",
                        "leading_zeros",
                        "trailing_zeros",
                        "leading_ones",
                        "trailing_ones"
                    ][self.g.src.below(6) as usize]
                ),
                false,
            ),
            4 if i.signed() => Ex::new(
                format!(
                    "{r}.{}()",
                    ["abs", "signum", "wrapping_abs"][self.g.src.below(3) as usize]
                ),
                false,
            ),
            5 => {
                let op = ["overflowing_add", "overflowing_sub", "overflowing_mul"]
                    [self.g.src.below(3) as usize];
                let b = self.expr(&ty, d);
                Ex::new(format!("{r}.{op}({}).0", b.s), false)
            }
            6 => {
                let op =
                    ["checked_add", "checked_sub", "checked_mul"][self.g.src.below(3) as usize];
                let b = self.expr(&ty, d);
                let c = self.expr(&ty, d);
                Ex::new(format!("{r}.{op}({}).unwrap_or({})", b.s, c.s), false)
            }
            7 if !i.signed() => {
                let b = self.expr(&ty, d);
                let op = ["div_euclid", "rem_euclid", "div_ceil"][self.g.src.below(3) as usize];
                Ex::new(format!("{r}.{op}({} | 1)", b.s), false)
            }
            8 if i.signed() => {
                // `unsigned_abs` and `abs_diff` give the unsigned type: cast back.
                let b = self.expr(&ty, d);
                let op = if self.g.src.chance(50) {
                    "unsigned_abs()".to_owned()
                } else {
                    format!("abs_diff({})", b.s)
                };
                Ex::new(format!("({r}.{op} as {})", i.name()), false)
            }
            9 => {
                let (lo, hi) = {
                    let x = ty::int_value(&mut self.g.src, i);
                    let y = ty::int_value(&mut self.g.src, i);
                    if i.cmp(x, y).is_le() { (x, y) } else { (y, x) }
                };
                Ex::new(
                    format!(
                        "{r}.clamp({}, {})",
                        ty::int_literal(i, lo),
                        ty::int_literal(i, hi)
                    ),
                    false,
                )
            }
            10 => {
                let b = self.expr(&ty, d);
                let f = ["core::cmp::max", "core::cmp::min"][self.g.src.below(2) as usize];
                Ex::new(format!("{f}({}, {})", a.s, b.s), false)
            }
            11 if i.bits() > 8 => {
                // `From` from a narrower type.
                let narrow: Vec<IntTy> = IntTy::ALL
                    .iter()
                    .copied()
                    .filter(|n| {
                        n.bits() < i.bits()
                            && (n.signed() == i.signed() || !n.signed() && i.signed())
                            && !matches!(n, IntTy::Isize | IntTy::Usize)
                            && !matches!(i, IntTy::Isize | IntTy::Usize)
                    })
                    .collect();
                if narrow.is_empty() {
                    return a;
                }
                let n = narrow[self.g.src.below(narrow.len() as u32) as usize];
                let b = self.expr(&Ty::Int(n), d);
                Ex::new(format!("{}::from({})", i.name(), b.s), false)
            }
            12 => {
                let u = unsigned_of(i);
                let b = self.expr(&Ty::Int(u), d);
                Ex::new(
                    format!("({}.isqrt() as {}).wrapping_add({r})", recv(&b.s), i.name()),
                    false,
                )
            }
            _ => {
                let bytes = i.bits() / 8;
                let arr = Ty::Array(Box::new(Ty::Int(IntTy::U8)), bytes as usize);
                let b = self.expr(&arr, d);
                let f = ["from_le_bytes", "from_be_bytes"][self.g.src.below(2) as usize];
                Ex::new(format!("{}::{f}({})", i.name(), b.s), false)
            }
        }
    }

    // ----- booleans and chars -----

    fn bool_expr(&mut self, depth: u32) -> Ex {
        let d = depth - 1;
        let w = self.g.cfg.weights.clone();
        let c = self.const_mode;
        let weights = [
            10,
            16,
            if c { 0 } else { 5 },
            4,
            8,
            if c { 0 } else { w.matches / 6 },
            if c { 0 } else { 5 },
            3,
            if c { 0 } else { 5 },
            if c { 0 } else { w.iterators / 6 },
            if c { 0 } else { w.side_effects / 5 },
            if c { 0 } else { w.slices / 8 },
            if c { 0 } else { w.options / 6 },
        ];
        match self.g.src.weighted(&weights) {
            0 => self.leaf(&Ty::Bool),
            1 => {
                // compare scalars
                let t = match self
                    .g
                    .src
                    .weighted(&[8, 1, if w.chars > 0 { 1 } else { 0 }])
                {
                    0 => Ty::Int(self.g.int_ty()),
                    1 => Ty::Bool,
                    _ => Ty::Char,
                };
                let a = self.expr(&t, d);
                let b = self.expr(&t, d);
                let op = ["==", "!=", "<", "<=", ">", ">="][self.g.src.below(6) as usize];
                Ex::new(format!("({} {op} {})", a.s, b.s), a.konst && b.konst)
            }
            2 => {
                // compare aggregates with `==`
                let t = self.g.random_ty(2);
                if !self.g.is_eq(&t) {
                    return self.leaf(&Ty::Bool);
                }
                let a = self.expr(&t, d);
                let mark = self.borrow_mark();
                if let Some(r) = a.root {
                    self.borrow(r, false);
                }
                let b = self.expr(&t, d);
                self.release_borrows(mark);
                let op = ["==", "!="][self.g.src.below(2) as usize];
                Ex::new(format!("({} {op} {})", a.s, b.s), a.konst && b.konst)
            }
            3 => {
                let a = self.expr(&Ty::Bool, d);
                Ex::new(format!("!{}", a.s), a.konst)
            }
            4 => {
                let op = ["&&", "||"][self.g.src.below(2) as usize];
                let a = self.expr(&Ty::Bool, d);
                let b = self.branches(2, |f, i| (i == 0).then(|| f.expr(&Ty::Bool, d)));
                let b = b.into_iter().next().flatten().unwrap();
                Ex::new(format!("({} {op} {})", a.s, b.s), a.konst && b.konst)
            }
            5 => self.matches_expr(d),
            6 => {
                let i = self.g.int_ty();
                let a = self.expr(&Ty::Int(i), d);
                let r = recv(&a.s);
                let s = if i.signed() {
                    format!(
                        "{r}.{}()",
                        ["is_negative", "is_positive"][self.g.src.below(2) as usize]
                    )
                } else if self.g.src.chance(50) {
                    format!("{r}.is_power_of_two()")
                } else {
                    let b = self.expr(&Ty::Int(i), d);
                    format!("{r}.overflowing_add({}).1", b.s)
                };
                Ex::new(s, false)
            }
            7 => self.if_expr(&Ty::Bool, d),
            8 => self
                .call_returning(&Ty::Bool, d)
                .unwrap_or_else(|| self.leaf(&Ty::Bool)),
            9 => self
                .iter_expr(&Ty::Bool, d)
                .unwrap_or_else(|| self.leaf(&Ty::Bool)),
            10 => self.probe_expr(&Ty::Bool, d),
            11 => self.slice_bool(d).unwrap_or_else(|| self.leaf(&Ty::Bool)),
            _ => {
                let t = self.g.random_ty(1);
                let o = self.expr(&Ty::Option(Box::new(t)), d);
                let m = ["is_some", "is_none"][self.g.src.below(2) as usize];
                Ex::new(format!("{}.{m}()", recv(&o.s)), false)
            }
        }
    }

    fn char_expr(&mut self, depth: u32) -> Ex {
        let d = depth - 1;
        if self.const_mode {
            return self.leaf(&Ty::Char);
        }
        match self.g.src.weighted(&[10, 4, 3, 3, 3, 3, 3]) {
            0 => self.leaf(&Ty::Char),
            1 => {
                let b = self.expr(&Ty::Int(IntTy::U8), d);
                Ex::new(format!("({} as char)", b.s), b.konst)
            }
            2 => {
                let b = self.expr(&Ty::Int(IntTy::U32), d);
                let (_, dflt) = ty::char_literal(&mut self.g.src);
                if self.g.src.chance(50) {
                    Ex::new(format!("char::from_u32({}).unwrap_or({dflt})", b.s), false)
                } else {
                    Ex::new(
                        format!("char::from_digit({}, 10).unwrap_or({dflt})", b.s),
                        false,
                    )
                }
            }
            3 => {
                let c = self.expr(&Ty::Char, d);
                let m = ["to_ascii_uppercase", "to_ascii_lowercase"][self.g.src.below(2) as usize];
                Ex::new(format!("{}.{m}()", recv(&c.s)), false)
            }
            4 => self.if_expr(&Ty::Char, d),
            5 => self
                .call_returning(&Ty::Char, d)
                .unwrap_or_else(|| self.leaf(&Ty::Char)),
            _ => self.match_value(&Ty::Char, d),
        }
    }

    // ----- everything else -----

    fn other_expr(&mut self, ty: &Ty, depth: u32) -> Ex {
        let d = depth - 1;
        let w = self.g.cfg.weights.clone();
        if self.const_mode {
            return self.construct(ty, depth);
        }
        let weights = [
            12,            // leaf
            14,            // constructor
            4,             // if
            w.matches / 8, // match
            2,             // block
            10,            // call
            if self.g.is_hashable(ty) {
                w.side_effects / 5
            } else {
                0
            }, // probe
            w.closures / 5, // closure call
            if self.can_question() { 2 } else { 0 }, // ?
            6,             // type-specific
            w.loops / 12,  // loop value
        ];
        match self.g.src.weighted(&weights) {
            0 => self.leaf(ty),
            1 => self.construct(ty, depth),
            2 => self.if_expr(ty, d),
            3 => self.match_value(ty, d),
            4 => self.block_expr(ty, d),
            5 => self.call_returning(ty, d).unwrap_or_else(|| self.leaf(ty)),
            6 => self.probe_expr(ty, d),
            7 => self.closure_call(ty, d).unwrap_or_else(|| self.leaf(ty)),
            8 => self.question(ty, d),
            9 => self
                .special(ty, d)
                .unwrap_or_else(|| self.construct(ty, depth)),
            _ => self.loop_expr(ty, d),
        }
    }

    /// Expressions only some types have.
    fn special(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        match ty {
            Ty::Option(t) => {
                let t = (**t).clone();
                match self.g.src.below(6) {
                    0 if t.int().is_some() => {
                        let i = t.int().unwrap();
                        let a = self.expr(&t, d);
                        let b = self.expr(&t, d);
                        let op = [
                            "checked_add",
                            "checked_sub",
                            "checked_mul",
                            "checked_div",
                            "checked_rem",
                        ][self.g.src.below(5) as usize];
                        let _ = i;
                        Some(Ex::new(format!("{}.{op}({})", recv(&a.s), b.s), false))
                    }
                    1 if self.g.is_copy(&t) => {
                        // An element of an array, if the index is in range.
                        let arr = Ty::Array(Box::new(t.clone()), self.g.src.range(1, 4) as usize);
                        let a = self.expr(&arr, d);
                        let i = self.expr(&Ty::Int(IntTy::Usize), d);
                        let i = Ex::new(format!("({} % 6)", i.s), false);
                        let m = ["first()", "last()"][self.g.src.below(2) as usize];
                        if self.g.src.chance(50) {
                            Some(Ex::new(
                                format!("{}.get({}).copied()", recv(&a.s), i.s),
                                false,
                            ))
                        } else {
                            Some(Ex::new(format!("{}.{m}.copied()", recv(&a.s)), false))
                        }
                    }
                    2 if self.g.is_copy(&t) => {
                        let o = self.expr(ty, d);
                        let x = self.fresh("x");
                        let body =
                            self.pure_closure(&[x], &[Ty::Ref(Box::new(t.clone()))], &Ty::Bool, d);
                        Some(Ex::new(format!("{}.filter({body})", recv(&o.s)), false))
                    }
                    3 => {
                        let u = self.g.random_ty(1);
                        let o = self.expr(&Ty::Option(Box::new(u.clone())), d);
                        let x = self.fresh("x");
                        let body = self.pure_closure(&[x], &[u], &t, d);
                        Some(Ex::new(format!("{}.map({body})", recv(&o.s)), false))
                    }
                    4 => {
                        let a = self.expr(ty, d);
                        let b = self.expr(ty, d);
                        let m = ["or", "xor"][self.g.src.below(2) as usize];
                        Some(Ex::new(format!("{}.{m}({})", recv(&a.s), b.s), false))
                    }
                    _ => {
                        // `take` from a variable.
                        let places = self.writable_places(Some(ty));
                        let places: Vec<_> = places
                            .into_iter()
                            .filter(|p| !(p.whole && self.vars[p.root].moved))
                            .collect();
                        if places.is_empty() {
                            return None;
                        }
                        let p = &places[self.g.src.below(places.len() as u32) as usize];
                        Some(Ex::new(format!("{}.take()", p.text), false))
                    }
                }
            }
            Ty::Result(t, e) => {
                let (t, e) = ((**t).clone(), (**e).clone());
                if self.g.src.chance(50) {
                    let o = self.expr(&Ty::Option(Box::new(t)), d);
                    let x = self.expr(&e, d);
                    Some(Ex::new(format!("{}.ok_or({})", recv(&o.s), x.s), false))
                } else {
                    let u = self.g.random_ty(1);
                    let r = self.expr(&Ty::Result(Box::new(u.clone()), Box::new(e)), d);
                    let x = self.fresh("x");
                    let body = self.pure_closure(&[x], &[u], &t, d);
                    Some(Ex::new(format!("{}.map({body})", recv(&r.s)), false))
                }
            }
            Ty::Array(elem, n) => {
                let (elem, n) = ((**elem).clone(), *n);
                if elem == Ty::Int(IntTy::U8)
                    && [1, 2, 4, 8, 16].contains(&n)
                    && self.g.src.chance(40)
                {
                    let i = match n {
                        1 => IntTy::U8,
                        2 => IntTy::I16,
                        4 => IntTy::U32,
                        8 => IntTy::I64,
                        _ => IntTy::U128,
                    };
                    let x = self.expr(&Ty::Int(i), d);
                    let m =
                        ["to_le_bytes", "to_be_bytes", "to_ne_bytes"][self.g.src.below(3) as usize];
                    return Some(Ex::new(format!("{}.{m}()", recv(&x.s)), false));
                }
                if self.g.cfg.weights.closures == 0 {
                    return None;
                }
                if self.g.src.chance(50) {
                    let i = self.fresh("i");
                    let body = self.pure_closure(&[i], &[Ty::Int(IntTy::Usize)], &elem, d);
                    Some(Ex::new(
                        format!("core::array::from_fn::<{}, {n}, _>({body})", elem.name()),
                        false,
                    ))
                } else {
                    let u = self.g.random_ty(1);
                    let a = self.expr(&Ty::Array(Box::new(u.clone()), n), d);
                    let x = self.fresh("x");
                    let body = self.pure_closure(&[x], &[u], &elem, d);
                    Some(Ex::new(format!("{}.map({body})", recv(&a.s)), false))
                }
            }
            Ty::Tuple(elems) if self.g.is_copy(ty) => {
                // The first part of a bigger tuple.
                let whole = Ty::Tuple(vec![Ty::Tuple(elems.clone()), Ty::Int(IntTy::U8)]);
                let e = self.expr(&whole, d);
                Some(Ex::new(format!("{}.0", recv(&e.s)), e.konst))
            }
            Ty::Struct(s) => {
                // An associated function returning it.
                let _ = s;
                None
            }
            Ty::Param => self.leaf(ty).into(),
            _ => None,
        }
    }

    // ----- control flow -----

    pub fn if_expr(&mut self, ty: &Ty, d: u32) -> Ex {
        let c = self.cond(d);
        let pad = self.pad();
        let arms = self.branches(2, |f, _| {
            f.indent += 1;
            let e = f.expr(ty, d);
            f.indent -= 1;
            e
        });
        let konst = arms.iter().any(|a| a.konst);
        Ex::new(
            format!(
                "if {c} {{\n{pad}    {}\n{pad}}} else {{\n{pad}    {}\n{pad}}}",
                arms[0].s, arms[1].s
            ),
            konst,
        )
    }

    fn block_expr(&mut self, ty: &Ty, _d: u32) -> Ex {
        let b = self.block(Some(ty));
        Ex::new(b, true)
    }

    fn match_value(&mut self, ty: &Ty, d: u32) -> Ex {
        let scrut = if self.g.src.chance(60) {
            Ty::Int(self.g.int_ty())
        } else {
            self.g.random_ty(2)
        };
        match self.match_expr(&scrut, ty, d) {
            Some(s) => Ex::new(s, true),
            None => self.leaf(ty),
        }
    }

    /// What to match on: a value, or a place by reference (bindings then
    /// borrow it). Returns the text, whether it is matched by reference,
    /// and the variable to keep borrowed during the arms.
    pub fn scrutinee(&mut self, ty: &Ty, d: u32) -> (String, bool, Option<usize>) {
        if !self.const_mode && self.g.src.chance(35) {
            let places = self.borrowable_places(ty);
            if !places.is_empty() {
                let p = places[self.g.src.below(places.len() as u32) as usize].clone();
                return (format!("&{}", p.text), true, Some(p.root));
            }
        }
        let e = self.expr(ty, d);
        (paren_if_brace(e.s), false, e.root)
    }

    /// `match scrutinee { arms }` producing `ty`.
    pub fn match_expr(&mut self, scrut_ty: &Ty, ty: &Ty, d: u32) -> Option<String> {
        let (scrut, by_ref, root) = self.scrutinee(scrut_ty, d);
        let mark = self.borrow_mark();
        if let Some(r) = root {
            self.borrow(r, false);
        }
        let n = self.g.src.range(1, 4) as usize;
        let pad = self.pad();
        let arms = self.branches(n + 1, |f, i| {
            let last = i == n;
            let pat = f.pattern(scrut_ty, by_ref, true, last);
            f.push_scope();
            for (name, t, m) in &pat.binds {
                f.declare(name, t.clone(), *m, true);
            }
            let guard = if !last && f.g.src.chance(30) {
                f.pure_depth += 1;
                let g = f.expr(&Ty::Bool, d.min(2));
                f.pure_depth -= 1;
                format!(" if {}", g.s)
            } else {
                String::new()
            };
            f.indent += 1;
            let body = if *ty == Ty::Unit {
                f.block(None)
            } else {
                f.expr(ty, d).s
            };
            f.indent -= 1;
            f.pop_scope();
            format!("{pad}    {}{guard} => {body},\n", pat.text)
        });
        self.release_borrows(mark);
        let mut s = format!("match {scrut} {{\n");
        for a in arms {
            s.push_str(&a);
        }
        let _ = write!(s, "{pad}}}");
        Some(s)
    }

    fn matches_expr(&mut self, d: u32) -> Ex {
        let ty = self.g.random_ty(2);
        let (scrut, by_ref, root) = self.scrutinee(&ty, d);
        let pat = self.pattern(&ty, by_ref, true, false);
        let mark = self.borrow_mark();
        if let Some(r) = root {
            self.borrow(r, false);
        }
        self.push_scope();
        for (name, t, m) in &pat.binds {
            self.declare(name, t.clone(), *m, true);
        }
        let guard = if !pat.binds.is_empty() && self.g.src.chance(50) {
            self.pure_depth += 1;
            let g = self.expr(&Ty::Bool, d.min(1));
            self.pure_depth -= 1;
            format!(" if {}", g.s)
        } else {
            String::new()
        };
        self.pop_scope();
        self.release_borrows(mark);
        let m = format!("matches!({scrut}, {}{guard})", pat.text);
        if self.g.src.chance(self.g.cfg.weights.bare_macros) {
            Ex::new(m, false)
        } else {
            Ex::new(format!("({m})"), false)
        }
    }

    fn loop_expr(&mut self, ty: &Ty, d: u32) -> Ex {
        // `{ let mut k = 0; loop { k += 1; if k > N { break v; } ... } }`
        let k = self.fresh("k");
        let trips = self.g.src.range(1, 4);
        let pad = self.pad();
        self.indent += 1;
        let (exit, body) = self.in_loop(trips as u64, None, Some(ty.clone()), |f| {
            let exit = f.no_jumps(|f| f.expr(ty, d));
            (exit, f.block(None))
        });
        self.indent -= 1;
        let body = body.replacen(
            "{\n",
            &format!("{{\n{pad}        {k} += 1;\n{pad}        if {k} > {trips} {{\n{pad}            break {};\n{pad}        }}\n", exit.s),
            1,
        );
        Ex::new(
            format!("{{\n{pad}    let mut {k} = 0u8;\n{pad}    loop {body}\n{pad}}}"),
            true,
        )
    }

    fn can_question(&self) -> bool {
        self.level == 0 && !self.const_mode && matches!(self.ret, Ty::Option(_) | Ty::Result(..))
    }

    /// `e?`, where `e` is an `Option` or `Result` as the function returns.
    fn question(&mut self, ty: &Ty, d: u32) -> Ex {
        if !self.can_question() {
            return self.leaf(ty);
        }
        let wrapped = match &self.ret {
            Ty::Option(_) => Ty::Option(Box::new(ty.clone())),
            Ty::Result(_, e) => Ty::Result(Box::new(ty.clone()), e.clone()),
            _ => unreachable!(),
        };
        let e = self.expr(&wrapped, d);
        Ex::new(format!("{}?", recv(&e.s)), false)
    }

    fn probe_expr(&mut self, ty: &Ty, d: u32) -> Ex {
        if !self.g.is_hashable(ty) {
            return self.leaf(ty);
        }
        let e = self.expr(ty, d);
        let tag = self.g.tag();
        let p = self.g.probe(ty);
        self.add_cost(1);
        Ex::new(format!("{p}({tag}, {})", e.s), false)
    }

    /// `ty` from an `Option<ty>` with a default.
    fn unwrap_or(&mut self, ty: &Ty, d: u32) -> Ex {
        let o = self.expr(&Ty::Option(Box::new(ty.clone())), d);
        if self.g.src.chance(30) && self.g.cfg.weights.closures > 0 {
            let x = self.fresh("x");
            let dflt = self.expr(ty, d);
            let body = self.pure_closure(&[x], std::slice::from_ref(ty), ty, d);
            return Ex::new(format!("{}.map_or({}, {body})", recv(&o.s), dflt.s), false);
        }
        let dflt = self.expr(ty, d);
        Ex::new(format!("{}.unwrap_or({})", recv(&o.s), dflt.s), false)
    }

    // ----- indexing, slices, iterators -----

    /// Arrays in places whose elements are `ty`.
    fn array_places(&self, elem: Option<&Ty>) -> Vec<super::func::Place> {
        let mut out = Vec::new();
        for v in 0..self.vars.len() {
            if !self.can_borrow(v) {
                continue;
            }
            let mut ps = Vec::new();
            self.places_of(v, &mut ps);
            for p in ps {
                if let Ty::Array(e, _) = &p.ty
                    && elem.is_none_or(|t| t == &**e)
                {
                    out.push(p);
                }
            }
        }
        out
    }

    fn index_expr(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        if !self.g.is_copy(ty) {
            return None;
        }
        let places: Vec<_> = self
            .array_places(Some(ty))
            .into_iter()
            .filter(|p| self.can_read(p.root))
            .collect();
        let (arr, n, root, konst) = if !places.is_empty() && self.g.src.chance(70) {
            let p = places[self.g.src.below(places.len() as u32) as usize].clone();
            let Ty::Array(_, n) = p.ty else {
                unreachable!()
            };
            let known = self.vars[p.root].known;
            (p.text, n, Some(p.root), known)
        } else {
            let n = self.g.src.range(1, 4) as usize;
            let a = self.expr(&Ty::Array(Box::new(ty.clone()), n), d);
            (recv(&a.s), n, a.root, a.konst)
        };
        let mark = self.borrow_mark();
        if let Some(r) = root {
            self.borrow(r, false);
        }
        let i = if self.g.src.chance(30) {
            Ex::new(format!("{}usize", self.g.src.below(n as u32)), true)
        } else {
            let t = self.g.int_ty();
            let e = self.expr(&Ty::Int(t), d);
            Ex::new(format!("(({} as usize) % {n})", e.s), e.konst)
        };
        self.release_borrows(mark);
        let text = format!("{arr}[{}]", i.s);
        Some(match root {
            // A place: borrowing it borrows the array.
            Some(r) => Ex::place(text, konst && i.konst, r),
            None => Ex::new(text, konst && i.konst),
        })
    }

    fn slice_len(&mut self, i: IntTy, _d: u32) -> Option<Ex> {
        let places = self.array_places(None);
        if places.is_empty() {
            return None;
        }
        let p = &places[self.g.src.below(places.len() as u32) as usize];
        let Ty::Array(_, n) = p.ty else {
            unreachable!()
        };
        let lo = self.g.src.below(n as u32 + 1);
        let hi = self.g.src.range(lo, n as u32);
        let s = match self.g.src.below(3) {
            0 => format!("{}.len()", p.text),
            1 => format!("{}[{lo}..{hi}].len()", p.text),
            _ => format!("{}[{lo}..].len()", p.text),
        };
        Some(Ex::new(format!("({s} as {})", i.name()), false))
    }

    fn slice_bool(&mut self, d: u32) -> Option<Ex> {
        let places = self.array_places(None);
        if places.is_empty() {
            return None;
        }
        let p = places[self.g.src.below(places.len() as u32) as usize].clone();
        let Ty::Array(elem, n) = p.ty.clone() else {
            unreachable!()
        };
        let lo = self.g.src.below(n as u32 + 1);
        let hi = self.g.src.range(lo, n as u32);
        let mark = self.borrow_mark();
        self.borrow(p.root, false);
        let s = if self.g.is_eq(&elem) && self.g.src.chance(70) {
            let e = self.expr(&elem, d);
            match self.g.src.below(3) {
                0 => format!("{}.contains(&{})", p.text, temp(&e)),
                1 => format!("{}[{lo}..{hi}].starts_with(&[{}])", p.text, e.s),
                _ => format!("{}[{lo}..].ends_with(&[{}])", p.text, e.s),
            }
        } else {
            format!("{}[{lo}..{hi}].is_empty()", p.text)
        };
        self.release_borrows(mark);
        Some(Ex::new(s, false))
    }

    /// A closure that may not change anything: `|a, b| { body }`, its
    /// parameters' types written or left to inference.
    pub fn pure_closure(&mut self, names: &[String], params: &[Ty], ret: &Ty, _d: u32) -> String {
        let annotate = !self.g.src.chance(self.g.cfg.weights.unannotated_closures);
        let header: Vec<String> = names
            .iter()
            .zip(params)
            .map(|(n, t)| {
                if annotate {
                    format!("{n}: {}", t.name())
                } else {
                    n.clone()
                }
            })
            .collect();
        let saved = self.mult;
        self.mult = self.mult.saturating_mul(4);
        let indent = self.indent;
        let body = self.closure_body(names, params, ret, false);
        self.indent = indent;
        self.mult = saved;
        format!("|{}| {body}", header.join(", "))
    }

    /// The start of an iterator chain.
    fn iter_source(&mut self, d: u32) -> Option<Chain> {
        match self.g.src.below(4) {
            0 => {
                let i = [IntTy::I32, IntTy::U8, IntTy::Usize, IntTy::I64, IntTy::U16]
                    [self.g.src.below(5) as usize];
                let lo = self.g.src.below(4);
                let hi = lo + self.g.src.below(7);
                let inclusive = self.g.src.chance(30);
                let text = if inclusive {
                    format!(
                        "({}..={})",
                        ty::int_literal(i, lo as u128),
                        ty::int_literal(i, hi as u128)
                    )
                } else {
                    format!(
                        "({}..{})",
                        ty::int_literal(i, lo as u128),
                        ty::int_literal(i, hi as u128)
                    )
                };
                let exact = if inclusive {
                    matches!(i, IntTy::U8 | IntTy::U16)
                } else {
                    !matches!(i, IntTy::I64)
                };
                Some(Chain {
                    text,
                    item: Ty::Int(i),
                    de: true,
                    exact,
                    simple_de: true,
                })
            }
            1 | 2 => {
                let places = self.array_places(None);
                if places.is_empty() {
                    return None;
                }
                let p = places[self.g.src.below(places.len() as u32) as usize].clone();
                let Ty::Array(elem, n) = p.ty.clone() else {
                    unreachable!()
                };
                self.borrow(p.root, false);
                let base = if self.g.cfg.weights.slices > 0 && self.g.src.chance(30) {
                    let lo = self.g.src.below(n as u32 + 1);
                    let hi = self.g.src.range(lo, n as u32);
                    format!("{}[{lo}..{hi}]", p.text)
                } else {
                    p.text.clone()
                };
                if self.g.is_copy(&elem) && self.g.src.chance(50) {
                    Some(Chain {
                        text: format!("{base}.iter().copied()"),
                        item: (*elem).clone(),
                        de: true,
                        exact: true,
                        simple_de: true,
                    })
                } else {
                    Some(Chain {
                        text: format!("{base}.iter()"),
                        item: Ty::Ref(elem),
                        de: true,
                        exact: true,
                        simple_de: true,
                    })
                }
            }
            _ => {
                let elem = self.g.random_ty(1);
                let n = self.g.src.range(1, 4) as usize;
                let a = self.expr(&Ty::Array(Box::new(elem.clone()), n), d);
                Some(Chain {
                    text: format!("{}.into_iter()", recv(&a.s)),
                    item: elem,
                    de: true,
                    exact: true,
                    simple_de: true,
                })
            }
        }
    }

    /// An iterator chain consumed into a value of `ty`.
    fn iter_expr(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        let mark = self.borrow_mark();
        self.pure_depth += 1;
        let r = self.iter_chain(ty, d);
        self.pure_depth -= 1;
        self.release_borrows(mark);
        r
    }

    fn iter_chain(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        let mut c = self.iter_source(d)?;
        let n = self.g.src.below(4);
        for _ in 0..n {
            match self.g.src.below(8) {
                0 | 1 => {
                    let u = Ty::Int(self.g.int_ty());
                    let x = self.fresh("x");
                    let body = self.pure_closure(&[x], &[c.item.clone()], &u, d);
                    c.text = format!("{}.map({body})", c.text);
                    c.item = u;
                }
                2 => {
                    let x = self.fresh("x");
                    let body =
                        self.pure_closure(&[x], &[Ty::Ref(Box::new(c.item.clone()))], &Ty::Bool, d);
                    c.text = format!("{}.filter({body})", c.text);
                    c.exact = false;
                }
                3 => {
                    c.text = format!("{}.enumerate()", c.text);
                    c.item = Ty::Tuple(vec![Ty::Int(IntTy::Usize), c.item.clone()]);
                    c.de &= c.exact;
                    c.simple_de = false;
                }
                4 if c.de
                    && (c.simple_de || self.g.src.chance(self.g.cfg.weights.rev_adapters)) =>
                {
                    c.text = format!("{}.rev()", c.text);
                }
                5 => {
                    let m = ["skip", "take", "step_by"][self.g.src.below(3) as usize];
                    let k = if m == "step_by" {
                        self.g.src.range(1, 3)
                    } else {
                        self.g.src.below(4)
                    };
                    c.text = format!("{}.{m}({k})", c.text);
                    c.de &= c.exact;
                    c.simple_de = false;
                }
                6 if self.g.src.chance(self.g.cfg.weights.zips) => {
                    let other = self.iter_source(d)?;
                    c.text = format!("{}.zip({})", c.text, other.text);
                    c.item = Ty::Tuple(vec![c.item.clone(), other.item]);
                    c.de = c.de && c.exact && other.de && other.exact;
                    c.simple_de = false;
                    c.exact &= other.exact;
                }
                _ => {
                    if let Ty::Int(i) = c.item {
                        let lo = self.g.src.below(4);
                        let hi = lo + self.g.src.below(4);
                        c.text = format!(
                            "{}.chain({}..{})",
                            c.text,
                            ty::int_literal(i, lo as u128),
                            ty::int_literal(i, hi as u128)
                        );
                        c.exact = false;
                    }
                }
            }
        }
        let item = c.item.clone();
        let text = c.text;
        let s = match ty {
            Ty::Int(i) => {
                let direct = item == *ty || item == Ty::Ref(Box::new(ty.clone()));
                match self.g.src.below(5) {
                    0 if direct => format!("{text}.sum::<{}>()", i.name()),
                    1 if *i == IntTy::Usize => format!("{text}.count()"),
                    2 if item == *ty => {
                        let m = ["max", "min", "last"][self.g.src.below(3) as usize];
                        let dflt = self.expr(ty, d);
                        format!("{text}.{m}().unwrap_or({})", dflt.s)
                    }
                    3 if *i == IntTy::Usize => {
                        let x = self.fresh("x");
                        let body = self.pure_closure(&[x], &[item], &Ty::Bool, d);
                        let dflt = self.g.src.below(9);
                        format!("{text}.position({body}).unwrap_or({dflt})")
                    }
                    _ => {
                        let acc = self.fresh("acc");
                        let x = self.fresh("x");
                        let init = self.expr(ty, d);
                        let body = self.pure_closure(
                            &[acc.clone(), x.clone()],
                            &[ty.clone(), item],
                            ty,
                            d,
                        );
                        format!("{text}.fold({}, {body})", init.s)
                    }
                }
            }
            Ty::Bool => {
                let x = self.fresh("x");
                let body = self.pure_closure(&[x], &[item], &Ty::Bool, d);
                let m = ["any", "all"][self.g.src.below(2) as usize];
                format!("{text}.{m}({body})")
            }
            _ => return None,
        };
        Some(Ex::new(s, false))
    }

    // ----- calls -----

    /// Arguments for parameters of types `params`, left to right: a
    /// reference borrows its place until the call ends.
    pub fn args(&mut self, params: &[Ty], d: u32) -> Vec<String> {
        let mut out = Vec::new();
        for ty in params {
            let s = match ty {
                Ty::Ref(t) => self.ref_arg(t, false, d),
                Ty::RefMut(t) => self.ref_arg(t, true, d),
                _ => self.expr(ty, d).s,
            };
            out.push(s);
        }
        out
    }

    fn ref_arg(&mut self, t: &Ty, mutable: bool, d: u32) -> String {
        if self.g.src.chance(70) {
            let mut places = if mutable {
                self.writable_places(Some(t))
            } else {
                self.borrowable_places(t)
            };
            places.retain(|p| !(p.whole && self.vars[p.root].moved));
            // An existing reference of the type, reborrowed.
            let want = if mutable {
                Ty::RefMut(Box::new(t.clone()))
            } else {
                Ty::Ref(Box::new(t.clone()))
            };
            for v in 0..self.vars.len() {
                if self.vars[v].ty == want
                    && if mutable {
                        self.can_write(v)
                    } else {
                        self.can_borrow(v)
                    }
                {
                    places.push(super::func::Place {
                        text: format!("*{}", self.vars[v].name),
                        ty: t.clone(),
                        root: v,
                        shared: false,
                        whole: false,
                    });
                }
            }
            if !places.is_empty() {
                let p = places[self.g.src.below(places.len() as u32) as usize].clone();
                self.borrow(p.root, mutable);
                return if mutable {
                    format!("&mut {}", p.text)
                } else {
                    format!("&{}", p.text)
                };
            }
        }
        let e = self.expr(t, d);
        if mutable {
            format!("&mut {}", temp(&e))
        } else {
            if let Some(r) = e.root {
                self.borrow(r, false);
            }
            format!("&{}", e.s)
        }
    }

    /// Functions that may be called here, returning `ty`.
    fn callables(&self, ty: Option<&Ty>) -> Vec<(FnSig, Option<Ty>)> {
        let mut out = Vec::new();
        for sig in self.g.fns.iter().take(self.callable_fns) {
            if ty.is_none_or(|t| &sig.ret == t) && self.affordable(sig.cost) {
                out.push((sig.clone(), None));
            }
        }
        if let Some(tr) = &self.g.tr {
            for sig in self.g.generic_fns.iter().take(self.callable_generic) {
                if !self.affordable(sig.cost) {
                    continue;
                }
                let mut with: Vec<Ty> = tr.impls.clone();
                if self.generic {
                    with.push(Ty::Param);
                }
                for w in with {
                    if ty.is_none_or(|t| &subst(&sig.ret, &w) == t) {
                        out.push((sig.clone(), Some(w)));
                    }
                }
            }
        }
        out
    }

    fn call_returning(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        let options = self.callables(Some(ty));
        if options.is_empty() {
            return None;
        }
        let (sig, with) = options[self.g.src.below(options.len() as u32) as usize].clone();
        Some(self.call_sig(&sig, with.as_ref(), d))
    }

    /// A call of any function: the call and the type it returns.
    pub fn any_call(&mut self, d: u32) -> Option<(Ex, Ty)> {
        let options = self.callables(None);
        if options.is_empty() {
            return None;
        }
        let (sig, with) = options[self.g.src.below(options.len() as u32) as usize].clone();
        let ret = match &with {
            Some(w) => subst(&sig.ret, w),
            None => sig.ret.clone(),
        };
        Some((self.call_sig(&sig, with.as_ref(), d), ret))
    }

    /// A call of `sig`; generic functions are called with `T` = `with`.
    pub fn call_sig(&mut self, sig: &FnSig, with: Option<&Ty>, d: u32) -> Ex {
        self.add_cost(sig.cost);
        let mark = self.borrow_mark();
        let s = match &sig.kind {
            FnKind::Free | FnKind::Const => {
                let args = self.args(&sig.params, d);
                format!("{}({})", sig.name, args.join(", "))
            }
            FnKind::Generic => {
                let w = with.cloned().unwrap_or(Ty::Param);
                let params: Vec<Ty> = sig.params.iter().map(|t| subst(t, &w)).collect();
                let args = self.args(&params, d);
                let turbofish = if w != Ty::Param && self.g.src.chance(30) {
                    format!("::<{}>", w.name())
                } else {
                    String::new()
                };
                format!("{}{turbofish}({})", sig.name, args.join(", "))
            }
            FnKind::Method {
                owner,
                recv: receiver,
            } => {
                let owner_ty = Ty::Struct(*owner);
                match receiver {
                    None => {
                        let args = self.args(&sig.params, d);
                        format!("S{owner}::{}({})", sig.name, args.join(", "))
                    }
                    Some(Recv::Ref) => {
                        let places = self.borrowable_places(&owner_ty);
                        let r = if !places.is_empty() && self.g.src.chance(60) {
                            let p = places[self.g.src.below(places.len() as u32) as usize].clone();
                            self.borrow(p.root, false);
                            p.text
                        } else {
                            let e = self.expr(&owner_ty, d);
                            if let Some(r) = e.root {
                                self.borrow(r, false);
                            }
                            recv(&e.s)
                        };
                        let args = self.args(&sig.params, d);
                        format!("{r}.{}({})", sig.name, args.join(", "))
                    }
                    Some(Recv::RefMut) => {
                        let places: Vec<_> = self
                            .writable_places(Some(&owner_ty))
                            .into_iter()
                            .filter(|p| !(p.whole && self.vars[p.root].moved))
                            .collect();
                        let r = if !places.is_empty() && self.g.src.chance(60) {
                            let p = places[self.g.src.below(places.len() as u32) as usize].clone();
                            self.borrow(p.root, true);
                            p.text
                        } else {
                            let e = self.expr(&owner_ty, d);
                            temp(&e)
                        };
                        let args = self.args(&sig.params, d);
                        format!("{r}.{}({})", sig.name, args.join(", "))
                    }
                    Some(Recv::Value) => {
                        let e = self.expr(&owner_ty, d);
                        let args = self.args(&sig.params, d);
                        format!("{}.{}({})", recv(&e.s), sig.name, args.join(", "))
                    }
                }
            }
        };
        self.release_borrows(mark);
        Ex::new(s, false)
    }

    fn const_call(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        let options: Vec<FnSig> = self
            .g
            .const_fns
            .iter()
            .take(self.callable_const)
            .filter(|f| &f.ret == ty)
            .cloned()
            .collect();
        if options.is_empty() {
            return None;
        }
        let sig = options[self.g.src.below(options.len() as u32) as usize].clone();
        Some(self.call_sig(&sig, None, d))
    }

    fn closure_call(&mut self, ty: &Ty, d: u32) -> Option<Ex> {
        let options: Vec<usize> = (0..self.vars.len())
            .filter(|&v| matches!(&self.vars[v].ty, Ty::Fn(_, r) if **r == *ty) && self.can_read(v))
            .collect();
        if options.is_empty() {
            return None;
        }
        let v = options[self.g.src.below(options.len() as u32) as usize];
        let Ty::Fn(params, _) = self.vars[v].ty.clone() else {
            unreachable!()
        };
        let name = self.vars[v].name.clone();
        let args = self.args(&params, d);
        Some(Ex::new(format!("{name}({})", args.join(", ")), false))
    }

    /// Integers from the trait: `x.val()`, `x.mix(k)`, `T::K`.
    fn trait_int(&mut self, i: IntTy, d: u32) -> Option<Ex> {
        let tr = self.g.tr.as_ref()?;
        let (k_ty, get_ty) = (tr.k_ty, tr.get_ty);
        let mut impls = tr.impls.clone();
        if self.generic {
            impls.push(Ty::Param);
        }
        if impls.is_empty() {
            return None;
        }
        let t = impls[self.g.src.below(impls.len() as u32) as usize].clone();
        let tname = if t == Ty::Param {
            "T".to_owned()
        } else {
            format!("<{} as Tr>", t.name())
        };
        if i == k_ty && self.g.src.chance(30) {
            return Some(Ex::new(format!("{tname}::K"), true));
        }
        if i != get_ty {
            return None;
        }
        // A receiver: a place of the type, through references too.
        let mut places = self.borrowable_places(&t);
        places.retain(|p| !(p.whole && self.vars[p.root].moved));
        let (r, root) = if !places.is_empty() && self.g.src.chance(70) {
            let p = places[self.g.src.below(places.len() as u32) as usize].clone();
            (p.text, Some(p.root))
        } else {
            let e = self.expr(&t, d);
            (recv(&e.s), e.root)
        };
        let mark = self.borrow_mark();
        if let Some(r) = root {
            self.borrow(r, false);
        }
        let s = if self.g.src.chance(30) {
            let k = self.expr(&Ty::Int(get_ty), d);
            format!("{r}.mix({})", k.s)
        } else {
            format!("{r}.val()")
        };
        self.release_borrows(mark);
        Some(Ex::new(s, false))
    }
}

/// A receiver for a method call: parenthesized unless it is a simple path,
/// call or literal.
pub fn recv(s: &str) -> String {
    let simple = s
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        || (s.ends_with(')') && s.starts_with(|c: char| c.is_alphabetic()) && balanced_call(s));
    if simple && !s.starts_with('-') {
        s.to_owned()
    } else {
        format!("({s})")
    }
}

/// `f(...)`: a name, then one parenthesized group to the end.
fn balanced_call(s: &str) -> bool {
    let Some(open) = s.find('(') else {
        return false;
    };
    if !s[..open]
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
    {
        return false;
    }
    let mut depth = 0;
    for (i, c) in s.char_indices().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i == s.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

/// A temporary holding `e`'s value: a place is copied (or moved) into a
/// block first, so borrowing it does not borrow the place.
pub fn temp(e: &Ex) -> String {
    let path_like =
        e.s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '[' | ']'));
    if e.root.is_some() || path_like {
        format!("{{ {} }}", e.s)
    } else {
        recv(&e.s)
    }
}
