#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[path = "../../flux-cli/tests/support/process.rs"]
mod process;

use flux_codegen::{Compilation, OptimizationTier, OutputKind, Target};
use flux_hir::{CrateId, Hir, SourceCrate, SourceDb};
use flux_intern::Symbol;

const LIBRARY: &str = r#"
pub static mut COUNT: i32 = 3;
pub fn twice<T: Copy + std::ops::Add<Output = T>>(x: T) -> T { x + x }
pub fn answer() -> i32 { (unsafe { COUNT += 1; COUNT }) + twice(19) }
pub fn text() -> &'static str { "separate" }
#[track_caller]
pub fn caller_line() -> u32 { std::panic::Location::caller().line() }
pub trait Number { fn value(&self) -> i32; fn double(&self) -> i32 { self.value() * 2 } }
struct Value;
impl Number for Value { fn value(&self) -> i32 { 23 } }
pub fn object() -> &'static dyn Number { &Value }
pub fn mapped(x: i32) -> i32 { let f = |x| twice(x); f(x) }
pub fn generic_pointer() -> fn(i32) -> i32 { twice::<i32> }
pub struct Wrapped(pub i32);
pub fn identity<T>(x: T) -> T { x }
pub fn wrapped() -> i32 { identity(Wrapped(9)).0 }
pub const TRACKED: fn() -> u32 = caller_line;
pub fn narrow(x: i8) -> i8 { x.wrapping_sub(3) }
pub fn pair(x: i8) -> (i8, u16) { (x, 257) }
pub fn array(x: u8) -> [u8; 20] { [x; 20] }
"#;
const PROGRAM: &str = r#"
fn main() {
    let pointer: fn() -> i32 = a::answer;
    println!("{} {} {}", pointer(), a::twice(7), a::text());
    let tracked: fn() -> u32 = a::caller_line;
    println!("{} {}", a::caller_line(), tracked());
    println!("{} {} {}", a::object().value(), a::object().double(), a::mapped(5));
    let upstream = a::generic_pointer();
    let downstream: fn(i32) -> i32 = a::twice::<i32>;
    println!("{}", upstream as usize == downstream as usize);
    println!("{} {}", a::wrapped(), a::identity(a::Wrapped(4)).0);
    println!("{}", a::TRACKED());
    let narrow: fn(i8) -> i8 = a::narrow;
    let pair: fn(i8) -> (i8, u16) = a::pair;
    let array: fn(u8) -> [u8; 20] = a::array;
    let value = pair(-4);
    println!("{} {} {} {} {}", a::narrow(-4), narrow(-4), value.0, value.1, array(9)[19]);
}
"#;

fn graph(library_only: bool, padding: bool) -> (SourceDb, Hir, CrateId) {
    let mut db = SourceDb::new();
    let mut graph = Vec::new();
    if padding {
        let root = db.add("padding.rs", "pub struct Padding; pub fn unused() {}");
        graph.push(SourceCrate {
            root,
            name: Symbol::intern("padding"),
            dependencies: Vec::new(),
        });
    }
    let library = graph.len();
    let root = db.add("a.rs", LIBRARY);
    graph.push(SourceCrate {
        root,
        name: Symbol::intern("a"),
        dependencies: Vec::new(),
    });
    if !library_only {
        let root = db.add("b.rs", PROGRAM);
        graph.push(SourceCrate {
            root,
            name: Symbol::intern("b"),
            dependencies: vec![(Symbol::intern("a"), library)],
        });
    }
    let hir = flux_hir::lower_sources(&mut db, &graph);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let krate = hir
        .items
        .crates
        .iter()
        .position(|c| c.name.as_str() == "a")
        .unwrap();
    (db, hir, CrateId(krate as u32))
}

fn module(library_only: bool, padding: bool) -> keel_ir::Module {
    let (_db, hir, library) = graph(library_only, padding);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let hashes = [
        (library, [1; 32]),
        (
            hir.items.main_crate(),
            if library_only { [1; 32] } else { [2; 32] },
        ),
    ];
    flux_codegen::lower_crate(
        &hir,
        &typed,
        &mir,
        Compilation {
            kind: if library_only {
                OutputKind::Library(library)
            } else {
                OutputKind::Executable
            },
            crate_hashes: &hashes,
            builtin_object: false,
            imported: if library_only {
                &[]
            } else {
                std::slice::from_ref(&library)
            },
            required_instances: &[],
            required_functions: &[],
        },
    )
    .unwrap()
}

#[test]
fn library_symbols_survive_unrelated_arena_entries() {
    let first = module(true, false);
    let shifted = module(true, true);
    let names = |m: &keel_ir::Module| {
        let mut names: Vec<_> = m.functions.iter().map(|f| f.name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(names(&first), names(&shifted));
    let data_names = |m: &keel_ir::Module| {
        let mut names: Vec<_> = m.data.iter().map(|d| d.name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(data_names(&first), data_names(&shifted));
    assert!(first.functions.iter().all(|f| f.name != "main"));
    assert!(
        first.functions.iter().any(|f| f.weak_odr),
        "generic instances must coalesce"
    );
    for mut m in [first, shifted] {
        flux_codegen::compile_object(&mut m, Target::Aarch64AppleDarwin, OptimizationTier::None)
            .unwrap();
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn separate_objects_link_with_generics_statics_and_function_pointers() {
    for tier in [OptimizationTier::None, OptimizationTier::Full] {
        let a = flux_codegen::compile_object(
            &mut module(true, false),
            Target::Aarch64AppleDarwin,
            tier,
        )
        .unwrap();
        let b = flux_codegen::compile_object(
            &mut module(false, true),
            Target::Aarch64AppleDarwin,
            tier,
        )
        .unwrap();
        let path =
            std::env::temp_dir().join(format!("flux-separate-{}-{tier:?}", std::process::id()));
        let result = flux_codegen::link_objects(
            &[
                flux_codegen::ObjectInput {
                    name: "a.o",
                    bytes: &a,
                },
                flux_codegen::ObjectInput {
                    name: "b.o",
                    bytes: &b,
                },
            ],
            Target::Aarch64AppleDarwin,
            &path,
            &Default::default(),
        );
        result.unwrap();
        let output = execute(&path, &format!("basic-{tier:?}"));
        let _ = std::fs::remove_file(&path);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "42 14 separate\n6 7\n23 46 10\ntrue\n9 4\n7\n-7 -7 -4 257 9\n"
        );
    }
}

// Names, sources, direct edges and explicit crate fingerprints for one stage.
type TestCrate<'a> = (&'a str, &'a str, &'a [(&'a str, usize)], u8);

fn staged_module(sources: &[TestCrate<'_>], library: Option<usize>) -> keel_ir::Module {
    staged_lowered(sources, library, &[]).module
}

fn staged_lowered(
    sources: &[TestCrate<'_>],
    library: Option<usize>,
    required_instances: &[flux_ty::Instance],
) -> flux_codegen::LoweredCrate {
    let mut db = SourceDb::new();
    let roots: Vec<_> = sources
        .iter()
        .map(|(name, text, _, hash)| db.add(format!("stage-{name}-{hash}.rs"), *text))
        .collect();
    let graph: Vec<_> = sources
        .iter()
        .zip(&roots)
        .map(|((name, _, deps, _), &root)| SourceCrate {
            root,
            name: Symbol::intern(name),
            dependencies: deps
                .iter()
                .map(|&(name, index)| (Symbol::intern(name), index))
                .collect(),
        })
        .collect();
    let hir = flux_hir::lower_sources(&mut db, &graph);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let ids: Vec<_> = roots
        .iter()
        .map(|root| {
            let index = hir
                .items
                .crates
                .iter()
                .position(|c| hir.items.modules[c.root].file == *root)
                .unwrap();
            CrateId(index as u32)
        })
        .collect();
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let hashes: Vec<_> = ids
        .iter()
        .zip(sources)
        .map(|(&id, (_, _, _, hash))| (id, [*hash; 32]))
        .collect();
    let own = library.unwrap_or(sources.len() - 1);
    let imported: Vec<_> = ids
        .iter()
        .enumerate()
        .filter_map(|(i, &id)| (i != own).then_some(id))
        .collect();
    flux_codegen::lower_crate_with_requirements(
        &hir,
        &typed,
        &mir,
        Compilation {
            kind: library.map_or(OutputKind::Executable, |index| {
                OutputKind::Library(ids[index])
            }),
            crate_hashes: &hashes,
            builtin_object: false,
            imported: &imported,
            required_instances,
            required_functions: &[],
        },
    )
    .unwrap()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn execute(path: &std::path::Path, label: &str) -> std::process::Output {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/flib-codegen")
        .join(format!("{}-{label}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    process::run(
        &mut std::process::Command::new(path),
        &directory,
        "run",
        std::time::Duration::from_secs(30),
    )
    .completed()
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn run_objects(modules: &[keel_ir::Module], name: &str, expected: &str) {
    for tier in [OptimizationTier::None, OptimizationTier::Full] {
        let objects: Vec<_> = modules
            .iter()
            .map(|m| {
                flux_codegen::compile_object(&mut m.clone(), Target::Aarch64AppleDarwin, tier)
                    .unwrap()
            })
            .collect();
        let names: Vec<_> = (0..objects.len()).map(|i| format!("stage-{i}.o")).collect();
        let inputs: Vec<_> = objects
            .iter()
            .zip(&names)
            .map(|(bytes, name)| flux_codegen::ObjectInput { name, bytes })
            .collect();
        let path =
            std::env::temp_dir().join(format!("flux-{name}-{}-{tier:?}", std::process::id()));
        flux_codegen::link_objects(
            &inputs,
            Target::Aarch64AppleDarwin,
            &path,
            &Default::default(),
        )
        .unwrap();
        let output = execute(&path, &format!("{name}-{tier:?}"));
        let _ = std::fs::remove_file(&path);
        assert!(output.status.success(), "{name} {tier:?}: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected,
            "{name} {tier:?}"
        );
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn transitive_objects_keep_private_helpers_and_embedded_static_callbacks() {
    let a = r#"
use std::sync::atomic::{AtomicU32, Ordering};
fn private(x: i32) -> i32 { x + 1 }
pub fn exposed() -> i32 { private(7) }
pub fn identity<T>(x: T) -> T { x }
pub static CORE_CALLBACK: fn(u64) -> u32 = u64::count_ones;
pub const CORE_CONST: fn(u64) -> u32 = u64::count_zeros;
pub static CALLBACKS: [fn(i32) -> i32; 2] = [private, identity::<i32>];
pub static FIRST: AtomicU32 = AtomicU32::new(7);
pub static SECOND: AtomicU32 = AtomicU32::new(7);
pub fn increment() { FIRST.fetch_add(1, Ordering::Relaxed); }
pub fn loads() -> (u32, u32) { (FIRST.load(Ordering::Relaxed), SECOND.load(Ordering::Relaxed)) }
// This definition is emitted even though the executable does not call it.
pub fn unused_runtime() -> u32 { std::process::id() }
"#;
    let b = r#"
fn helper(x: i32) -> i32 { upstream::CORE_CALLBACK(x as u64) as i32 + 1 }
pub fn compute() -> i32 { helper(15) + upstream::exposed() }
pub fn forward<T>(x: T) -> T { upstream::identity(x) }
pub fn pointer() -> fn(u64) -> u32 { upstream::CORE_CALLBACK }
"#;
    let c = r#"
fn main() {
    println!("{} {} {}", middle::compute(), middle::forward(9), middle::pointer()(255));
    println!("{} {} {}", base::CORE_CONST(0), base::CALLBACKS[0](5), base::CALLBACKS[1](5));
    base::increment();
    let (first, second) = base::loads();
    println!("{} {}", first, second);
}
"#;
    let unused = r#"
pub static UNRELATED: i32 = 51;
fn private(x: i32) -> i32 { x + 51 }
fn callback<T>(x: T) -> i32 { private(1) }
pub static UNRELATED_CALLBACK: fn(i32) -> i32 = callback::<i32>;
pub fn unused() -> i32 { 51 }
"#;
    let first = staged_module(
        &[("a", a, &[], 11), ("unrelated", unused, &[], 44)],
        Some(0),
    );
    let second = staged_module(
        &[("a", a, &[], 11), ("b", b, &[("upstream", 0)], 22)],
        Some(1),
    );
    let unrelated = staged_module(&[("unrelated", unused, &[], 44)], Some(0));
    let executable = staged_module(
        &[
            ("unrelated", unused, &[], 44),
            ("a", a, &[], 11),
            ("b", b, &[("upstream", 1)], 22),
            ("c", c, &[("base", 1), ("middle", 2)], 33),
        ],
        None,
    );
    assert!(
        !first
            .data
            .iter()
            .any(|d| d.init.is_some() && d.name.contains("UNRELATED"))
    );
    assert!(
        !first.functions.iter().any(|f| f.name.contains("callback")),
        "unrelated static callbacks must not become library roots"
    );
    assert!(
        !second
            .data
            .iter()
            .any(|d| d.init.is_some() && d.name.contains("FIRST"))
    );
    run_objects(
        &[first, second, unrelated, executable],
        "transitive",
        "13 9 8\n64 6 5\n8 7\n",
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn same_named_libraries_keep_static_and_promoted_relocations_distinct() {
    let template = r#"
pub static ITEM: i32 = 7;
pub fn value() -> i32 { __VALUE__ }
pub static TABLE: (&'static i32, fn() -> i32) = (&ITEM, value);
pub const PROMOTED: (&'static i32, fn() -> i32) = (&ITEM, value);
pub fn promoted() -> &'static (&'static i32, fn() -> i32) { &PROMOTED }
"#;
    let one = template.replace("__VALUE__", "11");
    let two = template.replace("__VALUE__", "22");
    let main = r#"
fn main() {
    let first = &left::ITEM as *const i32 as usize;
    let second = &right::ITEM as *const i32 as usize;
    println!("{} {} {}", left::value(), right::value(), first != second);
    println!("{} {} {} {}", *left::TABLE.0, left::TABLE.1(), *right::TABLE.0, right::TABLE.1());
    let first = left::promoted();
    let second = right::promoted();
    println!("{} {} {} {}", *first.0, first.1(), *second.0, second.1());
}
"#;
    let a = staged_module(&[("shared", &one, &[], 51)], Some(0));
    let b = staged_module(&[("shared", &two, &[], 52)], Some(0));
    let executable = staged_module(
        &[
            ("shared", &one, &[], 51),
            ("shared", &two, &[], 52),
            ("root", main, &[("left", 0), ("right", 1)], 53),
        ],
        None,
    );
    let names = |m: &keel_ir::Module| {
        m.data
            .iter()
            .filter(|d| d.name.contains("__flux_promoted") && d.init.is_some())
            .map(|d| d.name.clone())
            .collect::<Vec<_>>()
    };
    assert!(
        names(&a).iter().all(|name| !names(&b).contains(name)),
        "promotions contain different stable relocation targets"
    );
    run_objects(
        &[a, b, executable],
        "same-name",
        "11 22 true\n7 11 7 22\n7 11 7 22\n",
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn included_definitions_keep_symbols_across_include_coordinate_intervals() {
    let directory =
        std::env::temp_dir().join(format!("flux-included-symbols-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("library.rs"),
        r#"
pub fn answer() -> i32 { fn private() -> i32 { 37 } private() }
pub fn identity<T>(x: T) -> T { x }
"#,
    )
    .unwrap();
    std::fs::write(
        directory.join("padding.rs"),
        "pub fn padding() -> i32 { 8 }",
    )
    .unwrap();
    let library = format!("include!({:?});", directory.join("library.rs"));
    let padding = format!("include!({:?});", directory.join("padding.rs"));
    let program = "fn main() { println!(\"{} {}\", a::answer(), a::identity(5)); }";
    let first = staged_module(&[("a", &library, &[], 61)], Some(0));
    let shifted = staged_module(
        &[("padding", &padding, &[], 62), ("a", &library, &[], 61)],
        Some(1),
    );
    let names = |m: &keel_ir::Module| {
        let mut names: Vec<_> = m.functions.iter().map(|f| f.name.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(
        names(&first),
        names(&shifted),
        "include namespace allocation must not enter symbols"
    );
    let padding_object = staged_module(&[("padding", &padding, &[], 62)], Some(0));
    let executable = staged_module(
        &[
            ("padding", &padding, &[], 62),
            ("a", &library, &[], 61),
            ("root", program, &[("a", 1)], 63),
        ],
        None,
    );
    run_objects(
        &[first, padding_object, executable],
        "include-symbols",
        "37 5\n",
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn explicit_required_roots_preserve_entry_point_and_skip_generic_definitions() {
    let (_db, _root, hir) = flux_hir::lower_text(
        "required.rs",
        r#"
fn required() -> u32 { 15u64.count_ones() }
fn recursive<T>() { recursive::<(T, T)>(); }
fn main() {}
"#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let required: Vec<_> = hir
        .items
        .functions
        .iter()
        .filter_map(|(id, f)| matches!(f.name.as_str(), "required" | "recursive").then_some(id))
        .collect();
    assert_eq!(required.len(), 2);
    let hashes = [(hir.items.main_crate(), [71; 32])];
    let mut module = flux_codegen::lower_crate(
        &hir,
        &typed,
        &mir,
        Compilation {
            kind: OutputKind::Executable,
            crate_hashes: &hashes,
            builtin_object: false,
            imported: &[],
            required_instances: &[],
            required_functions: &required,
        },
    )
    .unwrap();
    assert!(module.functions.iter().any(|f| f.name == "main"));
    assert!(module.functions.iter().any(|f| f.name.contains("required")));
    assert!(
        !module
            .functions
            .iter()
            .any(|f| f.name.contains("recursive"))
    );
    assert!(
        module
            .functions
            .iter()
            .any(|f| f.name.contains("flush_stdout")),
        "additional roots must preserve runtime termination roots"
    );
    flux_codegen::compile_object(
        &mut module,
        Target::Aarch64AppleDarwin,
        OptimizationTier::None,
    )
    .unwrap();
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn exported_interfaces_retain_helpers_without_instantiating_dead_private_code() {
    let library = r#"
fn recursive<T>() { recursive::<(T, T)>(); }
fn unused() { recursive::<i32>(); }
static UNUSED_CALLBACK: fn() = unused;
struct PrivateLeak;
impl PrivateLeak { pub fn dead_method() { recursive::<u8>(); } }
pub fn returns_private() -> PrivateLeak { PrivateLeak }
mod hidden {
    fn helper(x: i32) -> i32 { x + 2 }
    pub fn expose() -> i32 { helper(3) }
    pub fn trap() { super::recursive::<u8>(); }
    pub static TRAP: fn() = trap;
    pub struct Handle { value: i32 }
    impl Handle {
        pub fn new(value: i32) -> Self { Self { value } }
        pub fn value(&self) -> i32 { self.value }
    }
    pub struct Returned { value: i32 }
    impl Returned { pub fn read(&self) -> i32 { self.value } }
    pub fn make() -> Returned { Returned { value: 13 } }
    pub fn generic<T>(x: T) -> (T, i32) { (x, helper(7)) }
}
pub use hidden::{expose, generic};
pub type Alias = hidden::Handle;
pub fn returned() -> hidden::Returned { hidden::make() }
pub trait Transform { fn convert(&self) -> i32; }
impl Transform for i32 { fn convert(&self) -> i32 { *self + 100 } }
pub struct Bound<T: Transform> { pub value: T }
impl<T: Transform> Bound<T> { pub fn convert(&self) -> i32 { self.value.convert() } }
trait Local { fn read(&self) -> i32; }
struct Private;
static PRIVATE_STATE: i32 = 19;
impl Local for Private { fn read(&self) -> i32 { PRIVATE_STATE } }
fn route<T: Local>(x: T) -> i32 { x.read() }
pub fn nested<T>(_: T) -> i32 { route(Private) }
const fn const_helper() -> u32 { 29 }
pub const READY: u32 = const_helper();
fn callback_helper(x: i32) -> i32 { x + 3 }
static CALLBACK: fn(i32) -> i32 = callback_helper;
pub fn callback<T>(x: T) -> (T, i32) { (x, CALLBACK(9)) }
pub enum Choice { Some(i32), None }
struct Hidden;
pub trait Visible { fn get(&self) -> i32; }
impl Visible for Hidden { fn get(&self) -> i32 { 33 } }
impl std::fmt::Display for Hidden {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { recursive::<i32>(); Ok(()) }
}
pub fn hidden() -> impl Visible { Hidden }
struct Dropper;
fn drop_helper() {}
impl Drop for Dropper { fn drop(&mut self) { drop_helper(); } }
impl Visible for Dropper { fn get(&self) -> i32 { 44 } }
static DROPPER: Dropper = Dropper;
pub fn borrowed() -> &'static dyn Visible { &DROPPER }
pub trait ChoiceFor<T> { fn choose(&self) -> i32; }
struct Selector;
impl ChoiceFor<u32> for Selector { fn choose(&self) -> i32 { 55 } }
impl ChoiceFor<u64> for Selector { fn choose(&self) -> i32 { recursive::<i32>(); 0 } }
fn dispatch<T: ChoiceFor<u32>>(x: T) -> i32 { x.choose() }
pub fn selected<T>(_: T) -> i32 { dispatch(Selector) }
"#;
    let program = r#"
fn main() {
    let handle = a::Alias::new(11);
    let (value, helper) = a::generic(7u8);
    println!("{} {} {} {} {}", a::expose(), handle.value(), a::returned().read(), value, helper);
    println!("{} {} {} {}", a::Bound { value: 5i32 }.convert(), a::nested(1u8), a::READY, a::callback(1u8).1);
    let constructor: fn(i32) -> a::Choice = a::Choice::Some;
    let choice = constructor(31);
    match choice { a::Choice::Some(value) => println!("{}", value), a::Choice::None => println!("none") }
    use a::Visible;
    println!("{} {} {}", a::hidden().get(), a::borrowed().get(), a::selected(1u8));
}
"#;
    let a = staged_module(&[("a", library, &[], 81)], Some(0));
    assert!(!a.functions.iter().any(|f| f.name.contains("unused")
        || f.name.contains("recursive")
        || f.name.contains("trap")
        || f.name.contains("const_helper")
        || f.name.contains("dead_method")));
    assert!(
        !a.data
            .iter()
            .any(|d| d.init.is_some() && (d.name.contains("UNUSED") || d.name.contains("TRAP")))
    );
    assert!(
        a.data
            .iter()
            .any(|d| d.init.is_some() && d.name.contains("PRIVATE_STATE"))
    );
    let executable = staged_module(
        &[("a", library, &[], 81), ("root", program, &[("a", 0)], 82)],
        None,
    );
    run_objects(
        &[a, executable],
        "reachable-exports",
        "5 11 13 7 9\n105 19 29 12\n31\n33 44 55\n",
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn exported_templates_follow_dependency_generic_methods_objects_and_constants() {
    let dependency = r#"
pub trait Value { fn value(&self) -> i32; }
pub fn read<T: Value>(x: T) -> i32 { x.value() }
pub fn object<T: Value>(x: &T) -> &dyn Value { x }
pub trait Callback { const FUNCTION: fn(i32) -> i32; }
pub fn constant<T: Callback>() -> fn(i32) -> i32 { T::FUNCTION }
"#;
    // Each case requires a different symbolic dependency operation. Keeping
    // them in separate libraries prevents another export from retaining it.
    for (index, operation) in [
        "dep::read(Private)",
        "dep::object(&Private).value()",
        "dep::constant::<Private>()(7)",
    ]
    .iter()
    .enumerate()
    {
        let library = format!(
            r#"
struct Private;
static PRIVATE_STATE: i32 = 41;
impl dep::Value for Private {{ fn value(&self) -> i32 {{ PRIVATE_STATE }} }}
fn callback(x: i32) -> i32 {{ x + 5 }}
impl dep::Callback for Private {{ const FUNCTION: fn(i32) -> i32 = callback; }}
pub fn exported<T>(_: T) -> i32 {{ {operation} }}
"#
        );
        let executable = "fn main() { println!(\"{}\", a::exported(1u8)); }";
        let d = staged_module(&[("dep", dependency, &[], 91)], Some(0));
        let a = staged_module(
            &[
                ("dep", dependency, &[], 91),
                ("a", &library, &[("dep", 0)], 92),
            ],
            Some(1),
        );
        let b = staged_module(
            &[
                ("dep", dependency, &[], 91),
                ("a", &library, &[("dep", 0)], 92),
                ("root", executable, &[("a", 1)], 93),
            ],
            None,
        );
        run_objects(
            &[d, a, b],
            &format!("dependency-template-{index}"),
            if index == 2 { "12\n" } else { "41\n" },
        );
    }
}

#[test]
fn forty_level_constant_allocation_dags_keep_shared_storage() {
    use flux_mir::eval::Value;
    use flux_ty_ir::{Mutability, Ty, TyKind};
    use std::sync::Arc;

    const DEPTH: usize = 40;
    // Construct checked IR directly: resolving a source tree of duplicated
    // type aliases would test alias expansion rather than storage sharing.
    let mut source = String::from("fn callback(x: i32) -> i32 { x + 1 }\n");
    for level in 0..DEPTH {
        source.push_str(&format!("const V{level}: fn(i32) -> i32 = callback;\n"));
    }
    source.push_str("pub static ROOT: fn(i32) -> i32 = callback;\n");
    let (_db, _root, hir) = flux_hir::lower_text("allocation-dag.rs", &source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let mut typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mut mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let krate = hir.items.main_crate();
    let callback = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "callback")
        .unwrap()
        .0;
    let find = |name: &str| {
        hir.items
            .consts
            .iter()
            .find(|(_, c)| c.name.is_some_and(|n| n.as_str() == name))
            .unwrap()
            .0
    };
    let root = find("ROOT");
    let mut ty = typed.sigs.consts[root];
    let mut value = Value::Function(flux_ty::Instance {
        function: callback,
        args: &[],
    });
    for level in 0..DEPTH {
        let id = find(&format!("V{level}"));
        typed.sigs.consts.insert(id, ty);
        mir.const_values.items.insert(id, value);
        let reference = Ty::new(TyKind::Ref(Mutability::Not, ty));
        ty = Ty::tuple(&[reference, reference]);
        value = Value::Aggregate(vec![Value::Static(id), Value::Static(id)].into());
    }
    typed.sigs.consts.insert(root, ty);
    mir.const_values.items.insert(root, value);
    let hashes = [(krate, [96; 32])];
    let compilation = Compilation {
        kind: OutputKind::Library(krate),
        crate_hashes: &hashes,
        builtin_object: false,
        imported: &[],
        required_instances: &[],
        required_functions: &[],
    };
    let named = flux_codegen::lower_crate(&hir, &typed, &mir, compilation).unwrap();
    let mut value = Value::Function(flux_ty::Instance {
        function: callback,
        args: &[],
    });
    for _ in 0..DEPTH {
        let child = Arc::new(value);
        value = Value::Aggregate(vec![Value::Ref(child.clone()), Value::Ref(child)].into());
    }
    mir.const_values.items.insert(root, value);
    let shared = flux_codegen::lower_crate(&hir, &typed, &mir, compilation).unwrap();
    assert_eq!(
        named.data, shared.data,
        "allocation identity must not enter emitted data or relocations"
    );
    for module in [&named, &shared] {
        assert_eq!(
            module.data.iter().filter(|d| d.init.is_some()).count(),
            DEPTH + 1
        );
        assert_eq!(
            module
                .functions
                .iter()
                .filter(|f| f.name.contains("callback"))
                .count(),
            1
        );
        flux_codegen::compile_object(
            &mut module.clone(),
            Target::Aarch64AppleDarwin,
            OptimizationTier::None,
        )
        .unwrap();
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn object_requirements_keep_concrete_embedded_instances() {
    let library = r#"
#[derive(Clone)]
pub struct Wrapper(pub u32);
pub fn cloned(value: u32) -> u32 { Wrapper(value).clone().0 }
pub fn initialized() -> std::sync::OnceLock<u32> { Default::default() }
pub fn characters<I: IntoIterator<Item = char>>(iter: I) -> String {
    let mut string = String::new();
    string.extend(iter);
    string
}
fn dead<T>() { dead::<(T, T)>(); }
"#;
    // The executable never calls the non-generic exports. Their object still
    // requires concrete embedded calls from both function bodies. The generic
    // export also checks deferred foreign adapters after concrete substitution.
    let sources = [
        ("a", library, &[][..], 101),
        (
            "root",
            "fn main() { assert_eq!(a::characters(['o', 'k']), \"ok\"); println!(\"42\"); }",
            &[("a", 0)][..],
            102,
        ),
    ];
    let a = staged_lowered(&sources, Some(0), &[]);
    assert!(!a.required_instances.is_empty());
    assert!(
        a.required_instances
            .iter()
            .any(|instance| !instance.args.is_empty())
    );
    assert!(
        !a.module
            .functions
            .iter()
            .any(|function| function.name.contains("dead"))
    );
    let b = staged_lowered(&sources, None, &a.required_instances);
    for declaration in &a.module.decls {
        if declaration.name.contains("assert_param_is_clone")
            || declaration.name.contains("once_lock")
        {
            assert!(
                b.module
                    .functions
                    .iter()
                    .any(|function| function.name == declaration.name),
                "missing {}",
                declaration.name
            );
        }
    }
    run_objects(
        &[a.module, b.module],
        "concrete-object-requirements",
        "42\n",
    );
}

#[test]
fn hidden_strong_definitions_allow_inlining_without_changing_linkage() {
    let mut module = staged_module(
        &[(
            "a",
            "pub fn leaf(x: u64) -> u64 { x + 1 } pub fn combined(x: u64) -> u64 { leaf(x) * 2 }",
            &[],
            111,
        )],
        Some(0),
    );
    let leaf = module
        .functions
        .iter()
        .find(|f| f.name.contains("4leaf"))
        .unwrap()
        .name
        .clone();
    let calls_leaf = |function: &keel_ir::Function| {
        function.layout_insts().any(|(_, inst)| {
        matches!(function.inst_data(inst), keel_ir::InstData::Call { func, .. } if function.callee(*func).name == leaf)
    })
    };
    assert!(module.functions.iter().any(calls_leaf));
    assert!(
        module
            .functions
            .iter()
            .all(|f| f.hidden && !f.local && f.attrs.noninterposable)
    );
    keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap();
    assert!(!module.functions.iter().any(calls_leaf));
    assert!(
        module
            .functions
            .iter()
            .any(|f| f.name == leaf && !f.local && !f.weak_odr)
    );
}

const WEAK_ODR_LIBRARY: &str = "
pub fn twice<T: Copy + std::ops::Add<Output = T>>(x: T) -> T { x + x }
pub fn combined(x: u64) -> u64 { twice(x) * 2 }
pub fn pointer() -> fn(u64) -> u64 { twice::<u64> }
";

#[test]
fn weak_odr_instances_allow_inlining_without_changing_linkage() {
    let mut module = staged_module(&[("a", WEAK_ODR_LIBRARY, &[], 112)], Some(0));
    let instance = module
        .functions
        .iter()
        .find(|f| f.weak_odr && f.name.contains("twice"))
        .unwrap();
    // Another object's copy may win at link time, so the instance promises an
    // equivalent body rather than an exact binding.
    assert!(instance.hidden && !instance.local);
    assert!(instance.attrs.no_semantic_interposition && !instance.attrs.noninterposable);
    let instance = instance.name.clone();
    let calls_instance = |function: &keel_ir::Function| {
        function.layout_insts().any(|(_, inst)| {
        matches!(function.inst_data(inst), keel_ir::InstData::Call { func, .. } if function.callee(*func).name == instance)
    })
    };
    assert!(module.functions.iter().any(calls_instance));
    keel_opt::optimize_with_tier(&mut module, OptimizationTier::Full).unwrap();
    assert!(!module.functions.iter().any(calls_instance));
    assert!(
        module
            .functions
            .iter()
            .any(|f| f.name == instance && f.weak_odr && f.hidden && !f.local)
    );
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn inlined_weak_odr_instances_keep_one_address_across_objects() {
    let root = "
fn main() {
    let mut total = 0u64;
    let mut i = 0u64;
    while i < 1000 { total = total.wrapping_add(a::twice(i)); i += 1; }
    let local: fn(u64) -> u64 = a::twice::<u64>;
    println!(\"{} {} {}\", total, a::combined(5), a::pointer() as usize == local as usize);
}
";
    let sources = [
        ("a", WEAK_ODR_LIBRARY, &[][..], 113),
        ("root", root, &[("a", 0)][..], 114),
    ];
    let a = staged_lowered(&sources, Some(0), &[]);
    let b = staged_lowered(&sources, None, &a.required_instances);
    run_objects(
        &[a.module, b.module],
        "weak-odr-inlining",
        "999000 20 true\n",
    );
}

#[test]
fn native_builtin_suite_owns_prefix_definitions_and_shared_generic_addresses() {
    let (_, _, mut hir) = flux_hir::lower_text_bare(
        "builtin-prefix.rs",
        r#"
unsafe extern "C" { fn abort() -> !; }
fn identity<T>(value: T) -> T { value }
fn unused() -> u64 { 99 }
pub fn pointer() -> fn(u64) -> u64 { identity::<u64> }
pub static VALUE: u64 = 42;
pub static CALLBACK: fn(u64) -> u64 = identity::<u64>;
fn main() {
    let a = pointer();
    let b: fn(u64) -> u64 = identity::<u64>;
    if a as usize != b as usize || VALUE != 42 || CALLBACK(7) != 7 {
        unsafe { abort(); }
    }
}
"#,
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mut mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let main = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "main")
        .unwrap()
        .0;
    // The bare fixture stands in for a decoded trusted prefix. Its source main
    // follows every builtin function; no source body is consulted by codegen.
    hir.items.core = hir.items.counts();
    hir.items.core.functions = main.into_raw();
    let hashes = [(hir.items.main_crate(), [112; 32])];
    let compilation = Compilation {
        kind: OutputKind::BuiltinSuite,
        crate_hashes: &hashes,
        imported: &[],
        builtin_object: false,
        required_functions: &[],
        required_instances: &[],
    };
    let suite =
        flux_codegen::lower_crate_with_requirements(&hir, &typed, &mir, compilation).unwrap();
    assert!(suite.required_instances.is_empty());
    assert!(!suite.module.functions.iter().any(|f| f.name == "main"));
    assert!(
        suite
            .module
            .functions
            .iter()
            .any(|f| f.name.contains("6unused"))
    );
    assert_eq!(
        suite
            .module
            .functions
            .iter()
            .filter(|f| !f.weak_odr)
            .count(),
        2
    );
    assert!(suite.module.functions.iter().any(|f| f.weak_odr));
    assert_eq!(
        suite
            .module
            .data
            .iter()
            .filter(|d| d.init.is_some())
            .count(),
        2
    );
    let consumer = flux_codegen::lower_crate(
        &hir,
        &typed,
        &mir,
        Compilation {
            kind: OutputKind::Executable,
            builtin_object: true,
            ..compilation
        },
    )
    .unwrap();
    assert!(
        !consumer
            .functions
            .iter()
            .any(|f| f.name.contains("7pointer"))
    );
    assert!(consumer.functions.iter().any(|f| f.weak_odr));
    assert!(consumer.data.iter().all(|d| d.init.is_none()));
    assert!(consumer.data.iter().any(|d| d.name.contains("VALUE")));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    run_objects(&[suite.module, consumer], "builtin-prefix", "");
    let value_id = hir
        .items
        .consts
        .iter()
        .find(|(_, c)| c.name.is_some_and(|name| name.as_str() == "VALUE"))
        .unwrap()
        .0;
    let value = mir.const_values.items.remove(value_id).unwrap();
    let errors = flux_codegen::lower_crate(&hir, &typed, &mir, compilation)
        .err()
        .unwrap();
    assert_eq!(errors[0].message, "missing checked built-in static value");
    mir.const_values.items.insert(value_id, value);
    mir.bodies.remove(
        hir.items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "unused")
            .unwrap()
            .0,
    );
    let errors = flux_codegen::lower_crate(&hir, &typed, &mir, compilation)
        .err()
        .unwrap();
    assert_eq!(errors[0].message, "missing checked built-in MIR");
}
