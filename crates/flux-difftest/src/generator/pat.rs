//! Patterns, for `match`, `if let`, `let ... else` and `matches!`.

use super::func::FnGen;
use super::ty::{self, IntTy, Ty};

/// A pattern and the bindings it makes: name, type, `mut`.
pub struct Pat {
    pub text: String,
    pub binds: Vec<(String, Ty, bool)>,
}

/// Whether `|` appears outside all brackets.
fn is_top_level_or(text: &str) -> bool {
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'|' if depth == 0 && i > 0 && bytes[i - 1] == b' ' => return true,
            _ => {}
        }
    }
    false
}

/// Where in a pattern we are.
#[derive(Clone, Copy)]
struct Mode {
    /// Matching through a reference: bindings borrow, and `mut`, `ref`
    /// and `&` may not be written (edition 2024).
    by_ref: bool,
    /// Inside a struct with a `Drop` impl, whose fields may not be moved
    /// out.
    under_drop: bool,
    /// Bindings are allowed here.
    bind: bool,
    /// Must match every value.
    irrefutable: bool,
}

impl FnGen<'_> {
    /// A pattern for a value of `ty`. `by_ref` says the value is matched
    /// through a reference; `irrefutable` asks for one that matches
    /// everything.
    pub fn pattern(&mut self, ty: &Ty, by_ref: bool, bind: bool, irrefutable: bool) -> Pat {
        let mut binds = Vec::new();
        let mode = Mode {
            by_ref,
            under_drop: false,
            bind,
            irrefutable,
        };
        let text = self.pat(ty, mode, &mut binds, 3);
        // Top-level or-patterns and half-open ranges need parentheses in
        // `let` and in `matches!`'s `pat` fragment.
        let text = if is_top_level_or(&text) || text.starts_with("..") {
            if !text.starts_with("..") || self.g.src.chance(self.g.cfg.weights.paren_patterns) {
                format!("({text})")
            } else {
                // Without parentheses: a wildcard (such patterns bind nothing).
                binds.clear();
                "_".to_owned()
            }
        } else {
            text
        };
        Pat { text, binds }
    }

    fn pat(
        &mut self,
        ty: &Ty,
        mode: Mode,
        binds: &mut Vec<(String, Ty, bool)>,
        depth: u32,
    ) -> String {
        let copy = self.g.is_copy(ty);
        // A by-value binding may move a part out, which is fine unless
        // it is inside a `Drop` struct.
        let can_bind = mode.bind && (mode.by_ref || copy || !mode.under_drop);
        let structural = depth > 0
            && !matches!(
                ty,
                Ty::Array(..) | Ty::Param | Ty::Fn(..) | Ty::Ref(_) | Ty::RefMut(_)
            );
        let choice = self.g.src.weighted(&[
            3,
            if can_bind { 4 } else { 0 },
            if structural { 8 } else { 0 },
        ]);
        match choice {
            0 => "_".to_owned(),
            1 => self.binding(ty, mode, binds, depth),
            _ => self.structure(ty, mode, binds, depth),
        }
    }

    fn binding(
        &mut self,
        ty: &Ty,
        mode: Mode,
        binds: &mut Vec<(String, Ty, bool)>,
        depth: u32,
    ) -> String {
        let name = self.fresh("b");
        if mode.by_ref {
            binds.push((name.clone(), Ty::Ref(Box::new(ty.clone())), false));
            return name;
        }
        match self.g.src.below(5) {
            0 if !self.forbid_ref => {
                // `ref x` borrows even where moving is not allowed.
                binds.push((name.clone(), Ty::Ref(Box::new(ty.clone())), false));
                format!("ref {name}")
            }
            1 if self.g.is_copy(ty) && depth > 0 && !mode.irrefutable => {
                // `x @ subpattern`, with no bindings in the subpattern.
                let sub = Mode {
                    bind: false,
                    ..mode
                };
                let mut none = Vec::new();
                let p = self.structure(ty, sub, &mut none, depth);
                binds.push((name.clone(), ty.clone(), false));
                if is_top_level_or(&p) || p.starts_with("..") {
                    if !p.starts_with("..") || self.g.src.chance(self.g.cfg.weights.paren_patterns)
                    {
                        format!("{name} @ ({p})")
                    } else {
                        name
                    }
                } else {
                    format!("{name} @ {p}")
                }
            }
            2 => {
                binds.push((name.clone(), ty.clone(), true));
                format!("mut {name}")
            }
            _ => {
                binds.push((name.clone(), ty.clone(), false));
                name
            }
        }
    }

    fn structure(
        &mut self,
        ty: &Ty,
        mode: Mode,
        binds: &mut Vec<(String, Ty, bool)>,
        depth: u32,
    ) -> String {
        let d = depth.saturating_sub(1);
        match ty {
            Ty::Int(i) => {
                if mode.irrefutable {
                    return "_".to_owned();
                }
                self.int_pat(*i, mode.by_ref)
            }
            Ty::Bool => {
                if mode.irrefutable {
                    "true | false".to_owned()
                } else {
                    ["true", "false"][self.g.src.below(2) as usize].to_owned()
                }
            }
            Ty::Char => {
                if mode.irrefutable {
                    return "_".to_owned();
                }
                let (a, at) = ty::char_literal(&mut self.g.src);
                if self.g.src.chance(40) {
                    let (b, bt) = ty::char_literal(&mut self.g.src);
                    let (lo, hi) = if a <= b { (at, bt) } else { (bt, at) };
                    format!("{lo}..={hi}")
                } else {
                    at
                }
            }
            Ty::Unit => "()".to_owned(),
            Ty::Tuple(elems) => {
                let k = elems.len();
                if self.g.src.chance(20) && k > 1 {
                    // `(p, ..)`
                    let keep = self.g.src.range(1, k as u32 - 1) as usize;
                    let ps: Vec<String> = elems[..keep]
                        .iter()
                        .map(|t| self.pat(t, mode, binds, d))
                        .collect();
                    return format!("({}, ..)", ps.join(", "));
                }
                let ps: Vec<String> = elems.iter().map(|t| self.pat(t, mode, binds, d)).collect();
                if k == 1 {
                    format!("({},)", ps[0])
                } else {
                    format!("({})", ps.join(", "))
                }
            }
            Ty::Struct(s) => {
                let s = *s;
                let fields = self.g.structs[s].fields.clone();
                let tuple = self.g.structs[s].tuple;
                let sub = Mode {
                    under_drop: mode.under_drop || self.g.structs[s].drop,
                    ..mode
                };
                let mut parts = Vec::new();
                let mut rest = false;
                for (i, t) in fields.iter().enumerate() {
                    if !tuple && self.g.src.chance(25) {
                        rest = true;
                        continue;
                    }
                    let p = self.pat(t, sub, binds, d);
                    if tuple {
                        parts.push(p);
                    } else {
                        parts.push(format!("{}: {p}", self.g.structs[s].field_name(i)));
                    }
                }
                if tuple {
                    format!("S{s}({})", parts.join(", "))
                } else if rest {
                    if parts.is_empty() {
                        format!("S{s} {{ .. }}")
                    } else {
                        format!("S{s} {{ {}, .. }}", parts.join(", "))
                    }
                } else {
                    format!("S{s} {{ {} }}", parts.join(", "))
                }
            }
            Ty::Enum(e) => {
                let e = *e;
                let n = self.g.enums[e].variants.len();
                if mode.irrefutable && n > 1 {
                    return "_".to_owned();
                }
                if !mode.irrefutable && n > 1 && self.g.src.chance(20) {
                    // An or-pattern of whole variants, binding nothing.
                    let a = self.g.src.below(n as u32) as usize;
                    let b = self.g.src.below(n as u32) as usize;
                    return format!("{} | {}", self.variant_any(e, a), self.variant_any(e, b));
                }
                let k = self.g.src.below(n as u32) as usize;
                let fields = self.g.enums[e].variants[k].fields.clone();
                let named = self.g.enums[e].variants[k].named;
                if fields.is_empty() {
                    return format!("E{e}::V{k}");
                }
                if self.g.src.chance(15) {
                    return self.variant_any(e, k);
                }
                let ps: Vec<String> = fields.iter().map(|t| self.pat(t, mode, binds, d)).collect();
                if named {
                    let fs: Vec<String> = ps
                        .iter()
                        .enumerate()
                        .map(|(i, p)| format!("{}: {p}", ["a", "b", "c", "d"][i]))
                        .collect();
                    format!("E{e}::V{k} {{ {} }}", fs.join(", "))
                } else {
                    format!("E{e}::V{k}({})", ps.join(", "))
                }
            }
            Ty::Option(t) => {
                if mode.irrefutable {
                    return "_".to_owned();
                }
                if self.g.src.chance(30) {
                    "None".to_owned()
                } else {
                    format!("Some({})", self.pat(t, mode, binds, d))
                }
            }
            Ty::Result(t, e) => {
                if mode.irrefutable {
                    return "_".to_owned();
                }
                if self.g.src.chance(40) {
                    format!("Err({})", self.pat(e, mode, binds, d))
                } else {
                    format!("Ok({})", self.pat(t, mode, binds, d))
                }
            }
            _ => "_".to_owned(),
        }
    }

    /// `E::V(..)`, `E::V { .. }` or `E::V`: any value of a variant.
    fn variant_any(&self, e: usize, k: usize) -> String {
        let v = &self.g.enums[e].variants[k];
        if v.fields.is_empty() {
            format!("E{e}::V{k}")
        } else if v.named {
            format!("E{e}::V{k} {{ .. }}")
        } else {
            format!("E{e}::V{k}(..)")
        }
    }

    fn int_pat(&mut self, i: IntTy, by_ref: bool) -> String {
        // Through a reference, a constant's path is not a pattern rustc
        // derefs to; a number is.
        let lit = |v: u128| {
            if by_ref {
                ty::int_numeric_literal(i, v)
            } else {
                ty::int_pattern_literal(i, v)
            }
        };
        let a = ty::int_value(&mut self.g.src, i);
        match self.g.src.below(6) {
            0 | 1 => lit(a),
            2 | 3 => {
                let b = ty::int_value(&mut self.g.src, i);
                let (lo, hi) = if i.cmp(a, b).is_le() { (a, b) } else { (b, a) };
                if i.cmp(lo, hi).is_lt() && self.g.src.chance(40) {
                    format!("{}..{}", lit(lo), lit(hi))
                } else {
                    format!("{}..={}", lit(lo), lit(hi))
                }
            }
            4 => {
                if self.g.src.chance(50) {
                    format!("{}..", lit(a))
                } else {
                    format!("..={}", lit(a))
                }
            }
            _ => {
                let b = ty::int_value(&mut self.g.src, i);
                format!("{} | {}", lit(a), lit(b))
            }
        }
    }
}
