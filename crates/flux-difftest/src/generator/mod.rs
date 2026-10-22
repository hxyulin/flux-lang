//! The program generator.
//!
//! [`generate`] turns a [`Source`] of choices into a Rust program that
//! rustc accepts and that is deterministic, terminates and has no
//! undefined behavior. It is type-directed: every expression is generated
//! for a type it must have, from the variables in scope that it may use
//! there, so the program type-checks and borrow-checks by construction.
//! The rules it follows are deliberately conservative (see [`func`]).
//!
//! A program prints what it computes as it goes: every observation is
//! `ck(tag, hash)`, which prints the tag and a hash of the value and folds
//! the hash into a running checksum, printed at the end and used as the
//! exit status. A mismatch between rustc's and flux's output is localized
//! by the first line that differs.
//!
//! The pieces of a program, in order:
//!
//! - structs and enums, some `Copy`, some with `Drop` impls that print;
//! - a trait with an associated constant, required and provided methods
//!   and an associated function, implemented for several types, and
//!   generic functions bounded by it (static dispatch);
//! - `const fn`s, constants computed by them at compile time, and statics;
//! - functions and methods, each calling only earlier ones, so the
//!   program terminates;
//! - `run`, which calls each function, and `main`, which prints the
//!   checksum and exits with it.
//!
//! Helpers are generated on demand: `hN` hashes a value of one type by
//! reference, `seN` observes a value and returns it (to record evaluation
//! order).

mod expr;
mod func;
mod pat;
pub mod ty;

use std::collections::HashMap;
use std::fmt::Write;

use crate::rng::Source;
use func::FnGen;
use ty::{IntTy, Ty};

/// How large programs get, and how often each feature appears. A weight
/// of 0 turns a feature off; the others are relative or percentages, as
/// each says.
#[derive(Clone, Debug)]
pub struct Config {
    pub max_structs: u32,
    pub max_enums: u32,
    pub max_fns: u32,
    /// Statements per block, at most.
    pub max_stmts: u32,
    /// Expression nesting, at most.
    pub max_depth: u32,
    pub max_params: u32,
    /// A rough bound on the work a program does: statements executed,
    /// counting loops' iterations and calls.
    pub cost_budget: u64,
    pub weights: Weights,
}

macro_rules! weights {
    ($($(#[doc = $doc:literal])* $name:ident = $default:expr,)*) => {
        /// Feature weights. Each is named on the command line as
        /// `--weight name=N`.
        #[derive(Clone, Debug)]
        pub struct Weights {
            $($(#[doc = $doc])* pub $name: u32,)*
        }

        impl Default for Weights {
            fn default() -> Weights {
                Weights { $($name: $default,)* }
            }
        }

        impl Weights {
            /// Every weight's name.
            pub const NAMES: &'static [&'static str] = &[$(stringify!($name),)*];

            /// Sets the weight called `name`.
            pub fn set(&mut self, name: &str, value: u32) -> Result<(), String> {
                match name {
                    $(stringify!($name) => self.$name = value,)*
                    _ => return Err(format!(
                        "unknown weight `{name}`; the weights are {}",
                        Self::NAMES.join(", ")
                    )),
                }
                Ok(())
            }
        }
    };
}

weights! {
    /// Structs with `Drop` impls that print, moved and dropped.
    drops = 30,
    /// A trait, its impls and generic functions over it.
    generics = 50,
    /// Closures bound to variables and called.
    closures = 30,
    /// Iterator chains over arrays and ranges.
    iterators = 30,
    /// Reference parameters, `let r = &x` and `&mut` through calls.
    refs = 40,
    /// `const fn`s evaluated at compile time and at run time.
    consts = 40,
    /// Statics and `static mut` counters.
    statics = 30,
    /// Inherent methods on structs.
    methods = 40,
    /// `match` expressions and statements.
    matches = 40,
    /// Loops: `for`, `while`, `loop` with `break` values, labels.
    loops = 40,
    /// `Option` values and their methods.
    options = 30,
    /// `Result` values and `?`.
    results = 20,
    /// `seN(tag, e)` probes recording evaluation order.
    side_effects = 20,
    /// Deliberate panics (an index out of bounds, `MIN / -1`): rare, as
    /// one ends the program.
    panics = 2,
    /// `char` values.
    chars = 20,
    /// `i128` and `u128`.
    wide_ints = 15,
    /// Function pointers.
    fn_ptrs = 15,
    /// `core::mem::{swap, replace, take}`.
    mem_ops = 15,
    /// Slices: `&a[lo..hi]` and their methods.
    slices = 25,
    /// `let ... else`.
    let_else = 15,
    /// `println!` and `eprintln!` of values with random format specs.
    println = 25,
    /// Arrays and tuples as values (in types chosen at random).
    aggregates = 40,

    /// Statics whose type contains an enum, percent.
    enum_statics = 25,
    /// `zip` in iterator chains, percent.
    zips = 25,

    /// Closure parameters left for inference, else annotated, percent.
    unannotated_closures = 25,

    // Constructs flux rejects for now (see tests/difftest/bugs): 0 keeps
    // campaigns from being swamped by known bugs; raise them once fixed.
    /// Parenthesized range-to patterns, `(..=5)` (bug 002), percent.
    paren_patterns = 0,
    /// `matches!(..)` unparenthesized, so it may end a block (bug 003), percent.
    bare_macros = 0,
    /// `rev()` after `zip`, `step_by`, `skip`, `take` and `enumerate` (bug 004), percent.
    rev_adapters = 0,
    /// `ref` bindings in `let ... else` (bug 005), percent.
    ref_let_else = 0,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_structs: 4,
            max_enums: 3,
            max_fns: 8,
            max_stmts: 6,
            max_depth: 3,
            max_params: 5,
            cost_budget: 40_000,
            weights: Weights::default(),
        }
    }
}

/// A struct: named fields (`S0 { a: i32 }`) or a tuple struct (`S1(i32)`).
pub struct StructDef {
    pub fields: Vec<Ty>,
    pub tuple: bool,
    pub copy: bool,
    /// Has a `Drop` impl (so it is not `Copy`, and fields may not be
    /// moved out of it).
    pub drop: bool,
    pub eq: bool,
}

impl StructDef {
    pub fn field_name(&self, i: usize) -> String {
        if self.tuple {
            i.to_string()
        } else {
            FIELD_NAMES[i % FIELD_NAMES.len()].to_owned()
        }
    }
}

const FIELD_NAMES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];

pub struct Variant {
    pub fields: Vec<Ty>,
    pub named: bool,
}

pub struct EnumDef {
    pub variants: Vec<Variant>,
    pub copy: bool,
    pub eq: bool,
}

/// How a method takes `self`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recv {
    Ref,
    RefMut,
    Value,
}

#[derive(Clone, Debug)]
pub enum FnKind {
    Free,
    /// An inherent method of a struct, or an associated function when
    /// `recv` is `None`.
    Method {
        owner: usize,
        recv: Option<Recv>,
    },
    /// Generic over `T: Tr`; its parameter types mention `Ty::Param`.
    Generic,
    Const,
}

/// A function the program can call.
#[derive(Clone, Debug)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    /// Roughly how many statements a call executes.
    pub cost: u64,
    pub kind: FnKind,
}

/// The program's trait, `Tr`, and the types implementing it.
pub struct TraitDef {
    pub k_ty: IntTy,
    pub get_ty: IntTy,
    pub put_ty: IntTy,
    pub make_ty: IntTy,
    pub impls: Vec<Ty>,
}

/// A constant or static.
pub struct Global {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
}

/// The program being generated.
pub struct Gen {
    pub src: Source,
    pub cfg: Config,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub fns: Vec<FnSig>,
    pub generic_fns: Vec<FnSig>,
    pub const_fns: Vec<FnSig>,
    /// The arguments each constant `C{i}` calls `cf{i}` with.
    pub const_args: Vec<Vec<String>>,
    pub tr: Option<TraitDef>,
    pub consts: Vec<Global>,
    pub statics: Vec<Global>,
    /// Item text, in order.
    items: Vec<String>,
    hashers: HashMap<Ty, String>,
    probes: HashMap<Ty, String>,
    helpers: String,
    next_tag: u32,
}

/// Generates a program from `src`.
pub fn generate(src: Source, cfg: &Config) -> (String, Source) {
    let mut g = Gen {
        src,
        cfg: cfg.clone(),
        structs: Vec::new(),
        enums: Vec::new(),
        fns: Vec::new(),
        generic_fns: Vec::new(),
        const_fns: Vec::new(),
        const_args: Vec::new(),
        tr: None,
        consts: Vec::new(),
        statics: Vec::new(),
        items: Vec::new(),
        hashers: HashMap::new(),
        probes: HashMap::new(),
        helpers: String::new(),
        next_tag: 1,
    };
    g.program();
    let mut text = String::new();
    text.push_str(PRELUDE);
    for item in &g.items {
        text.push('\n');
        text.push_str(item);
    }
    text.push_str(&g.helpers);
    (text, g.src)
}

/// What every program starts with: C's `printf` and `exit`, and the
/// checksum.
const PRELUDE: &str = r#"//! Generated by flux-difftest.
#![allow(warnings)]

use core::hint::black_box as bb;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}

static mut CK: u64 = 0xcbf2_9ce4_8422_2325;

/// Mixes `v` into a hash.
fn mx(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x0100_0000_01b3).rotate_left(7)
}

/// Prints an observation and folds it into the checksum.
fn ck(tag: u32, v: u64) {
    unsafe {
        CK = mx(CK, v ^ tag as u64);
        printf(c"%u %llx\n".as_ptr(), tag, v);
    }
}

fn main() {
    run();
    unsafe {
        printf(c"checksum %llx\n".as_ptr(), CK);
        exit((CK % 101) as i32);
    }
}
"#;

impl Gen {
    pub fn tag(&mut self) -> u32 {
        let t = self.next_tag;
        self.next_tag += 1;
        t
    }

    fn program(&mut self) {
        let n_structs = self.src.below(self.cfg.max_structs + 1);
        for _ in 0..n_structs {
            self.struct_def();
        }
        let n_enums = self.src.below(self.cfg.max_enums + 1);
        for _ in 0..n_enums {
            self.enum_def();
        }
        if self.src.chance(self.cfg.weights.consts) {
            let n = self.src.range(1, 3);
            for _ in 0..n {
                self.const_fn();
            }
        }
        if self.src.chance(self.cfg.weights.statics) {
            self.statics();
        }
        if self.src.chance(self.cfg.weights.generics) {
            self.trait_def();
        }
        let n_fns = self.src.below(self.cfg.max_fns + 1);
        for _ in 0..n_fns {
            if self.tr.is_some() && self.src.chance(self.cfg.weights.generics / 2) {
                self.generic_fn();
            } else if !self.structs.is_empty() && self.src.chance(self.cfg.weights.methods) {
                self.method();
            } else {
                self.free_fn();
            }
        }
        self.run_fn();
    }

    // ----- types -----

    /// A random type for a value, built from scalars and the structs and
    /// enums defined so far; `depth` bounds nesting.
    pub fn random_ty(&mut self, depth: u32) -> Ty {
        let w = &self.cfg.weights;
        let agg = if depth == 0 { 0 } else { w.aggregates };
        let weights = [
            30,
            6,
            if w.chars > 0 { 3 } else { 0 },
            agg / 4,
            agg / 4,
            if self.structs.is_empty() { 0 } else { 12 },
            if self.enums.is_empty() { 0 } else { 8 },
            if depth == 0 { 0 } else { w.options / 6 },
            if depth == 0 { 0 } else { w.results / 8 },
        ];
        match self.src.weighted(&weights) {
            0 => Ty::Int(self.int_ty()),
            1 => Ty::Bool,
            2 => Ty::Char,
            3 => {
                let n = self.src.range(2, 3);
                Ty::Tuple((0..n).map(|_| self.random_ty(depth - 1)).collect())
            }
            4 => {
                let elem = self.random_ty(depth - 1);
                Ty::Array(Box::new(elem), self.src.range(1, 5) as usize)
            }
            5 => Ty::Struct(self.src.below(self.structs.len() as u32) as usize),
            6 => Ty::Enum(self.src.below(self.enums.len() as u32) as usize),
            7 => Ty::Option(Box::new(self.random_ty(depth - 1))),
            _ => Ty::Result(
                Box::new(self.random_ty(depth - 1)),
                Box::new(Ty::Int(self.int_ty())),
            ),
        }
    }

    pub fn int_ty(&mut self) -> IntTy {
        let n = if self.cfg.weights.wide_ints > 0 && self.src.chance(self.cfg.weights.wide_ints) {
            12
        } else {
            10
        };
        IntTy::ALL[self.src.below(n) as usize]
    }

    pub fn is_copy(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int(_) | Ty::Bool | Ty::Char | Ty::Unit | Ty::Ref(_) | Ty::Fn(..) => true,
            Ty::Tuple(e) => e.iter().all(|t| self.is_copy(t)),
            Ty::Array(t, _) | Ty::Option(t) => self.is_copy(t),
            Ty::Result(t, e) => self.is_copy(t) && self.is_copy(e),
            Ty::Struct(i) => self.structs[*i].copy,
            Ty::Enum(i) => self.enums[*i].copy,
            Ty::RefMut(_) | Ty::Param => false,
        }
    }

    /// Whether `==` works on the type.
    pub fn is_eq(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int(_) | Ty::Bool | Ty::Char | Ty::Unit => true,
            Ty::Tuple(e) => e.iter().all(|t| self.is_eq(t)),
            Ty::Array(t, _) | Ty::Option(t) => self.is_eq(t),
            Ty::Result(t, e) => self.is_eq(t) && self.is_eq(e),
            Ty::Struct(i) => self.structs[*i].eq,
            Ty::Enum(i) => self.enums[*i].eq,
            Ty::Ref(_) | Ty::RefMut(_) | Ty::Param | Ty::Fn(..) => false,
        }
    }

    /// Whether a value can be hashed by an `hN` helper.
    pub fn is_hashable(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Param | Ty::Fn(..) => false,
            Ty::Tuple(e) => e.iter().all(|t| self.is_hashable(t)),
            Ty::Array(t, _) | Ty::Option(t) | Ty::Ref(t) | Ty::RefMut(t) => self.is_hashable(t),
            Ty::Result(t, e) => self.is_hashable(t) && self.is_hashable(e),
            _ => true,
        }
    }

    /// Whether the type has an enum (`Option` and `Result` included)
    /// anywhere inside.
    pub fn has_enum(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Enum(_) | Ty::Option(_) | Ty::Result(..) => true,
            Ty::Tuple(e) => e.iter().any(|t| self.has_enum(t)),
            Ty::Array(t, _) => self.has_enum(t),
            Ty::Struct(i) => self.structs[*i].fields.iter().any(|t| self.has_enum(t)),
            _ => false,
        }
    }

    /// Whether the type has a struct with a `Drop` impl anywhere inside.
    pub fn has_drop(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Tuple(e) => e.iter().any(|t| self.has_drop(t)),
            Ty::Array(t, _) | Ty::Option(t) => self.has_drop(t),
            Ty::Result(t, e) => self.has_drop(t) || self.has_drop(e),
            Ty::Struct(i) => {
                self.structs[*i].drop || self.structs[*i].fields.iter().any(|t| self.has_drop(t))
            }
            Ty::Enum(i) => self.enums[*i]
                .variants
                .iter()
                .any(|v| v.fields.iter().any(|t| self.has_drop(t))),
            _ => false,
        }
    }

    // ----- items -----

    fn struct_def(&mut self) {
        let index = self.structs.len();
        let drop = self.src.chance(self.cfg.weights.drops);
        let n = self.src.range(1, 4);
        let fields: Vec<Ty> = (0..n)
            .map(|_| {
                if drop {
                    // A `Drop` struct holds integers, so it is cheap to make.
                    Ty::Int(self.int_ty())
                } else {
                    self.random_ty(1)
                }
            })
            .collect();
        let tuple = self.src.chance(30);
        let all_copy = fields.iter().all(|t| self.is_copy(t));
        let copy = !drop && all_copy && self.src.below(4) != 0;
        let eq = fields.iter().all(|t| self.is_eq(t)) && self.src.chance(60);
        let def = StructDef {
            fields,
            tuple,
            copy,
            drop,
            eq,
        };
        let mut text = String::new();
        // `Debug` for `println!("{:?}")`.
        let mut derives = vec!["Debug"];
        if copy {
            derives.push("Clone");
            derives.push("Copy");
        }
        if eq {
            derives.push("PartialEq");
        }
        if !derives.is_empty() {
            let _ = writeln!(text, "#[derive({})]", derives.join(", "));
        }
        if tuple {
            let names: Vec<String> = def.fields.iter().map(Ty::name).collect();
            let _ = writeln!(text, "struct S{index}({});", names.join(", "));
        } else {
            let _ = writeln!(text, "struct S{index} {{");
            for (i, f) in def.fields.iter().enumerate() {
                let _ = writeln!(text, "    {}: {},", def.field_name(i), f.name());
            }
            text.push_str("}\n");
        }
        self.structs.push(def);
        if drop {
            let tag = self.tag() + 9000;
            let h = self.hasher(&Ty::Struct(index));
            let _ = write!(
                text,
                "\nimpl Drop for S{index} {{\n    fn drop(&mut self) {{\n        ck({tag}, {h}(self));\n    }}\n}}\n"
            );
        }
        self.items.push(text);
    }

    fn enum_def(&mut self) {
        let index = self.enums.len();
        let n = self.src.range(1, 4);
        let variants: Vec<Variant> = (0..n)
            .map(|_| {
                let k = self.src.below(4);
                let fields = (0..k).map(|_| self.random_ty(1)).collect();
                Variant {
                    fields,
                    named: self.src.chance(30),
                }
            })
            .collect();
        let all_copy = variants
            .iter()
            .all(|v| v.fields.iter().all(|t| self.is_copy(t)));
        let copy = all_copy && self.src.below(4) != 0;
        let eq = variants
            .iter()
            .all(|v| v.fields.iter().all(|t| self.is_eq(t)))
            && self.src.chance(60);
        let mut text = String::new();
        // `Debug` for `println!("{:?}")`.
        let mut derives = vec!["Debug"];
        if copy {
            derives.push("Clone");
            derives.push("Copy");
        }
        if eq {
            derives.push("PartialEq");
        }
        if !derives.is_empty() {
            let _ = writeln!(text, "#[derive({})]", derives.join(", "));
        }
        let _ = writeln!(text, "enum E{index} {{");
        for (i, v) in variants.iter().enumerate() {
            let _ = write!(text, "    V{i}");
            if !v.fields.is_empty() {
                if v.named {
                    let fs: Vec<String> = v
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(k, t)| format!("{}: {}", FIELD_NAMES[k], t.name()))
                        .collect();
                    let _ = write!(text, " {{ {} }}", fs.join(", "));
                } else {
                    let fs: Vec<String> = v.fields.iter().map(Ty::name).collect();
                    let _ = write!(text, "({})", fs.join(", "));
                }
            }
            text.push_str(",\n");
        }
        text.push_str("}\n");
        self.items.push(text);
        self.enums.push(EnumDef { variants, copy, eq });
    }

    /// A `const fn` over integers, a constant computed with it, and both
    /// observed by `run` (the constant at compile time, a call at run time).
    fn const_fn(&mut self) {
        let index = self.const_fns.len();
        let n = self.src.range(1, 3);
        let params: Vec<Ty> = (0..n).map(|_| Ty::Int(self.int_ty())).collect();
        let ret = Ty::Int(self.int_ty());
        let name = format!("cf{index}");
        let names: Vec<String> = (0..params.len()).map(|i| format!("p{i}")).collect();
        let mut f = FnGen::new(self, ret.clone(), true);
        for (name, ty) in names.iter().zip(&params) {
            f.declare_param(name, ty.clone());
        }
        let body = f.body();
        let cost = f.cost();
        let sig_params: Vec<String> = names
            .iter()
            .zip(&params)
            .map(|(n, t)| format!("{n}: {}", t.name()))
            .collect();
        // The constant `C{index}` is the function's value at arguments
        // chosen now; `run` calls it with the same ones at run time.
        let args: Vec<String> = params
            .iter()
            .map(|t| {
                let i = t.int().unwrap();
                let v = ty::int_value(&mut self.src, i);
                ty::int_literal(i, v)
            })
            .collect();
        let text = format!(
            "const fn {name}({}) -> {} {body}\n\nconst C{index}: {} = {name}({});\n",
            sig_params.join(", "),
            ret.name(),
            ret.name(),
            args.join(", ")
        );
        self.const_args.push(args);
        self.items.push(text);
        self.const_fns.push(FnSig {
            name,
            params,
            ret,
            cost,
            kind: FnKind::Const,
        });
    }

    fn statics(&mut self) {
        let n = self.src.range(1, 3);
        for _ in 0..n {
            let index = self.statics.len();
            let mutable = self.src.chance(50);
            let ty = if mutable {
                Ty::Int(self.int_ty())
            } else {
                let ty = self.random_ty(1);
                let enum_ok = !self.has_enum(&ty) || self.src.chance(self.cfg.weights.enum_statics);
                if self.has_drop(&ty) || !self.is_copy(&ty) || !enum_ok {
                    Ty::Int(self.int_ty())
                } else {
                    ty
                }
            };
            let name = format!("G{index}");
            let mut f = FnGen::new(self, ty.clone(), true);
            let value = f.const_value(&ty);
            let m = if mutable { "mut " } else { "" };
            self.items
                .push(format!("static {m}{name}: {} = {value};\n", ty.name()));
            self.statics.push(Global { name, ty, mutable });
        }
    }

    /// The trait `Tr` and its impls for a few types.
    fn trait_def(&mut self) {
        let k_ty = self.int_ty();
        let get_ty = self.int_ty();
        let put_ty = self.int_ty();
        let make_ty = self.int_ty();
        let mut text = format!(
            "trait Tr {{\n    const K: {k};\n    fn val(&self) -> {g};\n    fn store(&mut self, v: {p});\n    fn make(v: {m}) -> Self\n    where\n        Self: Sized;\n    fn mix(&self, k: {g}) -> {g} {{\n        self.val().wrapping_add(k) ^ (Self::K as {g})\n    }}\n}}\n",
            k = k_ty.name(),
            g = get_ty.name(),
            p = put_ty.name(),
            m = make_ty.name(),
        );
        self.tr = Some(TraitDef {
            k_ty,
            get_ty,
            put_ty,
            make_ty,
            impls: Vec::new(),
        });
        // Distinct types to implement it for.
        let n = self.src.range(1, 3);
        let mut impls: Vec<Ty> = Vec::new();
        for _ in 0..n {
            let ty = self.random_ty(1);
            if impls.contains(&ty) || !self.is_hashable(&ty) {
                continue;
            }
            impls.push(ty);
        }
        for ty in &impls {
            text.push('\n');
            text.push_str(&self.trait_impl(ty));
        }
        self.tr.as_mut().unwrap().impls = impls;
        self.items.push(text);
    }

    fn trait_impl(&mut self, ty: &Ty) -> String {
        let tr = self.tr.as_ref().unwrap();
        let (k_ty, get_ty, put_ty, make_ty) = (tr.k_ty, tr.get_ty, tr.put_ty, tr.make_ty);
        let k = {
            let v = ty::int_value(&mut self.src, k_ty);
            ty::int_literal(k_ty, v)
        };
        let mut f = FnGen::new(self, Ty::Int(get_ty), false);
        f.declare_param("self", Ty::Ref(Box::new(ty.clone())));
        let get = f.body();
        let mut f = FnGen::new(self, Ty::Unit, false);
        f.declare_param("self", Ty::RefMut(Box::new(ty.clone())));
        f.declare_param("v", Ty::Int(put_ty));
        let put = f.body();
        let mut f = FnGen::new(self, ty.clone(), false);
        f.declare_param("v", Ty::Int(make_ty));
        let make = f.body();
        let mut text = format!(
            "impl Tr for {t} {{\n    const K: {k_ty} = {k};\n    fn val(&self) -> {g} {get}\n    fn store(&mut self, v: {p}) {put}\n    fn make(v: {m}) -> {t} {make}\n",
            t = ty.name(),
            k_ty = k_ty.name(),
            g = get_ty.name(),
            p = put_ty.name(),
            m = make_ty.name(),
        );
        if self.src.chance(30) {
            let mut f = FnGen::new(self, Ty::Int(get_ty), false);
            f.declare_param("self", Ty::Ref(Box::new(ty.clone())));
            f.declare_param("k", Ty::Int(get_ty));
            let mix = f.body();
            let _ = writeln!(
                text,
                "    fn mix(&self, k: {}) -> {} {mix}",
                get_ty.name(),
                get_ty.name()
            );
        }
        text.push_str("}\n");
        // Indent the bodies one level, as they are inside the impl.
        text
    }

    fn random_params(&mut self, generic: bool) -> Vec<Ty> {
        let n = self.src.below(self.cfg.max_params + 1);
        let mut params = Vec::new();
        if generic {
            params.push(Ty::Ref(Box::new(Ty::Param)));
            if self.src.chance(60) {
                params.push(Ty::RefMut(Box::new(Ty::Param)));
            }
        }
        for _ in 0..n {
            let ty = self.random_ty(2);
            let ty = if self.src.chance(self.cfg.weights.refs / 2) {
                if self.src.chance(50) {
                    Ty::Ref(Box::new(ty))
                } else {
                    Ty::RefMut(Box::new(ty))
                }
            } else {
                ty
            };
            params.push(ty);
        }
        params
    }

    fn random_ret(&mut self) -> Ty {
        match self.src.weighted(&[
            10,
            2,
            self.cfg.weights.options / 6,
            self.cfg.weights.results / 6,
        ]) {
            0 => self.random_ty(2),
            1 => Ty::Unit,
            2 => Ty::Option(Box::new(self.random_ty(1))),
            _ => Ty::Result(
                Box::new(self.random_ty(1)),
                Box::new(Ty::Int(self.int_ty())),
            ),
        }
    }

    fn free_fn(&mut self) {
        let index = self.fns.len();
        let params = self.random_params(false);
        let ret = self.random_ret();
        let name = format!("f{index}");
        let (text, cost) = self.fn_text(&name, "", &params, &ret, None, false);
        self.items.push(text);
        self.fns.push(FnSig {
            name,
            params,
            ret,
            cost,
            kind: FnKind::Free,
        });
    }

    fn generic_fn(&mut self) {
        let index = self.generic_fns.len();
        let params = self.random_params(true);
        let ret = self.random_ret();
        let name = format!("g{index}");
        let (text, cost) = self.fn_text(&name, "<T: Tr>", &params, &ret, None, true);
        self.items.push(text);
        self.generic_fns.push(FnSig {
            name,
            params,
            ret,
            cost,
            kind: FnKind::Generic,
        });
    }

    /// An inherent method (or associated function) of a struct.
    fn method(&mut self) {
        let owner = self.src.below(self.structs.len() as u32) as usize;
        let index = self.fns.len();
        let recv = match self.src.weighted(&[4, 3, 2, 2]) {
            0 => Some(Recv::Ref),
            1 => Some(Recv::RefMut),
            2 => Some(Recv::Value),
            _ => None,
        };
        let params = self.random_params(false);
        let ret = if recv.is_none() {
            Ty::Struct(owner)
        } else {
            self.random_ret()
        };
        let name = format!("m{index}");
        let (body, cost) = self.fn_text(&name, "", &params, &ret, Some((owner, recv)), false);
        self.items.push(format!("impl S{owner} {{\n{body}}}\n"));
        self.fns.push(FnSig {
            name,
            params,
            ret,
            cost,
            kind: FnKind::Method { owner, recv },
        });
    }

    /// A function's text and cost.
    fn fn_text(
        &mut self,
        name: &str,
        generics: &str,
        params: &[Ty],
        ret: &Ty,
        method: Option<(usize, Option<Recv>)>,
        generic: bool,
    ) -> (String, u64) {
        let mut f = FnGen::new(self, ret.clone(), false);
        f.set_generic(generic);
        let mut sig = Vec::new();
        if let Some((owner, Some(recv))) = method {
            let (text, ty) = match recv {
                Recv::Ref => ("&self", Ty::Ref(Box::new(Ty::Struct(owner)))),
                Recv::RefMut => ("&mut self", Ty::RefMut(Box::new(Ty::Struct(owner)))),
                Recv::Value => ("self", Ty::Struct(owner)),
            };
            // `mut self` lets the body change its own copy.
            let mutable = recv == Recv::Value && f.g.src.chance(40);
            f.declare_var("self", ty, mutable);
            sig.push(if mutable {
                "mut self".to_owned()
            } else {
                text.to_owned()
            });
        }
        for (i, ty) in params.iter().enumerate() {
            let pname = format!("p{i}");
            let mutable = !matches!(ty, Ty::Ref(_) | Ty::RefMut(_)) && f.g.src.chance(30);
            f.declare_var(&pname, ty.clone(), mutable);
            let m = if mutable { "mut " } else { "" };
            sig.push(format!("{m}{pname}: {}", ty.name()));
        }
        let body = f.body();
        let cost = f.cost();
        let ret_text = if *ret == Ty::Unit {
            String::new()
        } else {
            format!(" -> {}", ret.name())
        };
        let text = format!("fn {name}{generics}({}){ret_text} {body}\n", sig.join(", "));
        (text, cost)
    }

    /// `run`: a random body, then a call of every function, observed.
    fn run_fn(&mut self) {
        let mut f = FnGen::new(self, Ty::Unit, false);
        let body = f.run_body();
        self.items.push(format!("fn run() {body}\n"));
    }

    // ----- helpers made on demand -----

    /// The name of the function hashing a `&ty`, defining it first if
    /// needed.
    pub fn hasher(&mut self, ty: &Ty) -> String {
        if let Ty::Ref(t) | Ty::RefMut(t) = ty {
            return self.hasher(t);
        }
        if let Some(name) = self.hashers.get(ty) {
            return name.clone();
        }
        let name = format!("h{}", self.hashers.len());
        self.hashers.insert(ty.clone(), name.clone());
        let body = match ty {
            Ty::Int(i) if i.bits() == 128 => {
                "(*v as u64) ^ ((*v >> 64) as u64).rotate_left(29)".to_owned()
            }
            Ty::Int(_) | Ty::Bool | Ty::Char => "*v as u64".to_owned(),
            Ty::Unit => "0".to_owned(),
            Ty::Tuple(elems) => {
                let mut s = "0x7475_706c".to_owned();
                for (i, t) in elems.iter().enumerate() {
                    let h = self.hasher(t);
                    s = format!("mx({s}, {h}(&v.{i}))");
                }
                s
            }
            Ty::Array(t, n) => {
                let h = self.hasher(t);
                format!(
                    "let mut s = 0x6172_7279u64;\n    let mut i = 0;\n    while i < {n} {{\n        s = mx(s, {h}(&v[i]));\n        i += 1;\n    }}\n    s"
                )
            }
            Ty::Struct(i) => {
                let def = &self.structs[*i];
                let fields: Vec<(String, Ty)> = def
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(k, t)| (def.field_name(k), t.clone()))
                    .collect();
                let mut s = format!("{}", 0x5300 + i);
                for (f, t) in fields {
                    let h = self.hasher(&t);
                    s = format!("mx({s}, {h}(&v.{f}))");
                }
                s
            }
            Ty::Enum(i) => {
                let variants: Vec<(usize, Vec<Ty>, bool)> = self.enums[*i]
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(k, v)| (k, v.fields.clone(), v.named))
                    .collect();
                let mut arms = String::new();
                for (k, fields, named) in variants {
                    let names: Vec<String> = (0..fields.len()).map(|j| format!("x{j}")).collect();
                    let pat = if fields.is_empty() {
                        format!("E{i}::V{k}")
                    } else if named {
                        let fs: Vec<String> = names
                            .iter()
                            .enumerate()
                            .map(|(j, n)| format!("{}: {n}", FIELD_NAMES[j]))
                            .collect();
                        format!("E{i}::V{k} {{ {} }}", fs.join(", "))
                    } else {
                        format!("E{i}::V{k}({})", names.join(", "))
                    };
                    let mut s = format!("{}", 0x4500 + k);
                    for (n, t) in names.iter().zip(&fields) {
                        let h = self.hasher(t);
                        s = format!("mx({s}, {h}({n}))");
                    }
                    let _ = writeln!(arms, "        {pat} => {s},");
                }
                format!("match v {{\n{arms}    }}")
            }
            Ty::Option(t) => {
                let h = self.hasher(t);
                format!(
                    "match v {{\n        Some(x) => mx(0x536f, {h}(x)),\n        None => 0x4e6f,\n    }}"
                )
            }
            Ty::Result(t, e) => {
                let h = self.hasher(t);
                let he = self.hasher(e);
                format!(
                    "match v {{\n        Ok(x) => mx(0x4f6b, {h}(x)),\n        Err(e) => mx(0x4572, {he}(e)),\n    }}"
                )
            }
            Ty::Ref(_) | Ty::RefMut(_) | Ty::Param | Ty::Fn(..) => unreachable!("not hashable"),
        };
        let _ = write!(
            self.helpers,
            "\nfn {name}(v: &{}) -> u64 {{\n    {body}\n}}\n",
            ty.name()
        );
        name
    }

    /// The name of the probe that observes a value of `ty` and returns it.
    pub fn probe(&mut self, ty: &Ty) -> String {
        if let Some(name) = self.probes.get(ty) {
            return name.clone();
        }
        let name = format!("se{}", self.probes.len());
        self.probes.insert(ty.clone(), name.clone());
        let h = self.hasher(ty);
        let _ = write!(
            self.helpers,
            "\nfn {name}(tag: u32, v: {t}) -> {t} {{\n    ck(tag, {h}(&v));\n    v\n}}\n",
            t = ty.name()
        );
        name
    }
}
