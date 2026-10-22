//! Checks of a body's patterns once its types are known, as rustc's
//! `check_match` does: `match`es must be exhaustive (E0004), `let` and
//! parameter patterns irrefutable (E0005), range patterns non-empty
//! (E0030, E0579), and binding modifiers must not be written where a
//! pattern borrows implicitly (edition 2024).
//!
//! Exhaustiveness is the usefulness algorithm of Maranget's "Warnings
//! for pattern matching", as rustc implements it: a match is exhaustive
//! if a wildcard is not useful after its arms. Patterns are seen as
//! constructors applied to fields: an enum's variants, `true` and
//! `false`, one constructor for a struct, tuple or reference, and ranges
//! for the integers, split at the bounds the patterns of a column use.
//! What is not covered is reported as rustc does, by witnesses: patterns
//! for values no arm matches (`E::B(_)`, `6_u8..=u8::MAX`).

use flux_arena::ArenaMap;
use flux_hir::{
    Body, BodySourceMap, Diagnostic, Expr, ExprId, ItemTree, Literal, Pat, PatId, Stmt, StructKind,
    UnaryOp,
};
use flux_syntax::TextRange;

use crate::Signatures;
use crate::ty::{IntTy, Mutability, Ty, TyKind};

/// The pattern checks of a body that type-checked without errors.
/// `consts` are the values of the paths to constants in its patterns
/// (`1` for `true`), and `mode_errors` the binding modifiers written
/// where a pattern borrows implicitly, found during inference.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_body(
    items: &ItemTree,
    sigs: &Signatures,
    body: &Body,
    source_map: &BodySourceMap,
    pat_types: &ArenaMap<Pat, Ty>,
    pat_derefs: &ArenaMap<Pat, u32>,
    expr_types: &ArenaMap<Expr, Ty>,
    consts: &ArenaMap<Expr, i128>,
    mode_errors: &[(TextRange, &'static str)],
) -> Vec<Diagnostic> {
    let mut cx = Cx {
        items,
        sigs,
        body,
        pat_types,
        pat_derefs,
        consts,
        diagnostics: mode_errors
            .iter()
            .map(|&(range, message)| Diagnostic::new(range, message))
            .collect(),
    };
    let pat_range = |pat: PatId| source_map.pats.get(pat).copied().unwrap_or_default();
    for (pat, data) in body.pats.iter() {
        if let Pat::Range {
            start: Some(start),
            end: Some(end),
            inclusive,
        } = *data
            && let Some(int) = pat_types.get(pat).and_then(|t| t.as_int())
            && let (Some(lo), Some(hi)) = (cx.literal(start), cx.literal(end))
        {
            let domain = IntDomain { int };
            let (lo, hi) = (domain.encode(lo), domain.encode(hi));
            if inclusive && lo > hi {
                cx.error(
                    pat_range(pat),
                    "lower bound for range pattern must be less than or equal to upper bound",
                );
            } else if !inclusive && lo >= hi {
                cx.error(
                    pat_range(pat),
                    "lower bound for range pattern must be less than upper bound",
                );
            }
        }
    }
    // Function parameters, then `let`s and `match`es, in order.
    for &param in body.params.iter() {
        if let Some(witnesses) = cx.refutable(param) {
            let message = format!(
                "refutable pattern in function argument: {}",
                not_covered(&witnesses)
            );
            cx.error(pat_range(param), message);
        }
    }
    for data in body.exprs.values() {
        match data {
            Expr::Block { stmts, .. } => {
                for stmt in stmts.iter() {
                    if let Stmt::Let {
                        pat,
                        else_branch: None,
                        ..
                    } = *stmt
                        && let Some(witnesses) = cx.refutable(pat)
                    {
                        let message = format!(
                            "refutable pattern in local binding: {}",
                            not_covered(&witnesses)
                        );
                        cx.error(pat_range(pat), message);
                    }
                }
            }
            Expr::Match { scrutinee, arms } => {
                let range = source_map
                    .exprs
                    .get(*scrutinee)
                    .copied()
                    .unwrap_or_default();
                let ty = expr_types.get(*scrutinee).copied().unwrap_or(Ty::ERROR);
                if has_error(ty) {
                    continue;
                }
                let rows: Vec<Vec<DPat>> = arms
                    .iter()
                    .filter(|arm| arm.guard.is_none())
                    .map(|arm| vec![cx.lower(arm.pat)])
                    .collect();
                // An empty `match` on a type other than an enum says so.
                let is_enum = matches!(ty.kind(), TyKind::Adt(adt, _) if sigs.is_enum(adt.strukt));
                if arms.is_empty() && !is_enum && !cx.uninhabited(ty, 0) {
                    cx.error(
                        range,
                        format!("non-exhaustive patterns: type `{ty}` is non-empty"),
                    );
                    continue;
                }
                let witnesses = cx.witnesses(rows, &[ty], true);
                if !witnesses.is_empty() {
                    let shown: Vec<String> = witnesses.iter().map(|w| cx.show(&w[0])).collect();
                    let message = format!(
                        "non-exhaustive patterns: {} not covered",
                        join_witnesses(&shown)
                    );
                    cx.error(range, message);
                }
            }
            _ => {}
        }
    }
    cx.diagnostics
}

/// "pattern `x` not covered", or "patterns `x` and `y` not covered".
fn not_covered(witnesses: &[String]) -> String {
    let s = if witnesses.len() == 1 { "" } else { "s" };
    format!("pattern{s} {} not covered", join_witnesses(witnesses))
}

/// "`a`", "`a` and `b`", "`a`, `b` and `c`", or "`a`, `b`, `c` and 2
/// more", as rustc lists what is not covered.
fn join_witnesses(witnesses: &[String]) -> String {
    let quoted: Vec<String> = witnesses.iter().map(|w| format!("`{w}`")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., tail] if head.len() < 3 => format!("{} and {tail}", head.join(", ")),
        _ => format!("{} and {} more", quoted[..3].join(", "), quoted.len() - 3),
    }
}

fn has_error(ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Error | TyKind::Infer(_) => true,
        TyKind::Tuple(elements) | TyKind::Adt(_, elements) => {
            elements.iter().any(|&e| has_error(e))
        }
        TyKind::Ref(_, pointee)
        | TyKind::RawPtr(_, pointee)
        | TyKind::Array(pointee, _)
        | TyKind::Slice(pointee) => has_error(pointee),
        _ => false,
    }
}

/// A constructor: what a pattern that is not a wildcard says about the
/// value it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctor {
    /// A struct's, a tuple's or a reference's only one.
    Single,
    Variant(u32),
    Bool(bool),
    /// The integers in an inclusive range, encoded by [`IntDomain`].
    Range(u128, u128),
}

/// A pattern as constructors and fields.
#[derive(Debug, Clone)]
enum DPat {
    Wild,
    Ctor(Ctor, Vec<DPat>),
    Or(Vec<DPat>),
}

/// A witness: a pattern for values no arm matches.
#[derive(Debug, Clone)]
enum WPat {
    Wild,
    Ctor(Ty, Ctor, Vec<WPat>),
}

struct Cx<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    body: &'a Body,
    pat_types: &'a ArenaMap<Pat, Ty>,
    pat_derefs: &'a ArenaMap<Pat, u32>,
    consts: &'a ArenaMap<Expr, i128>,
    diagnostics: Vec<Diagnostic>,
}

/// The integers of a type as `u128`s that sort as they do. `isize` and
/// `usize` are as wide as a pointer, which may be wider than flux's
/// targets, so as rustc does, matching them exhaustively takes ranges
/// open at the end (`0..`): their values have an infinity beyond the
/// maximum, and below the minimum for `isize`.
#[derive(Clone, Copy)]
struct IntDomain {
    int: IntTy,
}

impl IntDomain {
    fn min(self) -> u128 {
        0
    }

    fn max(self) -> u128 {
        match self.int {
            IntTy::Usize => 1 << 64,
            IntTy::Isize => (1 << 64) + 1,
            int if int.bits() == 128 => u128::MAX,
            int => (1u128 << int.bits()) - 1,
        }
    }

    fn encode(self, value: i128) -> u128 {
        match self.int {
            IntTy::Usize => value as u128,
            IntTy::Isize => (value + (1 << 63) + 1) as u128,
            IntTy::I128 => (value as u128) ^ (1 << 127),
            IntTy::U128 => value as u128,
            int if int.is_signed() => (value + (1i128 << (int.bits() - 1))) as u128,
            _ => value as u128,
        }
    }

    /// The value `encoded` stands for, `None` for an infinity.
    fn decode(self, encoded: u128) -> Option<i128> {
        match self.int {
            IntTy::Usize if encoded == 1 << 64 => None,
            IntTy::Usize => Some(encoded as i128),
            IntTy::Isize if encoded == 0 || encoded == (1 << 64) + 1 => None,
            IntTy::Isize => Some(encoded as i128 - (1 << 63) - 1),
            IntTy::I128 => Some((encoded ^ (1 << 127)) as i128),
            IntTy::U128 => Some(encoded as i128),
            int if int.is_signed() => Some(encoded as i128 - (1i128 << (int.bits() - 1))),
            _ => Some(encoded as i128),
        }
    }

    /// The lowest and highest values of the type, encoded: no infinity.
    fn finite(self) -> (u128, u128) {
        match self.int {
            IntTy::Usize => (0, (1 << 64) - 1),
            IntTy::Isize => (1, 1 << 64),
            _ => (self.min(), self.max()),
        }
    }
}

impl Cx<'_> {
    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(range, message));
    }

    /// The witnesses that `pat`, whose type is known, is refutable.
    fn refutable(&mut self, pat: PatId) -> Option<Vec<String>> {
        let ty = self.pat_type(pat);
        if has_error(ty) {
            return None;
        }
        let row = vec![self.lower(pat)];
        let witnesses = self.witnesses(vec![row], &[ty], true);
        (!witnesses.is_empty()).then(|| witnesses.iter().map(|w| self.show(&w[0])).collect())
    }

    /// The type of the value `pat` matches, before the references it
    /// sees through.
    fn pat_type(&self, pat: PatId) -> Ty {
        let mut ty = self.pat_types.get(pat).copied().unwrap_or(Ty::ERROR);
        // The references seen through are shared or not; either suits
        // the analysis, which does not look at mutability.
        for _ in 0..self.pat_derefs.get(pat).copied().unwrap_or(0) {
            ty = Ty::new(TyKind::Ref(Mutability::Not, ty));
        }
        ty
    }

    /// The value of `expr`, a literal or constant in a pattern, of an
    /// integer type.
    fn literal(&self, expr: ExprId) -> Option<i128> {
        if let Some(&value) = self.consts.get(expr) {
            return Some(value);
        }
        match &self.body.exprs[expr] {
            // A `u128` above `i128::MAX` reads as negative, which keeps
            // its bits for `IntDomain::encode`; out of range literals
            // were reported.
            Expr::Literal(Literal::Int { value, .. }) => Some(*value as i128),
            Expr::Unary {
                op: UnaryOp::Neg,
                operand,
            } => match &self.body.exprs[*operand] {
                Expr::Literal(Literal::Int { value, .. }) => Some((*value as i128).wrapping_neg()),
                _ => None,
            },
            _ => None,
        }
    }

    /// `pat` as constructors and fields.
    fn lower(&self, pat: PatId) -> DPat {
        let ty = self.pat_types.get(pat).copied().unwrap_or(Ty::ERROR);
        let inner = match &self.body.pats[pat] {
            Pat::Missing | Pat::Wild | Pat::Bind { sub: None, .. } => DPat::Wild,
            Pat::Bind { sub: Some(sub), .. } => self.lower(*sub),
            Pat::Tuple { pats, rest } => {
                let count = match ty.kind() {
                    TyKind::Tuple(elements) => elements.len(),
                    _ => pats.len(),
                };
                DPat::Ctor(Ctor::Single, self.lower_fields(pats, *rest, count))
            }
            Pat::TupleStruct {
                strukt,
                variant,
                pats,
                rest,
            } => {
                let count = self.items.structs[*strukt].variant(*variant).fields.len();
                let ctor = self.adt_ctor(*strukt, *variant);
                DPat::Ctor(ctor, self.lower_fields(pats, *rest, count))
            }
            Pat::Record {
                strukt,
                variant,
                fields,
                ..
            } => {
                let def = self.items.structs[*strukt].variant(*variant);
                let mut lowered = vec![DPat::Wild; def.fields.len()];
                for &(name, field) in fields.iter() {
                    if let Some((i, _)) = def.field(name) {
                        lowered[i] = self.lower(field);
                    }
                }
                DPat::Ctor(self.adt_ctor(*strukt, *variant), lowered)
            }
            Pat::Unit(strukt, variant) => DPat::Ctor(self.adt_ctor(*strukt, *variant), Vec::new()),
            Pat::Expr(expr) => match (&self.body.exprs[*expr], ty.kind()) {
                (Expr::Literal(Literal::Bool(b)), _) => DPat::Ctor(Ctor::Bool(*b), Vec::new()),
                (_, TyKind::Bool) => match self.literal(*expr) {
                    Some(value) => DPat::Ctor(Ctor::Bool(value != 0), Vec::new()),
                    None => DPat::Wild,
                },
                (_, TyKind::Int(int)) => match self.literal(*expr) {
                    Some(value) => {
                        let at = IntDomain { int }.encode(value);
                        DPat::Ctor(Ctor::Range(at, at), Vec::new())
                    }
                    None => DPat::Wild,
                },
                _ => DPat::Wild,
            },
            Pat::Range {
                start,
                end,
                inclusive,
            } => match ty.kind() {
                TyKind::Int(int) => {
                    let domain = IntDomain { int };
                    let value = |e: &Option<ExprId>| e.and_then(|e| self.literal(e));
                    let lo = value(start).map_or(domain.min(), |v| domain.encode(v));
                    let hi = match value(end) {
                        Some(v) if *inclusive => Some(domain.encode(v)),
                        Some(v) => domain.encode(v).checked_sub(1),
                        None => Some(domain.max()),
                    };
                    match hi {
                        Some(hi) if lo <= hi => DPat::Ctor(Ctor::Range(lo, hi), Vec::new()),
                        // Empty, and reported: matches nothing.
                        _ => DPat::Or(Vec::new()),
                    }
                }
                _ => DPat::Wild,
            },
            Pat::Or(alternatives) => {
                DPat::Or(alternatives.iter().map(|&a| self.lower(a)).collect())
            }
            Pat::Ref { pat: inner, .. } => DPat::Ctor(Ctor::Single, vec![self.lower(*inner)]),
        };
        let derefs = self.pat_derefs.get(pat).copied().unwrap_or(0);
        (0..derefs).fold(inner, |p, _| DPat::Ctor(Ctor::Single, vec![p]))
    }

    fn adt_ctor(&self, strukt: flux_hir::StructId, variant: u32) -> Ctor {
        if self.items.structs[strukt].is_enum() {
            Ctor::Variant(variant)
        } else {
            Ctor::Single
        }
    }

    /// The fields of a tuple or tuple struct pattern, `count` of them,
    /// with those `..` stands for wildcards.
    fn lower_fields(&self, pats: &[PatId], rest: Option<u32>, count: usize) -> Vec<DPat> {
        let mut out = vec![DPat::Wild; count];
        for (i, &pat) in pats.iter().enumerate() {
            let index = match rest {
                Some(rest) if i >= rest as usize => count - (pats.len() - i),
                _ => i,
            };
            if let Some(slot) = out.get_mut(index) {
                *slot = self.lower(pat);
            }
        }
        out
    }

    /// Whether no value has type `ty`: `!`, an enum without variants, or
    /// one whose every variant has an uninhabited field, a struct or
    /// tuple with one.
    fn uninhabited(&self, ty: Ty, depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        match ty.kind() {
            TyKind::Never => true,
            TyKind::Tuple(elements) => elements.iter().any(|&e| self.uninhabited(e, depth + 1)),
            TyKind::Adt(..) => match self.sigs.variants(ty) {
                Some(variants) => variants
                    .iter()
                    .all(|fields| fields.iter().any(|&f| self.uninhabited(f, depth + 1))),
                None => self
                    .sigs
                    .fields(ty)
                    .is_some_and(|fields| fields.iter().any(|&f| self.uninhabited(f, depth + 1))),
            },
            _ => false,
        }
    }

    /// The types of the fields of `ctor` for a value of `ty`.
    fn ctor_fields(&self, ty: Ty, ctor: Ctor) -> Vec<Ty> {
        match (ctor, ty.kind()) {
            (Ctor::Single, TyKind::Ref(_, pointee)) => vec![pointee],
            (Ctor::Single, _) => self.sigs.fields(ty).map_or(Vec::new(), |f| f.to_vec()),
            (Ctor::Variant(v), _) => self
                .sigs
                .variant_fields(ty, v)
                .map_or(Vec::new(), |f| f.to_vec()),
            (Ctor::Bool(_) | Ctor::Range(..), _) => Vec::new(),
        }
    }

    /// The constructors of `ty` the patterns of a column, `heads`, use,
    /// split so that each is wholly inside or outside each pattern's,
    /// and those missing. `None` for types whose values have no
    /// constructors to match (type parameters, pointers): only wildcards
    /// cover them.
    fn split(&self, ty: Ty, heads: &[&DPat], at_scrutinee: bool) -> (Vec<Ctor>, Vec<Ctor>) {
        let used = |ctor: Ctor| {
            heads
                .iter()
                .any(|h| matches!(h, DPat::Ctor(c, _) if *c == ctor))
        };
        match ty.kind() {
            TyKind::Bool => [false, true]
                .into_iter()
                .map(Ctor::Bool)
                .partition(|&c| used(c)),
            TyKind::Int(int) => self.split_ints(IntDomain { int }, heads),
            TyKind::Tuple(_) | TyKind::Ref(..) => {
                if heads.is_empty() {
                    (Vec::new(), vec![Ctor::Single])
                } else {
                    (vec![Ctor::Single], Vec::new())
                }
            }
            TyKind::Adt(adt, _) if self.sigs.is_enum(adt.strukt) => {
                let count = self.items.structs[adt.strukt].variant_count() as u32;
                let mut present = Vec::new();
                let mut missing = Vec::new();
                for v in 0..count {
                    let ctor = Ctor::Variant(v);
                    if used(ctor) {
                        present.push(ctor);
                        continue;
                    }
                    // A variant no value can have need not be matched
                    // where the scrutinee is matched by value.
                    let fields = self.ctor_fields(ty, ctor);
                    if at_scrutinee && fields.iter().any(|&f| self.uninhabited(f, 0)) {
                        continue;
                    }
                    missing.push(ctor);
                }
                (present, missing)
            }
            TyKind::Adt(..) => {
                if heads.is_empty() {
                    (Vec::new(), vec![Ctor::Single])
                } else {
                    (vec![Ctor::Single], Vec::new())
                }
            }
            TyKind::Never => (Vec::new(), Vec::new()),
            // Only a wildcard matches these: a missing constructor that no
            // pattern can name.
            _ => (Vec::new(), vec![Ctor::Range(1, 0)]),
        }
    }

    /// The integers split at the bounds the patterns use.
    fn split_ints(&self, domain: IntDomain, heads: &[&DPat]) -> (Vec<Ctor>, Vec<Ctor>) {
        let ranges: Vec<(u128, u128)> = heads
            .iter()
            .filter_map(|h| match h {
                DPat::Ctor(Ctor::Range(lo, hi), _) => Some((*lo, *hi)),
                _ => None,
            })
            .collect();
        let (min, max) = (domain.min(), domain.max());
        let mut cuts = vec![min];
        for &(lo, hi) in &ranges {
            cuts.push(lo);
            if hi < max {
                cuts.push(hi + 1);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut present = Vec::new();
        let mut missing = Vec::new();
        for (i, &lo) in cuts.iter().enumerate() {
            let hi = cuts.get(i + 1).map_or(max, |&next| next - 1);
            let ctor = Ctor::Range(lo, hi);
            if ranges.iter().any(|&(a, b)| a <= lo && hi <= b) {
                present.push(ctor);
            } else {
                missing.push(ctor);
            }
        }
        (present, missing)
    }

    /// The witnesses that `rows`, each a list of patterns for values of
    /// `tys`, do not cover every value: none if they do.
    fn witnesses(&self, rows: Vec<Vec<DPat>>, tys: &[Ty], at_scrutinee: bool) -> Vec<Vec<WPat>> {
        let Some((&ty, rest_tys)) = tys.split_first() else {
            return if rows.is_empty() {
                vec![Vec::new()]
            } else {
                Vec::new()
            };
        };
        // An or-pattern at the head is a row per alternative.
        let mut expanded = Vec::with_capacity(rows.len());
        let mut stack: Vec<Vec<DPat>> = rows.into_iter().rev().collect();
        while let Some(row) = stack.pop() {
            match &row[0] {
                DPat::Or(alternatives) => {
                    for alt in alternatives.iter().rev() {
                        let mut new = row.clone();
                        new[0] = alt.clone();
                        stack.push(new);
                    }
                }
                _ => expanded.push(row),
            }
        }
        let rows = expanded;
        let heads: Vec<&DPat> = rows
            .iter()
            .map(|r| &r[0])
            .filter(|p| !matches!(p, DPat::Wild))
            .collect();
        let (present, missing) = self.split(ty, &heads, at_scrutinee);
        let mut out = Vec::new();
        if missing.is_empty() {
            for ctor in present {
                let fields = self.ctor_fields(ty, ctor);
                let arity = fields.len();
                let specialized: Vec<Vec<DPat>> = rows
                    .iter()
                    .filter_map(|row| specialize(row, ctor, arity))
                    .collect();
                let mut sub_tys = fields;
                sub_tys.extend_from_slice(rest_tys);
                for mut w in self.witnesses(specialized, &sub_tys, false) {
                    let rest = w.split_off(arity);
                    let mut applied = vec![WPat::Ctor(ty, ctor, w)];
                    applied.extend(rest);
                    out.push(applied);
                }
            }
            return out;
        }
        // Only what no pattern's constructor covers can be a witness:
        // the rows that match it are those with a wildcard.
        let defaults: Vec<Vec<DPat>> = rows
            .iter()
            .filter(|row| matches!(row[0], DPat::Wild))
            .map(|row| row[1..].to_vec())
            .collect();
        let is_int = matches!(ty.kind(), TyKind::Int(_));
        let individually = (at_scrutinee && !is_int) || !present.is_empty();
        for w in self.witnesses(defaults, rest_tys, false) {
            if individually {
                for &ctor in &missing {
                    let fields = self.ctor_fields(ty, ctor).len();
                    let mut applied = vec![WPat::Ctor(ty, ctor, vec![WPat::Wild; fields])];
                    applied.extend(w.iter().cloned());
                    out.push(applied);
                }
            } else {
                let mut applied = vec![WPat::Wild];
                applied.extend(w);
                out.push(applied);
            }
        }
        out
    }

    /// A witness as rustc writes it.
    fn show(&self, w: &WPat) -> String {
        let WPat::Ctor(ty, ctor, fields) = w else {
            return "_".to_owned();
        };
        let show_all =
            |fields: &[WPat]| -> Vec<String> { fields.iter().map(|f| self.show(f)).collect() };
        match (*ctor, ty.kind()) {
            (Ctor::Bool(b), _) => b.to_string(),
            (Ctor::Range(lo, hi), TyKind::Int(int)) if lo <= hi => {
                show_range(IntDomain { int }, lo, hi)
            }
            (Ctor::Range(..), _) => "_".to_owned(),
            (Ctor::Single, TyKind::Ref(Mutability::Mut, _)) => {
                format!("&mut {}", self.show(&fields[0]))
            }
            (Ctor::Single, TyKind::Ref(..)) => format!("&{}", self.show(&fields[0])),
            (Ctor::Single, TyKind::Tuple(_)) => {
                let shown = show_all(fields);
                if shown.len() == 1 {
                    format!("({},)", shown[0])
                } else {
                    format!("({})", shown.join(", "))
                }
            }
            (Ctor::Single | Ctor::Variant(_), TyKind::Adt(adt, _)) => {
                let variant = match *ctor {
                    Ctor::Variant(v) => v,
                    _ => 0,
                };
                let data = self.items.structs[adt.strukt].variant(variant);
                let name = self.variant_name(adt.strukt, variant);
                match data.kind {
                    StructKind::Unit => name,
                    StructKind::Tuple => format!("{name}({})", show_all(fields).join(", ")),
                    StructKind::Record => {
                        let shown: Vec<String> = data
                            .fields
                            .iter()
                            .zip(fields)
                            .filter(|(_, w)| !matches!(w, WPat::Wild))
                            .map(|(f, w)| format!("{}: {}", f.name, self.show(w)))
                            .collect();
                        if shown.is_empty() {
                            format!("{name} {{ .. }}")
                        } else if shown.len() == data.fields.len() {
                            format!("{name} {{ {} }}", shown.join(", "))
                        } else {
                            format!("{name} {{ {}, .. }}", shown.join(", "))
                        }
                    }
                }
            }
            _ => "_".to_owned(),
        }
    }

    /// A struct's name, or a variant's path as rustc writes it: alone if
    /// the prelude has it (`None`), else after its enum's (`E::A`).
    fn variant_name(&self, strukt: flux_hir::StructId, variant: u32) -> String {
        let data = self.items.structs[strukt].variant(variant);
        let Some(_) = data.enum_name else {
            return data.name.to_string();
        };
        let in_prelude = self.items.def_map.prelude.is_some_and(|prelude| {
            let scope = &self.items.def_map.scopes[prelude.index()];
            [&scope.types, &scope.values].iter().any(|ns| {
                ns.get(&data.name)
                    .is_some_and(|b| b.def == flux_hir::def_map::Def::Variant(strukt, variant))
            })
        });
        if in_prelude {
            data.name.to_string()
        } else {
            data.path()
        }
    }
}

/// `row` specialized by `ctor` of `arity` fields: its first pattern's
/// fields and the rest, if it matches `ctor`.
fn specialize(row: &[DPat], ctor: Ctor, arity: usize) -> Option<Vec<DPat>> {
    let mut out = match &row[0] {
        DPat::Wild => vec![DPat::Wild; arity],
        DPat::Ctor(head, fields) => {
            let matches = match (*head, ctor) {
                (Ctor::Range(a, b), Ctor::Range(lo, hi)) => a <= lo && hi <= b,
                (head, ctor) => head == ctor,
            };
            if !matches {
                return None;
            }
            fields.clone()
        }
        DPat::Or(_) => unreachable!("or-patterns are expanded first"),
    };
    out.extend_from_slice(&row[1..]);
    Some(out)
}

/// An inclusive range of encoded integers as rustc writes it:
/// `5_u8`, `i32::MIN..=-1_i32`, `1_i128..`, `usize::MAX..`.
fn show_range(domain: IntDomain, lo: u128, hi: u128) -> String {
    let int = domain.int;
    let name = int.name();
    let (min, max) = domain.finite();
    let pointer_sized = matches!(int, IntTy::Isize | IntTy::Usize);
    let below = |encoded: u128| int == IntTy::Isize && encoded == 0;
    let beyond = |encoded: u128| pointer_sized && encoded == domain.max();
    let value = |encoded: u128| -> String {
        if encoded == max {
            format!("{name}::MAX")
        } else if encoded == min && int.is_signed() {
            format!("{name}::MIN")
        } else {
            let v = domain.decode(encoded).unwrap_or(0);
            if int == IntTy::U128 {
                format!("{}_{name}", v as u128)
            } else {
                format!("{v}_{name}")
            }
        }
    };
    if lo == hi {
        return if beyond(lo) {
            format!("{name}::MAX..")
        } else if below(lo) {
            format!("..{name}::MIN")
        } else {
            value(lo)
        };
    }
    // The 128-bit types' maximum, and the infinity of pointer-sized
    // ones, end ranges that are open.
    let open = beyond(hi) || (int.bits() == 128 && hi == max);
    match (below(lo), open) {
        (true, true) => "_".to_owned(),
        (true, false) => format!("..={}", value(hi)),
        (false, true) => format!("{}..", value(lo)),
        (false, false) => format!("{}..={}", value(lo), value(hi)),
    }
}
