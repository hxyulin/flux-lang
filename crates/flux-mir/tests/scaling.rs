//! Building MIR and checking it takes time linear in the program: four
//! times as many functions, or statements in a body, take about four
//! times as long. Only MIR is timed.

#[path = "../../flux-syntax/tests/support/scaling.rs"]
mod scaling;

use flux_hir::Hir;
use flux_ty::TypedFile;
use scaling::{assert_scales, repeat};

/// The program `text`, lowered and checked, which must be clean.
fn typed(text: String) -> (Hir, TypedFile) {
    let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics.first());
    let typed = flux_ty::check_file(&hir);
    assert!(
        typed.diagnostics.is_empty(),
        "{:?}",
        typed.diagnostics.first()
    );
    (hir, typed)
}

fn build((hir, typed): &(Hir, TypedFile)) {
    let mir = flux_mir::build_file(hir, typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics.first());
}

#[test]
fn many_functions() {
    let make = |n| {
        let fns = repeat(n, |i| {
            format!(
                "fn f{i}(p: &mut P, x: i32) -> i32 {{\n\
                 \x20   let r = &mut p.a;\n\
                 \x20   *r += x;\n\
                 \x20   if x > {i} {{ p.b }} else {{ *r / 2 }}\n\
                 }}"
            )
        });
        typed(format!(
            "struct P {{ a: i32, b: i32 }}\n{fns}fn main() {{}}\n"
        ))
    };
    assert_scales("functions", 300, make, build);
}

/// A body of `n` statements, `make(i)` each, between a parameter `x0`
/// and returning `x{n}`; with a type `D` with a destructor and a function
/// `take` that drops one.
fn body(n: usize, make: impl Fn(usize) -> String) -> (Hir, TypedFile) {
    let body = repeat(n, make);
    typed(format!(
        "struct D(i32);\n\
         impl Drop for D {{ fn drop(&mut self) {{}} }}\n\
         fn take(_d: D) {{}}\n\
         fn f(x0: i32) -> i32 {{\n{body}    x{n}\n}}\nfn main() {{}}\n"
    ))
}

/// One block: each statement names the binding just before it. The
/// borrow check once copied the set of live locals, a `bool` each, at
/// every statement: quadratic in the length of a body.
#[test]
fn long_block() {
    let make = |n| {
        body(n, |i| {
            format!(
                "    let x{} = x{i} * 3 + 1;\n    let t{i} = (x{i}, true);",
                i + 1
            )
        })
    };
    assert_scales("statements in a block", 1000, make, build);
}

/// Each statement branches, borrows or calls, so starts a block. The
/// initialization check's dataflow once kept a `bool` per move path for
/// each block, and the borrow check one per local for each statement and
/// block, and looked through every loan at each: quadratic in the length
/// of such a body. The initialization check keeps a bit per move path
/// for each block, still quadratic but 64 times smaller, and the borrow
/// check lists the locals holding loans that are live at each block's
/// end.
#[test]
fn long_body() {
    let make = |n| {
        body(n, |i| {
            let next = i + 1;
            let mut s = format!(
                "    let r{next} = &x{i};\n    let x{next} = if *r{next} > {i} {{ *r{next} - 1 }} \
                 else {{ x{i} + 1 }};"
            );
            if i % 8 == 0 {
                s += &format!("\n    take(D(x{next}));");
            }
            s
        })
    };
    assert_scales("statements in a body", 300, make, build);
}

/// Many regions and constraints: each statement borrows, through a
/// struct holding a reference and functions returning one, whose
/// results the next statements use: a few region variables and
/// constraints per statement.
#[test]
fn many_regions() {
    let make = |n| {
        let body = repeat(n, |i| {
            let next = i + 1;
            format!(
                "    let h{i} = H {{ r: &x{i} }};\n    let x{next} = *pick(h{i}.r, &x{i}) + 1;\n    let m{next} = pick(h{i}.r, &x{next});\n    let x{next} = x{next} + *m{next};"
            )
        });
        typed(format!(
            "struct H<'a> {{ r: &'a i32 }}\n\
             fn pick<'a>(a: &'a i32, b: &'a i32) -> &'a i32 {{ if *a > *b {{ a }} else {{ b }} }}\n\
             fn f(x0: i32) -> i32 {{\n{body}    x{n}\n}}\nfn main() {{}}\n"
        ))
    };
    assert_scales("regions in a body", 300, make, build);
}

/// A `mut` reference carried through a body, reassigned from each new
/// borrow: each loan's region is the rest of the body, as the reference's
/// is, so finding where loans are in scope is quadratic, as in rustc's
/// NLL, whose regions and borrow dataflow are the same.
#[test]
#[ignore = "known quadratic: every loan is live for the rest of the body"]
fn long_lived_reference() {
    let make = |n| {
        let body = repeat(n, |i| {
            let next = i + 1;
            format!("    let x{next} = *m + {i};\n    m = pick(m, &x{next});")
        });
        typed(format!(
            "fn pick<'a>(a: &'a i32, b: &'a i32) -> &'a i32 {{ if *a > *b {{ a }} else {{ b }} }}\n\
             fn f(x0: i32) -> i32 {{\n    let mut m = &x0;\n{body}    *m\n}}\nfn main() {{}}\n"
        ))
    };
    assert_scales("a reference carried through a body", 300, make, build);
}

/// Runs the ignored test `child` of this binary with `FLUX_SCALING_SIZE`
/// set to `size`, failing if it fails or takes more than `limit`: for
/// programs whose time was exponential in `size`, which an
/// [`assert_scales`] run at four times the size would never finish.
fn bounded(child: &str, size: usize, limit: std::time::Duration) {
    let mut process = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", child, "--nocapture"])
        .env("FLUX_SCALING_SIZE", size.to_string())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = process.try_wait().unwrap() {
            assert!(status.success(), "{child}: {status}");
            return;
        }
        if start.elapsed() > limit {
            process.kill().unwrap();
            process.wait().unwrap();
            panic!("{child} at size {size} took more than {limit:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The size [`bounded`] gives its child, which does nothing without it.
fn bounded_size() -> Option<usize> {
    Some(std::env::var("FLUX_SCALING_SIZE").ok()?.parse().unwrap())
}

/// `struct A0(A1, A1); ... struct A29(A30, A30);` is 31 types, but every
/// walk of `A0` that does not remember the types it met (the `Sync`
/// check of a static, whether dropping a value runs code) meets `A30`
/// 2^30 times. Both took 9 s at depth 24 in a release build.
#[test]
fn shared_field_types() {
    bounded(
        "shared_field_types_child",
        30,
        std::time::Duration::from_secs(20),
    );
}

#[test]
#[ignore = "run by shared_field_types in a bounded subprocess"]
fn shared_field_types_child() {
    let Some(depth) = bounded_size() else {
        return;
    };
    let mut text = format!("struct A{depth};\n");
    for i in (0..depth).rev() {
        text.push_str(&format!("struct A{i}(A{}, A{});\n", i + 1, i + 1));
    }
    text.push_str("static S: Option<A0> = None;\nfn main() { let _s = &S; }\n");
    build(&typed(text));
}

/// Each `Map` adapter's `Iterator` impl needs its inner iterator's, and
/// the borrow check's proofs of those bounds selected the inner impls
/// again for every candidate: 61 s for 14 adapters in a release build.
#[test]
fn nested_iterator_adapters() {
    bounded(
        "nested_iterator_adapters_child",
        14,
        std::time::Duration::from_secs(20),
    );
}

#[test]
#[ignore = "run by nested_iterator_adapters in a bounded subprocess"]
fn nested_iterator_adapters_child() {
    let Some(n) = bounded_size() else {
        return;
    };
    let adapters = ".map(|x| x + 1)".repeat(n);
    build(&typed(format!(
        "fn total(n: u32) -> u32 {{ (0..n){adapters}.sum() }}\nfn main() {{ let _t = total(3); }}\n"
    )));
}
