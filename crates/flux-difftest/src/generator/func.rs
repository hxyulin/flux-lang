//! Function bodies: variables in scope, the borrow and move rules the
//! generator keeps, and statements. Expressions are in [`super::expr`].
//!
//! The rules are conservative versions of rustc's, so every program
//! borrow-checks:
//!
//! - A variable of a type that is not `Copy` is only ever moved whole,
//!   and only if it was declared in the current loop (so no loop moves it
//!   twice) and outside any closure. A move in a branch counts after the
//!   branch, whichever branch ran.
//! - Borrows are tracked per variable: a reference stored in a variable
//!   (`let r = &x;`) borrows `x` until the end of `r`'s block; a borrow in
//!   a call's arguments, until the end of the call; matching a place
//!   borrows it for the arms. Nothing is written or moved while borrowed,
//!   nor read while mutably borrowed.
//! - Closures read only `Copy` values from outside (by copy for `move`
//!   closures, which see no `&mut` references), and never write.
//! - Guards, iterator chains and closures are *pure*: nothing in them
//!   writes, moves or borrows mutably.
//! - Arithmetic whose operands rustc's constant propagation may know is
//!   given an opaque operand (`bb(x)`, `core::hint::black_box`), so its
//!   deny-by-default overflow lints never fire; shift amounts it may know
//!   are in range, and divisors are never zero.

use std::fmt::Write;

use super::ty::{self, IntTy, Ty};
use super::{FnKind, Gen, Recv};

/// An expression's text, and whether rustc's lints might know its value.
#[derive(Clone, Debug)]
pub struct Ex {
    pub s: String,
    pub konst: bool,
    /// For a place in a variable, the variable.
    pub root: Option<usize>,
}

impl Ex {
    pub fn new(s: impl Into<String>, konst: bool) -> Ex {
        Ex {
            s: s.into(),
            konst,
            root: None,
        }
    }

    pub fn place(s: impl Into<String>, konst: bool, root: usize) -> Ex {
        Ex {
            s: s.into(),
            konst,
            root: Some(root),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Var {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
    pub moved: bool,
    /// The loop nesting it was declared at.
    pub loop_depth: u32,
    /// The closure nesting it was declared at.
    pub level: u32,
    /// Whether rustc may know its value (it was given a constant).
    pub known: bool,
}

#[derive(Clone, Copy, Debug)]
struct Borrow {
    var: usize,
    mutable: bool,
}

struct LoopCtx {
    /// The loop's label, if it was written with one.
    label: Option<String>,
    /// The type of `break`'s value, for a `loop` whose value is used.
    value: Option<Ty>,
    level: u32,
}

/// A place in a variable: `x`, `x.a.1`, `(*r).b`, `a[0]`.
#[derive(Clone, Debug)]
pub struct Place {
    pub text: String,
    pub ty: Ty,
    pub root: usize,
    /// Reached through a shared reference.
    pub shared: bool,
    /// The variable itself, not a part of it.
    pub whole: bool,
}

/// Generates one function body (or closure body, or constant).
pub struct FnGen<'g> {
    pub g: &'g mut Gen,
    pub vars: Vec<Var>,
    scopes: Vec<(usize, usize)>,
    borrows: Vec<Borrow>,
    loops: Vec<LoopCtx>,
    pub loop_depth: u32,
    /// Closure nesting; `move_closure[level - 1]` says whether each
    /// closure is a `move` one.
    pub level: u32,
    move_closure: Vec<bool>,
    /// Inside a guard, iterator chain or closure: no writes, moves or
    /// mutable borrows.
    pub pure_depth: u32,
    /// How many times the current statement runs per call.
    pub mult: u64,
    cost: u64,
    pub ret: Ty,
    pub const_mode: bool,
    pub generic: bool,
    /// Inside the trait's impls: no calls of the trait's methods.
    pub in_impl: bool,
    pub callable_fns: usize,
    pub callable_generic: usize,
    pub callable_const: usize,
    pub indent: usize,
    block_depth: u32,
    /// No `ref` bindings in the pattern being generated.
    pub forbid_ref: bool,
    names: u32,
}

impl<'g> FnGen<'g> {
    pub fn new(g: &'g mut Gen, ret: Ty, const_mode: bool) -> FnGen<'g> {
        let callable_fns = g.fns.len();
        let callable_generic = g.generic_fns.len();
        let callable_const = g.const_fns.len();
        let in_impl = g.tr.is_some() && g.tr.as_ref().unwrap().impls.is_empty();
        FnGen {
            g,
            vars: Vec::new(),
            scopes: Vec::new(),
            borrows: Vec::new(),
            loops: Vec::new(),
            loop_depth: 0,
            level: 0,
            move_closure: Vec::new(),
            pure_depth: 0,
            mult: 1,
            cost: 0,
            ret,
            const_mode,
            generic: false,
            in_impl,
            callable_fns,
            callable_generic,
            callable_const,
            indent: 0,
            block_depth: 0,
            forbid_ref: false,
            names: 0,
        }
    }

    pub fn set_generic(&mut self, generic: bool) {
        self.generic = generic;
    }

    pub fn cost(&self) -> u64 {
        self.cost.max(1)
    }

    pub fn add_cost(&mut self, c: u64) {
        self.cost = self.cost.saturating_add(c.saturating_mul(self.mult));
    }

    pub fn over_budget(&self) -> bool {
        self.cost > self.g.cfg.cost_budget
    }

    /// Whether a call costing `c` fits.
    pub fn affordable(&self, c: u64) -> bool {
        let total = c.saturating_mul(self.mult);
        total <= self.g.cfg.cost_budget / 4 && self.cost + total <= self.g.cfg.cost_budget
    }

    pub fn fresh(&mut self, prefix: &str) -> String {
        self.names += 1;
        format!("{prefix}{}", self.names)
    }

    pub fn pad(&self) -> String {
        "    ".repeat(self.indent)
    }

    // ----- variables and scopes -----

    pub fn declare_param(&mut self, name: &str, ty: Ty) {
        self.declare_var(name, ty, false);
    }

    pub fn declare_var(&mut self, name: &str, ty: Ty, mutable: bool) -> usize {
        self.declare(name, ty, mutable, false)
    }

    pub fn declare(&mut self, name: &str, ty: Ty, mutable: bool, known: bool) -> usize {
        self.vars.push(Var {
            name: name.to_owned(),
            ty,
            mutable,
            moved: false,
            loop_depth: self.loop_depth,
            level: self.level,
            known,
        });
        self.vars.len() - 1
    }

    pub fn push_scope(&mut self) {
        self.scopes.push((self.vars.len(), self.borrows.len()));
    }

    pub fn pop_scope(&mut self) {
        let (vars, borrows) = self.scopes.pop().unwrap();
        self.vars.truncate(vars);
        self.borrows.truncate(borrows);
    }

    pub fn borrow_mark(&self) -> usize {
        self.borrows.len()
    }

    pub fn release_borrows(&mut self, mark: usize) {
        self.borrows.truncate(mark);
    }

    pub fn borrow(&mut self, var: usize, mutable: bool) {
        self.borrows.push(Borrow { var, mutable });
    }

    fn borrowed(&self, var: usize) -> bool {
        self.borrows.iter().any(|b| b.var == var)
    }

    fn mut_borrowed(&self, var: usize) -> bool {
        self.borrows.iter().any(|b| b.var == var && b.mutable)
    }

    /// Whether the variable's `Copy` parts may be read here.
    pub fn can_read(&self, v: usize) -> bool {
        let var = &self.vars[v];
        if var.moved || self.mut_borrowed(v) {
            return false;
        }
        if var.level < self.level {
            // Captured by a closure: `Copy` reads only; a `move` closure
            // copies the variable, so it must be `Copy` (or a shared
            // reference) as a whole.
            let moving = self.move_closure[var.level as usize..].iter().any(|&m| m);
            if moving && !self.g.is_copy(&var.ty) {
                return false;
            }
        }
        true
    }

    /// Whether `&x` (or a part of it) may be taken.
    pub fn can_borrow(&self, v: usize) -> bool {
        self.can_read(v) && self.vars[v].level == self.level
    }

    pub fn effects_allowed(&self) -> bool {
        self.pure_depth == 0
    }

    /// Whether the variable (or a part, through `&mut`) may be assigned or
    /// mutably borrowed.
    pub fn can_write(&self, v: usize) -> bool {
        let var = &self.vars[v];
        self.effects_allowed() && var.level == self.level && !self.borrowed(v)
    }

    pub fn can_move(&self, v: usize) -> bool {
        let var = &self.vars[v];
        self.effects_allowed()
            && !var.moved
            && var.level == self.level
            && var.loop_depth == self.loop_depth
            && !self.borrowed(v)
            && !matches!(var.ty, Ty::RefMut(_))
    }

    /// Every place in variable `v`.
    pub fn places_of(&self, v: usize, out: &mut Vec<Place>) {
        let var = &self.vars[v];
        self.descend(v, var.name.clone(), &var.ty, false, true, 0, out);
    }

    #[allow(clippy::too_many_arguments)]
    fn descend(
        &self,
        root: usize,
        text: String,
        ty: &Ty,
        shared: bool,
        whole: bool,
        depth: u32,
        out: &mut Vec<Place>,
    ) {
        out.push(Place {
            text: text.clone(),
            ty: ty.clone(),
            root,
            shared,
            whole,
        });
        if depth >= 3 {
            return;
        }
        match ty {
            Ty::Ref(t) => self.descend(root, format!("(*{text})"), t, true, false, depth + 1, out),
            Ty::RefMut(t) => {
                self.descend(root, format!("(*{text})"), t, shared, false, depth + 1, out)
            }
            Ty::Tuple(elems) => {
                for (i, t) in elems.iter().enumerate() {
                    self.descend(
                        root,
                        format!("{text}.{i}"),
                        t,
                        shared,
                        false,
                        depth + 1,
                        out,
                    );
                }
            }
            Ty::Struct(s) => {
                let def = &self.g.structs[*s];
                for (i, t) in def.fields.iter().enumerate() {
                    let f = def.field_name(i);
                    self.descend(
                        root,
                        format!("{text}.{f}"),
                        t,
                        shared,
                        false,
                        depth + 1,
                        out,
                    );
                }
            }
            Ty::Array(t, n) => {
                self.descend(root, format!("{text}[0]"), t, shared, false, depth + 1, out);
                if *n > 1 {
                    let last = n - 1;
                    self.descend(
                        root,
                        format!("{text}[{last}]"),
                        t,
                        shared,
                        false,
                        depth + 1,
                        out,
                    );
                }
            }
            _ => {}
        }
    }

    /// Places of type `ty` that may be read by copy.
    pub fn readable_places(&self, ty: &Ty) -> Vec<Place> {
        let mut out = Vec::new();
        if !self.g.is_copy(ty) {
            return out;
        }
        for v in 0..self.vars.len() {
            if !self.can_read(v) {
                continue;
            }
            let mut ps = Vec::new();
            self.places_of(v, &mut ps);
            out.extend(
                ps.into_iter()
                    .filter(|p| &p.ty == ty && !self.through_mut_in_move(p)),
            );
        }
        out
    }

    /// A `move` closure would capture a `&mut` it reads through by value.
    fn through_mut_in_move(&self, p: &Place) -> bool {
        let var = &self.vars[p.root];
        var.level < self.level && matches!(var.ty, Ty::RefMut(_)) && !p.whole
    }

    /// Places of type `ty` (any) that may be borrowed shared.
    pub fn borrowable_places(&self, ty: &Ty) -> Vec<Place> {
        let mut out = Vec::new();
        for v in 0..self.vars.len() {
            if !self.can_borrow(v) {
                continue;
            }
            let mut ps = Vec::new();
            self.places_of(v, &mut ps);
            out.extend(ps.into_iter().filter(|p| &p.ty == ty));
        }
        out
    }

    /// Places of type `ty` that may be assigned or borrowed mutably.
    pub fn writable_places(&self, ty: Option<&Ty>) -> Vec<Place> {
        let mut out = Vec::new();
        if !self.effects_allowed() {
            return out;
        }
        for v in 0..self.vars.len() {
            if !self.can_write(v) {
                continue;
            }
            let var = &self.vars[v];
            let mut ps = Vec::new();
            self.places_of(v, &mut ps);
            for p in ps {
                if ty.is_some_and(|t| t != &p.ty) || p.shared || matches!(p.ty, Ty::Fn(..)) {
                    continue;
                }
                if matches!(p.ty, Ty::Ref(_) | Ty::RefMut(_)) && !p.whole {
                    continue;
                }
                let through_ref = p.text.starts_with("(*");
                let ok = if through_ref {
                    !var.moved
                } else if p.whole {
                    var.mutable && !matches!(var.ty, Ty::Ref(_) | Ty::RefMut(_))
                } else {
                    var.mutable && !var.moved
                };
                if ok {
                    out.push(p);
                }
            }
        }
        out
    }

    /// Whole variables of type `ty` that may be moved out.
    pub fn movable_vars(&self, ty: &Ty) -> Vec<usize> {
        (0..self.vars.len())
            .filter(|&v| &self.vars[v].ty == ty && self.can_move(v))
            .collect()
    }

    pub fn moved_state(&self) -> Vec<bool> {
        self.vars.iter().map(|v| v.moved).collect()
    }

    pub fn set_moved_state(&mut self, state: &[bool]) {
        for (v, &m) in self.vars.iter_mut().zip(state) {
            v.moved = m;
        }
    }

    /// Runs `branches` alternatives from the same moved state, then
    /// leaves every variable any of them moved as moved.
    pub fn branches<T>(&mut self, n: usize, mut f: impl FnMut(&mut Self, usize) -> T) -> Vec<T> {
        let start = self.moved_state();
        let mut merged = start.clone();
        let mut results = Vec::new();
        for i in 0..n {
            self.set_moved_state(&start);
            results.push(f(self, i));
            for (m, v) in merged.iter_mut().zip(&self.vars) {
                *m |= v.moved;
            }
        }
        self.set_moved_state(&merged);
        results
    }

    // ----- bodies -----

    /// A function body: statements and a tail expression of the return
    /// type, in braces.
    pub fn body(&mut self) -> String {
        if self.const_mode {
            self.g.cfg.max_stmts = self.g.cfg.max_stmts.min(4);
        }
        let ret = self.ret.clone();
        self.block(Some(&ret))
    }

    /// A block: `{ statements tail }`, its own scope.
    pub fn block(&mut self, tail: Option<&Ty>) -> String {
        self.push_scope();
        self.block_depth += 1;
        self.indent += 1;
        let mut out = String::from("{\n");
        let max = self
            .g
            .cfg
            .max_stmts
            .saturating_sub(self.block_depth.saturating_sub(1) * 2)
            .max(1);
        let n = self.g.src.below(max + 1);
        for _ in 0..n {
            if self.over_budget() {
                break;
            }
            let s = self.stmt();
            out.push_str(&s);
        }
        if let Some(ty) = tail
            && *ty != Ty::Unit
        {
            let depth = self.g.cfg.max_depth;
            let e = self.expr(ty, depth);
            let _ = writeln!(out, "{}{}", self.pad(), e.s);
        }
        self.indent -= 1;
        self.block_depth -= 1;
        let _ = write!(out, "{}}}", self.pad());
        self.pop_scope();
        out
    }

    /// `run`'s body: random statements, then a call of every function and
    /// an observation of every constant.
    pub fn run_body(&mut self) -> String {
        self.push_scope();
        self.indent = 1;
        self.block_depth = 1;
        let mut out = String::from("{\n");
        let n = self.g.src.below(self.g.cfg.max_stmts * 2 + 1);
        for _ in 0..n {
            if self.over_budget() {
                break;
            }
            let s = self.stmt();
            out.push_str(&s);
        }
        for i in 0..self.g.const_fns.len() {
            let sig = self.g.const_fns[i].clone();
            let tag = self.g.tag();
            let h = self.g.hasher(&sig.ret);
            let _ = writeln!(out, "    ck({tag}, {h}(&C{i}));");
            // The same call at run time, its arguments hidden from rustc.
            let args = self.g.const_args[i].clone();
            let hidden: Vec<String> = args.iter().map(|a| format!("bb({a})")).collect();
            let tag = self.g.tag();
            let _ = writeln!(
                out,
                "    ck({tag}, {h}(&{}({})));",
                sig.name,
                hidden.join(", ")
            );
        }
        for i in 0..self.g.statics.len() {
            let s = &self.g.statics[i];
            let (name, ty, mutable) = (s.name.clone(), s.ty.clone(), s.mutable);
            let tag = self.g.tag();
            let h = self.g.hasher(&ty);
            if mutable {
                let _ = writeln!(out, "    ck({tag}, {h}(&unsafe {{ {name} }}));");
            } else {
                let _ = writeln!(out, "    ck({tag}, {h}(&{name}));");
            }
        }
        let depth = self.g.cfg.max_depth;
        for i in 0..self.g.fns.len() {
            let sig = self.g.fns[i].clone();
            let call = self.call_sig(&sig, None, depth.min(2));
            out.push_str(&self.observe_text(&call, &sig.ret));
        }
        for i in 0..self.g.generic_fns.len() {
            let sig = self.g.generic_fns[i].clone();
            let impls = self.g.tr.as_ref().unwrap().impls.clone();
            for imp in impls {
                let call = self.call_sig(&sig, Some(&imp), depth.min(2));
                out.push_str(&self.observe_text(&call, &sig.ret));
            }
        }
        out.push('}');
        self.pop_scope();
        out
    }

    /// A statement observing `e` of type `ty` (or just evaluating it).
    fn observe_text(&mut self, e: &Ex, ty: &Ty) -> String {
        if *ty == Ty::Unit || !self.g.is_hashable(ty) {
            return format!("{}{};\n", self.pad(), e.s);
        }
        let tag = self.g.tag();
        let h = self.g.hasher(ty);
        format!("{}ck({tag}, {h}(&{}));\n", self.pad(), e.s)
    }

    /// A constant expression of `ty`, for a static's initializer.
    pub fn const_value(&mut self, ty: &Ty) -> String {
        self.const_mode = true;
        let e = self.expr(ty, 1);
        e.s
    }

    // ----- statements -----

    /// One statement, with its indentation and newline.
    pub fn stmt(&mut self) -> String {
        self.add_cost(1);
        let w = self.g.cfg.weights.clone();
        let in_loop = self.loops.last().is_some_and(|l| l.level == self.level);
        let nested = self.block_depth < 4;
        let c = self.const_mode;
        let weights = [
            20,                         // let
            if c { 0 } else { 14 },     // observe
            10,                         // assign
            8,                          // compound assign
            if nested { 7 } else { 0 }, // if
            if nested && self.mult < 64 {
                w.loops / 5
            } else {
                0
            }, // loop
            if nested { w.matches / 6 } else { 0 }, // match
            if c { 0 } else { 7 },      // call
            if nested && !c { w.drops / 6 } else { 0 }, // block
            if c { 0 } else { 3 },      // deferred init
            if c || !nested { 0 } else { w.refs / 6 }, // let r = &x
            if c { 0 } else { w.closures / 5 }, // closure
            if c { 0 } else { w.mem_ops / 4 }, // mem ops
            if self.level == 0 { 2 } else { 0 }, // early return
            if in_loop { 5 } else { 0 }, // break/continue
            if c { 0 } else { w.let_else / 4 }, // let-else
            if c || !nested { 0 } else { w.options / 5 }, // if let
            if c || self.g.statics.iter().all(|s| !s.mutable) {
                0
            } else {
                3
            }, // static mut
            if c { 0 } else { w.panics }, // panic
            if c { 0 } else { w.fn_ptrs / 4 }, // fn pointer
            if c { 0 } else { w.println / 2 }, // println!
        ];
        let s = match self.g.src.weighted(&weights) {
            0 => self.stmt_let(),
            1 => self.stmt_observe(),
            2 => self.stmt_assign(),
            3 => self.stmt_compound(),
            4 => self.stmt_if(),
            5 => self.stmt_loop(),
            6 => self.stmt_match(),
            7 => self.stmt_call(),
            8 => self.stmt_block(),
            9 => self.stmt_deferred(),
            10 => self.stmt_let_ref(),
            11 => self.stmt_closure(),
            12 => self.stmt_mem(),
            13 => self.stmt_return(),
            14 => self.stmt_break(),
            15 => self.stmt_let_else(),
            16 => self.stmt_if_let(),
            17 => self.stmt_static_mut(),
            18 => self.stmt_panic(),
            19 => self.stmt_fn_ptr(),
            _ => self.stmt_println(),
        };
        s.unwrap_or_default()
    }

    /// A type for a new local.
    fn local_ty(&mut self) -> Ty {
        if self.const_mode {
            return if self.g.src.chance(15) {
                Ty::Bool
            } else {
                Ty::Int(self.g.int_ty())
            };
        }
        if self.generic && self.g.src.chance(10) {
            return Ty::Param;
        }
        self.g.random_ty(2)
    }

    fn stmt_let(&mut self) -> Option<String> {
        let ty = self.local_ty();
        let depth = self.g.cfg.max_depth;
        let e = self.expr(&ty, depth);
        let mutable = self.g.src.chance(50);
        let name = self.fresh("v");
        let m = if mutable { "mut " } else { "" };
        let annotate = self.g.src.chance(60) || ty.contains_param();
        let annotation = if annotate {
            format!(": {}", ty.name())
        } else {
            String::new()
        };
        // Destructure tuples and structs sometimes.
        if let Ty::Tuple(elems) = &ty
            && self.g.src.chance(30)
        {
            let names: Vec<String> = elems.iter().map(|_| self.fresh("v")).collect();
            let pats: Vec<String> = names
                .iter()
                .map(|n| {
                    if mutable {
                        format!("mut {n}")
                    } else {
                        n.clone()
                    }
                })
                .collect();
            for (n, t) in names.iter().zip(elems) {
                self.declare(n, t.clone(), mutable, e.konst);
            }
            return Some(format!(
                "{}let ({}){annotation} = {};\n",
                self.pad(),
                pats.join(", "),
                e.s
            ));
        }
        let v = self.declare(&name, ty, mutable, e.konst);
        let _ = v;
        Some(format!(
            "{}let {m}{name}{annotation} = {};\n",
            self.pad(),
            e.s
        ))
    }

    fn stmt_observe(&mut self) -> Option<String> {
        // A place, or an expression.
        if self.g.src.chance(60) {
            let mut candidates = Vec::new();
            for v in 0..self.vars.len() {
                if self.can_borrow(v) {
                    let mut ps = Vec::new();
                    self.places_of(v, &mut ps);
                    candidates.extend(ps.into_iter().filter(|p| self.g.is_hashable(&p.ty)));
                }
            }
            if !candidates.is_empty() {
                let p = candidates.swap_remove(self.g.src.below(candidates.len() as u32) as usize);
                let tag = self.g.tag();
                let h = self.g.hasher(&p.ty);
                let arg = match p.ty {
                    Ty::Ref(_) => p.text.clone(),
                    Ty::RefMut(_) => format!("&*{}", p.text),
                    _ => format!("&{}", p.text),
                };
                return Some(format!("{}ck({tag}, {h}({arg}));\n", self.pad()));
            }
        }
        let ty = self.g.random_ty(2);
        let depth = self.g.cfg.max_depth;
        let e = self.expr(&ty, depth);
        Some(self.observe_text(&e, &ty))
    }

    fn stmt_assign(&mut self) -> Option<String> {
        let places = self.writable_places(None);
        let places: Vec<Place> = places
            .into_iter()
            .filter(|p| !p.ty.contains_param() || self.generic)
            .collect();
        if places.is_empty() {
            return None;
        }
        let p = places[self.g.src.below(places.len() as u32) as usize].clone();
        let depth = self.g.cfg.max_depth;
        // The place is borrowed (mutably) only once its value is ready.
        let e = self.expr(&p.ty, depth);
        if !self.can_write(p.root) || (!p.whole && self.vars[p.root].moved) {
            return None;
        }
        let var = &mut self.vars[p.root];
        if p.whole && var.loop_depth == self.loop_depth {
            var.moved = false;
        }
        var.known |= e.konst;
        Some(format!("{}{} = {};\n", self.pad(), p.text, e.s))
    }

    fn stmt_compound(&mut self) -> Option<String> {
        let places: Vec<Place> = self
            .writable_places(None)
            .into_iter()
            .filter(|p| p.ty.int().is_some() || p.ty == Ty::Bool)
            .collect();
        if places.is_empty() {
            return None;
        }
        let p = places[self.g.src.below(places.len() as u32) as usize].clone();
        let known = self.vars[p.root].known;
        let depth = self.g.cfg.max_depth;
        let text = match p.ty {
            Ty::Bool => {
                let op = ["&=", "|=", "^="][self.g.src.below(3) as usize];
                let e = self.expr(&Ty::Bool, depth);
                format!("{} {op} {}", p.text, e.s)
            }
            Ty::Int(i) => {
                if self.const_mode {
                    // Constants check overflow: wrapping methods only.
                    let op = ["wrapping_add", "wrapping_sub", "wrapping_mul"]
                        [self.g.src.below(3) as usize];
                    let e = self.expr(&p.ty, depth);
                    if self.g.src.chance(50) {
                        let op = ["&=", "|=", "^="][self.g.src.below(3) as usize];
                        format!("{} {op} {}", p.text, e.s)
                    } else {
                        format!("{} = {}.{op}({})", p.text, p.text, e.s)
                    }
                } else {
                    match self.g.src.below(4) {
                        0 | 1 => {
                            let op =
                                ["+=", "-=", "*=", "&=", "|=", "^="][self.g.src.below(6) as usize];
                            let mut e = self.expr(&p.ty, depth);
                            if known && e.konst && matches!(op, "+=" | "-=" | "*=") {
                                e = self.opaque(e);
                            }
                            format!("{} {op} {}", p.text, e.s)
                        }
                        2 => {
                            let op = ["<<=", ">>="][self.g.src.below(2) as usize];
                            let s = self.shift_amount(i);
                            format!("{} {op} {}", p.text, s.s)
                        }
                        _ => {
                            let op = ["/=", "%="][self.g.src.below(2) as usize];
                            let e = self.expr(&p.ty, depth);
                            if i.signed() {
                                // `MIN / -1` would panic: keep the divisor positive.
                                let max = ty::int_literal(i, i.max_bits());
                                format!("{} {op} (({}) & {max}) | 1", p.text, e.s)
                            } else {
                                format!("{} {op} ({}) | 1", p.text, e.s)
                            }
                        }
                    }
                }
            }
            _ => unreachable!(),
        };
        if !self.can_write(p.root) || self.vars[p.root].moved {
            return None;
        }
        Some(format!("{}{text};\n", self.pad()))
    }

    pub fn cond(&mut self, depth: u32) -> String {
        let e = self.expr(&Ty::Bool, depth);
        paren_if_brace(e.s)
    }

    fn stmt_if(&mut self) -> Option<String> {
        let depth = self.g.cfg.max_depth.min(2);
        let c = self.cond(depth);
        let has_else = self.g.src.chance(50);
        let blocks = self.branches(if has_else { 2 } else { 1 }, |f, _| f.block(None));
        let mut s = format!("{}if {c} {}", self.pad(), blocks[0]);
        if has_else {
            let _ = write!(s, " else {}", blocks[1]);
        }
        s.push('\n');
        Some(s)
    }

    /// Runs `f` for a loop body that runs up to `trips` times.
    /// Runs `f` where no `break` or `continue` may be written: the value
    /// a loop's own exit `break`s with, which must not jump elsewhere.
    pub(super) fn no_jumps<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.loops.push(LoopCtx {
            label: None,
            value: None,
            level: u32::MAX,
        });
        let r = f(self);
        self.loops.pop();
        r
    }

    pub(super) fn in_loop<T>(
        &mut self,
        trips: u64,
        label: Option<String>,
        value: Option<Ty>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let saved = self.mult;
        self.mult = self.mult.saturating_mul(trips.max(1));
        self.loop_depth += 1;
        self.loops.push(LoopCtx {
            label,
            value,
            level: self.level,
        });
        let r = f(self);
        self.loops.pop();
        self.loop_depth -= 1;
        self.mult = saved;
        r
    }

    fn stmt_loop(&mut self) -> Option<String> {
        let trips = self.g.src.range(0, 6);
        let label = self.fresh("'l");
        let labeled = self.g.src.chance(30);
        let lab = if labeled {
            format!("{label}: ")
        } else {
            String::new()
        };
        let loop_label = labeled.then(|| label.clone());
        let pad = self.pad();
        let int = self.g.int_ty();
        let kind = if self.const_mode {
            1
        } else {
            self.g.src.weighted(&[4, 3, 2, 3])
        };
        let s = match kind {
            0 => {
                // for i in a..b
                let i = self.fresh("i");
                let lo = ty::int_literal(int, 0);
                let hi = ty::int_literal(int, trips as u128);
                let inclusive = self.g.src.chance(30) && trips > 0;
                let range = if inclusive {
                    format!("{lo}..={}", ty::int_literal(int, trips as u128 - 1))
                } else {
                    format!("{lo}..{hi}")
                };
                let rev = self.g.src.chance(20);
                let range = if rev {
                    format!("({range}).rev()")
                } else {
                    range
                };
                let body = self.in_loop(trips as u64, loop_label.clone(), None, |f| {
                    f.push_scope();
                    f.declare(&i, Ty::Int(int), false, false);
                    let b = f.block(None);
                    f.pop_scope();
                    b
                });
                format!("{pad}{lab}for {i} in {range} {body}\n")
            }
            1 => {
                // while with a counter
                let k = self.fresh("k");
                let body = self.in_loop(trips as u64, loop_label.clone(), None, |f| {
                    let b = f.block(None);
                    // The counter is bumped first, so `continue` cannot skip it.
                    b.replacen("{\n", &format!("{{\n{}    {k} += 1;\n", f.pad()), 1)
                });
                format!("{pad}let mut {k} = 0u32;\n{pad}{lab}while {k} < {trips} {body}\n")
            }
            2 => {
                // for over an array's elements
                let mut arrays = Vec::new();
                for v in 0..self.vars.len() {
                    let mut ps = Vec::new();
                    self.places_of(v, &mut ps);
                    arrays.extend(ps.into_iter().filter(|p| matches!(p.ty, Ty::Array(..))));
                }
                if arrays.is_empty() {
                    return None;
                }
                let p = arrays[self.g.src.below(arrays.len() as u32) as usize].clone();
                let Ty::Array(elem, n) = p.ty.clone() else {
                    unreachable!()
                };
                let mutable = self.g.src.chance(40)
                    && self
                        .writable_places(Some(&p.ty))
                        .iter()
                        .any(|w| w.text == p.text);
                if !mutable && !self.can_borrow(p.root) {
                    return None;
                }
                let x = self.fresh("x");
                let enumerate = self.g.src.chance(30);
                let idx = self.fresh("i");
                let (head, item) = if mutable {
                    (format!("{}.iter_mut()", p.text), Ty::RefMut(elem.clone()))
                } else if self.g.src.chance(50) {
                    (format!("&{}", p.text), Ty::Ref(elem.clone()))
                } else {
                    (format!("{}.iter()", p.text), Ty::Ref(elem.clone()))
                };
                let (pat, head) = if enumerate {
                    let head = if head.starts_with('&') {
                        format!("{}.iter().enumerate()", p.text)
                    } else {
                        format!("{head}.enumerate()")
                    };
                    (format!("({idx}, {x})"), head)
                } else {
                    (x.clone(), head)
                };
                let mark = self.borrow_mark();
                self.borrow(p.root, mutable);
                let body = self.in_loop(n as u64, loop_label.clone(), None, |f| {
                    f.push_scope();
                    if enumerate {
                        f.declare(&idx, Ty::Int(IntTy::Usize), false, false);
                    }
                    f.declare(&x, item, false, false);
                    let b = f.block(None);
                    f.pop_scope();
                    b
                });
                self.release_borrows(mark);
                format!("{pad}{lab}for {pat} in {head} {body}\n")
            }
            _ => {
                // loop with a counter and a break value
                let k = self.fresh("k");
                let ty = if self.g.src.chance(50) {
                    Ty::Unit
                } else {
                    self.g.random_ty(1)
                };
                let v = self.fresh("v");
                let depth = self.g.cfg.max_depth.min(2);
                let (body, exit) =
                    self.in_loop(trips as u64, loop_label.clone(), Some(ty.clone()), |f| {
                        let exit = if ty == Ty::Unit {
                            String::new()
                        } else {
                            format!(" {}", f.no_jumps(|f| f.expr(&ty, depth)).s)
                        };
                        (f.block(None), exit)
                    });
                let target = if labeled {
                    format!(" {label}")
                } else {
                    String::new()
                };
                let body = body.replacen(
                    "{\n",
                    &format!(
                        "{{\n{pad}    {k} += 1;\n{pad}    if {k} > {trips} {{\n{pad}        break{target}{exit};\n{pad}    }}\n"
                    ),
                    1,
                );
                if ty == Ty::Unit {
                    format!("{pad}let mut {k} = 0u32;\n{pad}{lab}loop {body}\n")
                } else {
                    let s = format!(
                        "{pad}let mut {k} = 0u32;\n{pad}let {v}: {} = {lab}loop {body};\n",
                        ty.name()
                    );
                    self.declare(&v, ty, false, true);
                    s
                }
            }
        };
        Some(s)
    }

    /// `if c { break; }` or `continue`, of the innermost loop or of a
    /// labeled one around it (in the same closure).
    fn stmt_break(&mut self) -> Option<String> {
        let innermost = self.loops.len().checked_sub(1)?;
        if self.loops[innermost].level != self.level {
            return None;
        }
        let candidates: Vec<usize> = (0..self.loops.len())
            .filter(|&i| {
                self.loops[i].level == self.level
                    && (i == innermost || self.loops[i].label.is_some())
            })
            .collect();
        let pick = candidates[self.g.src.below(candidates.len() as u32) as usize];
        let depth = self.g.cfg.max_depth.min(2);
        let c = self.cond(depth);
        let label = match &self.loops[pick].label {
            Some(l) if pick != innermost || self.g.src.chance(50) => format!(" {l}"),
            _ => String::new(),
        };
        let value = self.loops[pick].value.clone();
        let what = if self.g.src.chance(40) {
            format!("continue{label}")
        } else {
            let v = match value {
                Some(t) if t != Ty::Unit => format!(" {}", self.expr(&t, depth).s),
                _ => String::new(),
            };
            format!("break{label}{v}")
        };
        let pad = self.pad();
        Some(format!("{pad}if {c} {{\n{pad}    {what};\n{pad}}}\n"))
    }

    fn stmt_match(&mut self) -> Option<String> {
        let ty = if self.const_mode {
            Ty::Int(self.g.int_ty())
        } else {
            self.g.random_ty(2)
        };
        let m = self.match_expr(&ty, &Ty::Unit, 2)?;
        Some(format!("{}{};\n", self.pad(), m))
    }

    fn stmt_call(&mut self) -> Option<String> {
        // A method with `&mut self` on a place, the trait's `put`, or any call.
        let depth = self.g.cfg.max_depth.min(2);
        if self.g.src.chance(40)
            && let Some(s) = self.mut_method_call(depth)
        {
            return Some(format!("{}{s};\n", self.pad()));
        }
        let e = self.any_call(depth)?;
        Some(self.observe_text(&e.0, &e.1))
    }

    fn stmt_block(&mut self) -> Option<String> {
        // A block with values to drop at its end.
        let b = self.block(None);
        Some(format!("{}{b}\n", self.pad()))
    }

    fn stmt_deferred(&mut self) -> Option<String> {
        let ty = self.g.random_ty(1);
        let name = self.fresh("v");
        let depth = self.g.cfg.max_depth.min(2);
        let c = self.cond(depth);
        let pad = self.pad();
        let values = self.branches(2, |f, _| {
            f.indent += 1;
            let e = f.expr(&ty, depth);
            f.indent -= 1;
            e.s
        });
        let s = format!(
            "{pad}let {name}: {};\n{pad}if {c} {{\n{pad}    {name} = {};\n{pad}}} else {{\n{pad}    {name} = {};\n{pad}}}\n",
            ty.name(),
            values[0],
            values[1]
        );
        self.declare(&name, ty, false, false);
        Some(s)
    }

    fn stmt_let_ref(&mut self) -> Option<String> {
        let mutable = self.g.src.chance(40);
        let mut candidates = Vec::new();
        if mutable {
            candidates = self.writable_places(None);
            candidates.retain(|p| !matches!(p.ty, Ty::Ref(_) | Ty::RefMut(_) | Ty::Param));
            candidates.retain(|p| !(p.whole && self.vars[p.root].moved));
        } else {
            for v in 0..self.vars.len() {
                if self.can_borrow(v) {
                    let mut ps = Vec::new();
                    self.places_of(v, &mut ps);
                    candidates.extend(
                        ps.into_iter()
                            .filter(|p| !matches!(p.ty, Ty::Ref(_) | Ty::RefMut(_) | Ty::Param)),
                    );
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let p = candidates[self.g.src.below(candidates.len() as u32) as usize].clone();
        let name = self.fresh("r");
        let (amp, ty) = if mutable {
            ("&mut ", Ty::RefMut(Box::new(p.ty.clone())))
        } else {
            ("&", Ty::Ref(Box::new(p.ty.clone())))
        };
        self.borrow(p.root, mutable);
        self.declare(&name, ty, false, false);
        Some(format!("{}let {name} = {amp}{};\n", self.pad(), p.text))
    }

    fn stmt_closure(&mut self) -> Option<String> {
        let n = self.g.src.range(0, 3);
        let params: Vec<Ty> = (0..n).map(|_| self.g.random_ty(1)).collect();
        let ret = self.g.random_ty(1);
        let name = self.fresh("c");
        let pnames: Vec<String> = (0..n).map(|_| self.fresh("a")).collect();
        let body = self.closure_body(&pnames, &params, &ret, true);
        let ps: Vec<String> = pnames
            .iter()
            .zip(&params)
            .map(|(n, t)| format!("{n}: {}", t.name()))
            .collect();
        self.declare(&name, Ty::Fn(params, Box::new(ret.clone())), false, false);
        Some(format!(
            "{}let {name} = move |{}| -> {} {body};\n",
            self.pad(),
            ps.join(", "),
            ret.name()
        ))
    }

    /// A closure's body, with its parameters in scope.
    pub fn closure_body(
        &mut self,
        names: &[String],
        params: &[Ty],
        ret: &Ty,
        moving: bool,
    ) -> String {
        self.level += 1;
        self.move_closure.push(moving);
        self.pure_depth += 1;
        let saved_loops = self.loop_depth;
        self.push_scope();
        for (n, t) in names.iter().zip(params) {
            self.declare(n, t.clone(), false, false);
        }
        let body = self.block(Some(ret));
        self.pop_scope();
        self.loop_depth = saved_loops;
        self.pure_depth -= 1;
        self.move_closure.pop();
        self.level -= 1;
        body
    }

    fn stmt_mem(&mut self) -> Option<String> {
        let places = self.writable_places(None);
        let places: Vec<Place> = places
            .into_iter()
            .filter(|p| !matches!(p.ty, Ty::Ref(_) | Ty::RefMut(_) | Ty::Fn(..)))
            .filter(|p| !(p.whole && self.vars[p.root].moved))
            .collect();
        if places.is_empty() {
            return None;
        }
        let p = places[self.g.src.below(places.len() as u32) as usize].clone();
        let depth = self.g.cfg.max_depth.min(2);
        match self.g.src.below(3) {
            0 => {
                // swap with another place of the same type in another variable
                let others: Vec<Place> = places_filter(&self.writable_places(Some(&p.ty)), p.root);
                let others: Vec<Place> = others
                    .into_iter()
                    .filter(|q| !(q.whole && self.vars[q.root].moved))
                    .collect();
                if others.is_empty() {
                    return None;
                }
                let q = &others[self.g.src.below(others.len() as u32) as usize];
                Some(format!(
                    "{}core::mem::swap(&mut {}, &mut {});\n",
                    self.pad(),
                    p.text,
                    q.text
                ))
            }
            1 => {
                // `&mut place` is taken before the new value is computed.
                let mark = self.borrow_mark();
                self.borrow(p.root, true);
                let e = self.expr(&p.ty, depth);
                self.release_borrows(mark);
                let old = core_call("core::mem::replace", &format!("&mut {}, {}", p.text, e.s));
                Some(self.observe_text(&Ex::new(old, false), &p.ty))
            }
            _ => {
                if !self.is_default(&p.ty) {
                    return None;
                }
                let old = core_call("core::mem::take", &format!("&mut {}", p.text));
                Some(self.observe_text(&Ex::new(old, false), &p.ty))
            }
        }
    }

    /// Whether `core` implements `Default` for the type.
    fn is_default(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int(_) | Ty::Bool | Ty::Char | Ty::Unit | Ty::Option(_) => true,
            Ty::Tuple(e) => e.iter().all(|t| self.is_default(t)),
            Ty::Array(t, _) => self.is_default(t),
            _ => false,
        }
    }

    fn stmt_return(&mut self) -> Option<String> {
        let depth = self.g.cfg.max_depth.min(2);
        let c = self.cond(depth);
        let ret = self.ret.clone();
        let pad = self.pad();
        let value = self.branches(1, |f, _| {
            if ret == Ty::Unit {
                String::new()
            } else {
                format!(" {}", f.expr(&ret, depth).s)
            }
        });
        Some(format!(
            "{pad}if {c} {{\n{pad}    return{};\n{pad}}}\n",
            value[0]
        ))
    }

    /// Something that leaves the current block: for `let ... else`.
    fn diverge(&mut self) -> String {
        let depth = self.g.cfg.max_depth.min(2);
        if let Some(l) = self.loops.last()
            && l.level == self.level
            && self.g.src.chance(50)
        {
            let value = l.value.clone();
            return match value {
                Some(t) if t != Ty::Unit => format!("break {}", self.expr(&t, depth).s),
                _ => ["break", "continue"][self.g.src.below(2) as usize].to_owned(),
            };
        }
        let ret = self.ret.clone();
        if ret == Ty::Unit {
            "return".to_owned()
        } else {
            format!("return {}", self.expr(&ret, depth).s)
        }
    }

    fn stmt_let_else(&mut self) -> Option<String> {
        if self.level > 0 {
            return None;
        }
        let ty = self.g.random_ty(2);
        let depth = self.g.cfg.max_depth.min(2);
        let e = self.expr(&ty, depth);
        self.forbid_ref = !self.g.src.chance(self.g.cfg.weights.ref_let_else);
        let pat = self.pattern(&ty, false, true, false);
        self.forbid_ref = false;
        let pad = self.pad();
        self.indent += 1;
        let d = self.branches(1, |f, _| f.diverge());
        self.indent -= 1;
        let s = format!(
            "{pad}let {} = {} else {{\n{pad}    {};\n{pad}}};\n",
            pat.text,
            paren_if_brace(e.s),
            d[0]
        );
        for (n, t, m) in pat.binds {
            self.declare(&n, t, m, true);
        }
        Some(s)
    }

    fn stmt_if_let(&mut self) -> Option<String> {
        let ty = self.g.random_ty(2);
        let depth = self.g.cfg.max_depth.min(2);
        let (scrutinee, by_ref, root) = self.scrutinee(&ty, depth);
        let pat = self.pattern(&ty, by_ref, true, false);
        let mark = self.borrow_mark();
        if let Some(r) = root {
            self.borrow(r, false);
        }
        let has_else = self.g.src.chance(50);
        let blocks = self.branches(if has_else { 2 } else { 1 }, |f, i| {
            if i == 0 {
                f.push_scope();
                for (n, t, m) in &pat.binds {
                    f.declare(n, t.clone(), *m, true);
                }
                let b = f.block(None);
                f.pop_scope();
                b
            } else {
                f.block(None)
            }
        });
        self.release_borrows(mark);
        let mut s = format!(
            "{}if let {} = {} {}",
            self.pad(),
            pat.text,
            scrutinee,
            blocks[0]
        );
        if has_else {
            let _ = write!(s, " else {}", blocks[1]);
        }
        s.push('\n');
        Some(s)
    }

    fn stmt_static_mut(&mut self) -> Option<String> {
        let muts: Vec<usize> = (0..self.g.statics.len())
            .filter(|&i| self.g.statics[i].mutable)
            .collect();
        if muts.is_empty() || !self.effects_allowed() {
            return None;
        }
        let i = muts[self.g.src.below(muts.len() as u32) as usize];
        let (name, ty) = (self.g.statics[i].name.clone(), self.g.statics[i].ty.clone());
        let depth = self.g.cfg.max_depth.min(2);
        let e = self.expr(&ty, depth);
        let op = ["wrapping_add", "wrapping_mul", "wrapping_sub"][self.g.src.below(3) as usize];
        Some(format!(
            "{}unsafe {{\n{}    {name} = {name}.{op}({});\n{}}}\n",
            self.pad(),
            self.pad(),
            e.s,
            self.pad()
        ))
    }

    /// A deliberate panic, which both compilers must abort on: an index
    /// past an array's end, or `MIN / -1`, behind a condition.
    fn stmt_panic(&mut self) -> Option<String> {
        let depth = self.g.cfg.max_depth.min(2);
        let c = self.cond(depth);
        let pad = self.pad();
        let what = match self.g.src.below(3) {
            0 => {
                let n = self.g.src.range(1, 4);
                let i = n + self.g.src.below(3);
                format!("ck(0, [7u8; {n}][bb({i}usize)] as u64)")
            }
            1 => "ck(0, (bb(i32::MIN) / bb(-1i32)) as u64)".to_owned(),
            _ => "ck(0, bb(None::<u8>).unwrap() as u64)".to_owned(),
        };
        Some(format!("{pad}if {c} {{\n{pad}    {what};\n{pad}}}\n"))
    }

    fn stmt_fn_ptr(&mut self) -> Option<String> {
        let candidates: Vec<usize> = (0..self.callable_fns)
            .filter(|&i| matches!(self.g.fns[i].kind, FnKind::Free))
            .filter(|&i| self.affordable(self.g.fns[i].cost))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let sig =
            self.g.fns[candidates[self.g.src.below(candidates.len() as u32) as usize]].clone();
        if sig
            .params
            .iter()
            .any(|t| matches!(t, Ty::Ref(_) | Ty::RefMut(_)))
        {
            // Function pointers with reference parameters are higher-ranked.
            return None;
        }
        let name = self.fresh("fp");
        let ty = Ty::Fn(sig.params.clone(), Box::new(sig.ret.clone()));
        self.declare(&name, ty.clone(), false, false);
        Some(format!(
            "{}let {name}: {} = {};\n",
            self.pad(),
            ty.name(),
            sig.name
        ))
    }

    /// `println!` (or `eprintln!`) of a place or a value with a random
    /// format spec: `println!("p7 [{:>+08x}]", v3)`, `{v3:?}`, `{0:#?}`.
    fn stmt_println(&mut self) -> Option<String> {
        let tag = self.g.tag();
        let depth = self.g.cfg.max_depth.min(2);
        // A variable (captured by name, or passed), or an expression.
        let mut vars: Vec<usize> = (0..self.vars.len())
            .filter(|&v| self.can_borrow(v) && debuggable(&self.vars[v].ty))
            .collect();
        let (arg, ty, name) = if !vars.is_empty() && self.g.src.chance(50) {
            let v = vars.swap_remove(self.g.src.below(vars.len() as u32) as usize);
            let name = self.vars[v].name.clone();
            (name.clone(), self.vars[v].ty.clone(), Some(name))
        } else {
            let ty = self.g.random_ty(2);
            let e = self.expr(&ty, depth);
            (e.s, ty, None)
        };
        if !debuggable(&ty) {
            return None;
        }
        let spec = format_spec(&mut self.g.src, ty.strip_refs());
        let mac = if self.g.src.chance(15) {
            "eprintln"
        } else {
            "println"
        };
        let pad = self.pad();
        let s = match (name, self.g.src.below(3)) {
            // Captured by name: `{v3:?}`.
            (Some(n), 0) => format!("{pad}{mac}!(\"p{tag} [{{{n}{spec}}}]\");\n"),
            // Positional, twice: `{0:x} {0:?}`.
            (_, 1) => format!("{pad}{mac}!(\"p{tag} [{{0{spec}}}] {{0:?}}\", {arg});\n"),
            _ => format!("{pad}{mac}!(\"p{tag} [{{{spec}}}]\", {arg});\n"),
        };
        Some(s)
    }

    /// A call of a method taking `&mut self`, or of the trait's `put`, on
    /// a place.
    fn mut_method_call(&mut self, depth: u32) -> Option<String> {
        let mut options: Vec<(Place, String, Vec<Ty>)> = Vec::new();
        for i in 0..self.callable_fns {
            let sig = &self.g.fns[i];
            if let FnKind::Method {
                owner,
                recv: Some(Recv::RefMut),
            } = sig.kind
                && self.affordable(sig.cost)
            {
                for p in self.writable_places(Some(&Ty::Struct(owner))) {
                    if p.whole && self.vars[p.root].moved {
                        continue;
                    }
                    options.push((p, sig.name.clone(), sig.params.clone()));
                }
            }
        }
        if !self.in_impl
            && let Some(tr) = &self.g.tr
        {
            let put = Ty::Int(tr.put_ty);
            let mut tys: Vec<Ty> = tr.impls.clone();
            if self.generic {
                tys.push(Ty::Param);
            }
            for t in tys {
                for p in self.writable_places(Some(&t)) {
                    if p.whole && self.vars[p.root].moved {
                        continue;
                    }
                    options.push((p, "store".to_owned(), vec![put.clone()]));
                }
                for p in self.writable_places(Some(&Ty::RefMut(Box::new(t.clone())))) {
                    if p.whole {
                        let text = format!("(*{})", p.text);
                        options.push((Place { text, ..p }, "store".to_owned(), vec![put.clone()]));
                    }
                }
            }
        }
        if options.is_empty() {
            return None;
        }
        let (p, name, params) =
            options.swap_remove(self.g.src.below(options.len() as u32) as usize);
        let mark = self.borrow_mark();
        self.borrow(p.root, true);
        let args = self.args(&params, depth);
        self.release_borrows(mark);
        Some(format!("{}.{name}({})", p.text, args.join(", ")))
    }
}

/// Whether `{:?}` works on the type (every generated type derives
/// `Debug`).
fn debuggable(ty: &Ty) -> bool {
    match ty {
        Ty::Param | Ty::Fn(..) => false,
        Ty::Tuple(e) => e.iter().all(debuggable),
        Ty::Array(t, _) | Ty::Option(t) | Ty::Ref(t) | Ty::RefMut(t) => debuggable(t),
        Ty::Result(t, e) => debuggable(t) && debuggable(e),
        _ => true,
    }
}

/// A format spec for a value of `ty`, from the colon on (or nothing):
/// `[[fill]align][sign]['#']['0'][width]['.' precision][type]`.
fn format_spec(src: &mut crate::rng::Source, ty: &Ty) -> String {
    fn fill_align(src: &mut crate::rng::Source, spec: &mut String) {
        if src.chance(40) {
            if src.chance(50) {
                spec.push(['*', '_', '.', '0', 'x'][src.below(5) as usize]);
            }
            spec.push(['<', '>', '^'][src.below(3) as usize]);
        }
    }
    fn width(src: &mut crate::rng::Source, spec: &mut String) {
        if src.chance(50) {
            spec.push_str(&src.below(21).to_string());
        }
    }
    let mut spec = String::new();
    match ty {
        Ty::Int(_) => {
            let kind = ["", "?", "x", "X", "o", "b", "e", "E", "x?", "X?"][src.below(10) as usize];
            fill_align(src, &mut spec);
            if src.chance(25) {
                spec.push('+');
            }
            if src.chance(30) {
                spec.push('#');
            }
            if src.chance(25) {
                spec.push('0');
            }
            width(src, &mut spec);
            if matches!(kind, "e" | "E") && src.chance(50) {
                spec.push_str(&format!(".{}", src.below(6)));
            }
            spec.push_str(kind);
        }
        Ty::Bool | Ty::Char => {
            fill_align(src, &mut spec);
            width(src, &mut spec);
            if src.chance(50) {
                spec.push('?');
            }
        }
        _ => {
            // Debug passes its options on to the fields.
            if src.chance(30) {
                spec.push('#');
            }
            if src.chance(30) {
                width(src, &mut spec);
            }
            spec.push_str(["?", "?", "x?", "X?"][src.below(4) as usize]);
        }
    }
    if spec.is_empty() {
        spec
    } else {
        format!(":{spec}")
    }
}

fn places_filter(places: &[Place], not_root: usize) -> Vec<Place> {
    places
        .iter()
        .filter(|p| p.root != not_root)
        .cloned()
        .collect()
}

fn core_call(path: &str, args: &str) -> String {
    format!("{path}({args})")
}

/// Wraps an expression in parentheses if it has a brace, so a struct
/// literal or block is not taken for the body of an `if` or `match`.
pub fn paren_if_brace(s: String) -> String {
    if s.contains('{') { format!("({s})") } else { s }
}
