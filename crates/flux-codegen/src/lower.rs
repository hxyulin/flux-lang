//! MIR to keel IR.
//!
//! keel has no aggregate values, so a tuple or struct is split into its
//! scalar *leaves*, in field order, each a keel variable of its own: a
//! local of type `(i32, (bool, u64))` is three variables, and a field is a
//! range of its local's leaves. A local that is borrowed needs an address
//! instead, so it lives in a stack slot, laid out as rustc lays its type
//! out ([`flux_mir::layout`]); so does whatever a pointer points to.
//! Small Rust aggregates cross calls as one argument per leaf; larger
//! scalar aggregates use a private by-value snapshot after sixteen leaves. A function returning
//! two or more leaves returns them as keel results when each gets a register
//! of its own (`abi::register_results`), and otherwise through memory the
//! caller provides (`sret`), laid out as a C struct of the leaves, which keel
//! returns in registers where the platform allows. Native C signatures instead describe
//! aggregates with their physical layout through Keel `ByVal` and `Sret`.
//!
//! An enum's leaves are its discriminant, of its `repr` type (`isize`
//! without one), then the fields of each of its variants in turn: only
//! the variant it holds has meaningful leaves. In memory, its variants
//! share their bytes, so storing one stores only the fields of the
//! variant its discriminant says (a `switch` where several variants have
//! fields), then its tag, which loading decodes: a direct tag is
//! extended, a niche (`Option<&T>`'s null) compared with the values it
//! gives other variants.
//!
//! A value that contains an array (a *memory* type) is indexed at run
//! time, so it always lives in memory: its locals are stack slots, and
//! its value is the address of one, whose bytes are copied where it is
//! assigned. It crosses calls as that address, which the callee copies,
//! and is returned through `sret` memory laid out as the value is. A
//! reference to a slice is two leaves, the address of its first element
//! and its length.

mod abi;
mod reachability;
mod shapes;
mod unwind;
use abi::{
    Argument, Convention, Return, attrs, c_signature, check_c_abi, ctx_signature, signature,
};
use unwind::Call;
pub(crate) use unwind::PERSONALITY as UNWIND_PERSONALITY;

use std::sync::Arc;

use flux_arena::{SliceId, SlicePool};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};

use flux_hir::{Diagnostic, FunctionId, Hir, ImplTarget, Owner};
use flux_mir::layout::{Layouts, Shape, TagEncoding};
use flux_mir::mono::Instance;
use flux_mir::{
    AggregateKind, AssertMessage, BinOp, Body, Constant, FileMir, Local, Operand, Place, PlaceTy,
    ProjectionElem, Rvalue, StatementKind, TerminatorKind, UnOp,
};
use flux_ty::{FnSig, GenericArgs, IntTy, Mutability, Signatures, Ty, TyKind, TypedFile};
use flux_ty_ir::query_identity;
use flux_ty_ir::type_identity::DefinitionKeys;
use keel_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use keel_ir::ConvertOp;
use keel_ir::{
    AbiParam, ArgExt, Block, BlockCall, CallConv, DataInit, DataItem, DataObject, FuncDecl,
    FuncRef, Function, FunctionAttrs, IntCC, Intrinsic, Layout, MemoryFlags, Module, Purpose,
    Signature, Type, Value,
};

// Scalar values and fat pointers stay on the stack; large aggregates spill.
type Values = SmallVec<[Value; 4]>;
type TypeLeaves = SmallVec<[Ty; 4]>;

/// The keel module for a type-checked program and its MIR, `core`
/// included: one function per instance of a function the program needs.
/// An error if a generic function needs instances of itself without end.
pub fn lower_module(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
) -> Result<Module, Vec<Diagnostic>> {
    lower_inner(hir, typed, mir, None, None, SymbolIdentity::Artifact).map(|lowered| lowered.module)
}

/// Time spent in parts of lowering, for `--timings`. Each field sums its part
/// over all instances; lowering MIR to keel IR is the rest of the call.
#[derive(Default, Debug)]
pub struct LoweringTimings {
    /// Reachability and monomorphization: finding every instance to emit.
    pub collect: std::time::Duration,
    /// Substituting each instance's generic arguments into its MIR body.
    pub substitute: std::time::Duration,
    /// Checking each substituted body with the MIR validator.
    pub validate: std::time::Duration,
}

/// [`lower_module`] or [`lower_crate`], recording [`LoweringTimings`].
///
/// # Errors
///
/// Returns the same diagnostics as the untimed entry points.
pub fn lower_with_timings(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
    compilation: Option<Compilation<'_>>,
    timings: &mut LoweringTimings,
) -> Result<Module, Vec<Diagnostic>> {
    lower_inner(
        hir,
        typed,
        mir,
        compilation,
        Some(timings),
        SymbolIdentity::Artifact,
    )
    .map(|lowered| lowered.module)
}

/// Which crate an object defines. Dependencies supply their own non-generic code.
#[derive(Debug, Clone, Copy)]
pub enum OutputKind {
    /// Emits the entry point, local code and required embedded-library code.
    Executable,
    /// Emits this crate's non-generic definitions without a C entry point.
    Library(flux_hir::CrateId),
    /// Emits every monomorphic function and static in the trusted built-in prefix.
    BuiltinSuite,
}

/// Stable identities and ownership for a separately compiled object.
#[derive(Debug, Clone, Copy)]
pub struct Compilation<'a> {
    /// Selects the owning library crate or the executable entry point.
    pub kind: OutputKind,
    /// Exact fingerprints for source and loaded crates, shared by all objects.
    /// Artifact compatibility and freshness must already have been checked.
    pub crate_hashes: &'a [(flux_hir::CrateId, [u8; 32])],
    /// Crates whose non-generic definitions come from separately linked objects.
    /// Each listed crate's object must be supplied to [`crate::link_objects`].
    pub imported: &'a [flux_hir::CrateId],
    /// A separately linked trusted suite supplies built-in non-generic definitions.
    /// Concrete generic instances still use shared weak ODR linkage.
    pub builtin_object: bool,
    /// Functions required by loaded MIR, shared with type and MIR checking.
    /// Only monomorphic bodies become explicit roots; generic instances are
    /// collected from calls with their concrete arguments.
    pub required_functions: &'a [FunctionId],
    /// Concrete calls required by dependency objects, retaining generic arguments.
    /// These come from each object's actual monomorphization traversal.
    pub required_instances: &'a [Instance],
}

/// One lowered object and the concrete definitions it expects other objects to supply.
pub struct LoweredCrate {
    pub module: Module,
    pub required_instances: Vec<Instance>,
}

/// Lowers one object using stable crate identities and a fixed Rust convention.
///
/// `hir`, `typed` and `mir` must contain the checked source crate and its loaded
/// dependencies. Imported MIR is traversed to discover monomorphizations and
/// embedded-library requirements, while its non-generic definitions stay in the
/// owning object's code. Generic definitions use hidden, coalescible linkage.
///
/// This currently requires imported non-generic MIR as well as generic templates;
/// it does not read archives or infer missing dependency bodies from source.
///
/// # Errors
///
/// Returns diagnostics for invalid native signatures, monomorphization failures
/// and constant evaluation failures discovered while lowering concrete instances.
pub fn lower_crate(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
    compilation: Compilation<'_>,
) -> Result<Module, Vec<Diagnostic>> {
    lower_crate_with_requirements(hir, typed, mir, compilation).map(|lowered| lowered.module)
}

/// Lowers an object and records its concrete dependencies before ownership filtering.
///
/// Persist `required_instances` with the owning object. Consumers add them to
/// [`Compilation::required_instances`] so embedded generic code needed only by
/// dependency objects is emitted with its original concrete arguments.
///
/// # Errors
///
/// Returns the same checking and lowering diagnostics as [`lower_crate`].
pub fn lower_crate_with_requirements(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
    compilation: Compilation<'_>,
) -> Result<LoweredCrate, Vec<Diagnostic>> {
    lower_inner(
        hir,
        typed,
        mir,
        Some(compilation),
        None,
        SymbolIdentity::Artifact,
    )
}

/// Lowers a fast-build program with edit-stable source symbols and the reified Rust ABI.
///
/// Source-owned definitions use query identities; imported and built-in symbols
/// retain artifact identities. These source objects must not be mixed with source
/// objects lowered through [`lower_crate`]. Unsupported query identities fall back
/// to artifact identities, so some edits can still invalidate unrelated units.
/// Runtime type IDs retain their existing artifact identity.
///
/// When no external object supplies definitions, generated instances have one
/// owner in this program and need no weak ODR sections. Ordinary weak symbols
/// retain their binding. Imported and built-in object contracts remain unchanged.
///
/// # Errors
/// Returns the same diagnostics as [`lower_crate_with_requirements`].
pub fn lower_partitioned_crate(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
    compilation: Compilation<'_>,
) -> Result<LoweredCrate, Vec<Diagnostic>> {
    let mut lowered = lower_inner(
        hir,
        typed,
        mir,
        Some(compilation),
        None,
        SymbolIdentity::Query,
    )?;
    if matches!(compilation.kind, OutputKind::Executable)
        && compilation.imported.is_empty()
        && !compilation.builtin_object
    {
        for function in &mut lowered.module.functions {
            if function.weak_odr {
                function.weak_odr = false;
                function.attrs.noninterposable = function.hidden && !function.attrs.weak;
            }
        }
        for object in &mut lowered.module.data {
            object.weak_odr = false;
        }
        lowered.module.sync_function_attrs();
    }
    Ok(lowered)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SymbolIdentity {
    Artifact,
    Query,
}

fn lower_inner(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
    compilation: Option<Compilation<'_>>,
    mut timings: Option<&mut LoweringTimings>,
    identity: SymbolIdentity,
) -> Result<LoweredCrate, Vec<Diagnostic>> {
    // Exceptional bodies cannot silently use abort-mode object emission.
    if mir.panic_strategy == flux_mir::PanicStrategy::Abort
        && let Some(body) = mir
            .bodies
            .iter()
            .map(|(_, body)| body)
            .chain(mir.closures.values())
            .chain(mir.consts.iter().map(|(_, body)| body))
            .find(|body| body.has_unwind())
    {
        let range = body
            .blocks
            .values()
            .find(|block| {
                block.terminator.unwind != flux_mir::UnwindAction::Unreachable
                    || matches!(
                        block.terminator.kind,
                        TerminatorKind::UnwindResume | TerminatorKind::UnwindTerminate
                    )
            })
            .expect("has_unwind found an exceptional terminator")
            .terminator
            .range;
        return Err(vec![Diagnostic::new(
            range,
            "exceptional MIR requires panic=unwind",
        )]);
    }
    let _shapes = shapes::scope(&typed.sigs);
    let _queries = flux_solve::remember_solver_queries(&hir.items, &typed.sigs);
    let _fields = flux_solve::remember_field_types(&typed.sigs);
    let _headers = flux_solve::remember_impl_headers(&hir.items, &typed.sigs);
    let start = timings.is_some().then(std::time::Instant::now);
    let monomorphic = |id: FunctionId| {
        let function = &hir.items.functions[id];
        let instance = Instance {
            function: id,
            args: &[],
        };
        (!generic_instance(&hir.items, instance)
            && !function.is_extern
            && mir.bodies.get(id).is_some())
        .then_some(instance)
    };
    let mut retained_functions = FxHashSet::default();
    let mut retained_statics = FxHashSet::default();
    if let Some(c) = compilation {
        if matches!(c.kind, OutputKind::BuiltinSuite) {
            for (id, function) in hir
                .items
                .functions
                .iter()
                .take(hir.items.core.functions as usize)
            {
                if function.has_body && !function.is_extern && mir.bodies.get(id).is_none() {
                    let mut diagnostic =
                        Diagnostic::new(function.ret_range, "missing checked built-in MIR");
                    diagnostic.file = function.loc.file;
                    return Err(vec![diagnostic]);
                }
                if monomorphic(id).is_some() {
                    retained_functions.insert(id);
                }
            }
            for (id, konst) in hir.items.consts.iter().take(hir.items.core.consts as usize) {
                if konst.is_static.is_none() {
                    continue;
                }
                if konst.extern_symbol.is_none() && mir.const_values.items.get(id).is_none() {
                    let mut diagnostic =
                        Diagnostic::new(konst.def_range, "missing checked built-in static value");
                    diagnostic.file = konst.loc.file;
                    return Err(vec![diagnostic]);
                }
                retained_statics.insert(id);
            }
        } else {
            // The imported provider also sees dead metadata bodies. Their user
            // definitions enter through export/template reachability below, rather
            // than becoming unconditional roots of the executable's source crate.
            let source_required: Vec<_> = c
                .required_functions
                .iter()
                .copied()
                .filter(|&id| {
                    !c.imported
                        .contains(&hir.items.modules[hir.items.functions[id].loc.module].krate)
                        && monomorphic(id).is_some()
                })
                .collect();
            let own = match c.kind {
                OutputKind::Library(krate) => reachability::library(hir, &typed.sigs, mir, krate),
                OutputKind::Executable => {
                    reachability::executable(hir, &typed.sigs, mir, &source_required)
                }
                OutputKind::BuiltinSuite => unreachable!(),
            };
            retained_functions.extend(own.functions);
            retained_statics.extend(own.statics);
            if matches!(c.kind, OutputKind::Executable) {
                for &krate in c.imported {
                    let imported = reachability::library(hir, &typed.sigs, mir, krate);
                    retained_functions.extend(imported.functions);
                    retained_statics.extend(imported.statics);
                }
                // Embedded definitions still belong to the executable. Their
                // compiler-owned static roots retain the existing runtime policy.
                retained_statics.extend(hir.items.consts.iter().filter_map(|(id, c)| {
                    (id.index() < hir.items.core.consts as usize && c.is_static.is_some())
                        .then_some(id)
                }));
            }
            for &function in c.required_functions {
                let owner = hir.items.modules[hir.items.functions[function].loc.module].krate;
                if !c.imported.contains(&owner) || retained_functions.contains(&function) {
                    retained_functions.insert(function);
                }
            }
        }
    }
    let mut roots: Vec<_> = retained_functions
        .into_iter()
        .filter_map(monomorphic)
        .collect();
    if let Some(compilation) = compilation {
        roots.extend_from_slice(compilation.required_instances);
    }
    roots.sort_unstable_by_key(|i| i.function);
    let mut static_roots: Vec<_> = retained_statics.iter().copied().collect();
    static_roots.sort_unstable();
    let collected = match compilation.map(|c| c.kind) {
        Some(OutputKind::Library(_) | OutputKind::BuiltinSuite) => {
            flux_mir::mono::collect_from_roots_and_statics(
                hir,
                &typed.sigs,
                mir,
                &roots,
                &static_roots,
            )
        }
        Some(OutputKind::Executable) => flux_mir::mono::collect_all_with_roots_and_statics(
            hir,
            &typed.sigs,
            mir,
            &roots,
            &static_roots,
        ),
        None => flux_mir::mono::collect_all(hir, &typed.sigs, mir),
    }
    .map_err(|d| vec![d])?;
    if let (Some(timings), Some(start)) = (timings.as_deref_mut(), start) {
        timings.collect = start.elapsed();
    }
    let instances = collected.instances;
    let required_instances = compilation.map_or_else(Vec::new, |compilation| {
        instances
            .iter()
            .copied()
            .filter(|&instance| !owns_instance(&hir.items, instance, compilation))
            .collect()
    });
    let errors = check_c_abi(hir, typed, mir, &instances);
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut module = Module::default();
    let items = &hir.items;
    let identities = compilation.map_or(&[][..], |c| c.crate_hashes);
    let definitions = std::cell::RefCell::new(DefinitionKeys::new(items, identities));
    let queries = (identity == SymbolIdentity::Query).then(|| {
        QuerySymbols::new(
            items,
            compilation.expect("query symbols require object ownership"),
        )
    });
    let mut query_functions = FxHashSet::default();
    let names: FxHashMap<FunctionId, String> = items
        .functions
        .iter()
        .map(|(id, f)| {
            let name = if f.is_extern || f.link_name.is_some() {
                f.link_name.unwrap_or(f.name).to_string()
            } else {
                // The function's path: its crate, its modules, and its
                // type for an associated function, `krate::m::S::new`,
                // where `S` is in `m` wherever the `impl` block is.
                let target = items.impl_of(id).and_then(|i| items.impls[i].target);
                let module = match target {
                    Some(ImplTarget::Struct(s)) => items.structs[s].loc.module,
                    _ => f.loc.module,
                };
                let mut path = Vec::new();
                let mut m = Some(module);
                while let Some(id) = m {
                    path.push(items.modules[id].name.as_str());
                    m = items.modules[id].parent;
                }
                path.reverse();
                match (target, f.owner) {
                    (Some(ImplTarget::Struct(s)), _) => path.push(items.structs[s].name.as_str()),
                    (Some(ImplTarget::Primitive(name)), _) => path.push(name.as_str()),
                    (Some(ImplTarget::Object(t)), _) => path.push(items.traits[t].name.as_str()),
                    (None, Some(Owner::Trait(t))) => path.push(items.traits[t].name.as_str()),
                    (None, Some(Owner::Impl(imp))) => {
                        if let Some(t) = items.impls[imp].trait_ {
                            path.push(items.traits[t].name.as_str());
                        }
                    }
                    (None, None) => {}
                }
                if let Some((strukt, _)) = f.ctor
                    && items.structs[strukt].is_enum()
                {
                    path.push(items.structs[strukt].name.as_str());
                }
                path.push(f.name.as_str());
                // An impl may live in a different crate from its target.
                // Separate objects use the definition owner's crate identity;
                // the whole-program path retains its process-local suffix.
                let identity;
                if compilation.is_some() || id.index() >= items.core.functions as usize {
                    identity = if compilation.is_some() {
                        if let Some(key) = queries.as_ref().and_then(|queries| {
                            queries.definition(
                                items,
                                flux_hir::flib::IdKind::Function,
                                id.into_raw(),
                            )
                        }) {
                            query_functions.insert(id);
                            content_symbol("query_definition", key)
                        } else {
                            definition_symbol(
                                &mut definitions.borrow_mut(),
                                flux_hir::flib::IdKind::Function,
                                id.into_raw(),
                            )
                        }
                    } else {
                        format!("__flux_f{}", id.index())
                    };
                    path.push(&identity);
                }
                mangle(&path)
            };
            (id, name)
        })
        .collect();
    let symbols = Symbols {
        names,
        separate: compilation.is_some(),
        definitions,
        queries,
        query_functions,
        query_all_instances: identity == SymbolIdentity::Query
            && compilation.is_some_and(|c| {
                matches!(c.kind, OutputKind::Executable)
                    && c.imported.is_empty()
                    && !c.builtin_object
            }),
        query_type_keys: std::cell::RefCell::new(FxHashMap::default()),
        type_keys: std::cell::RefCell::new(FxHashMap::default()),
        instances: std::cell::RefCell::new(FxHashMap::default()),
    };
    let mut declared_abort = false;
    let mut declared: FxHashSet<_> = instances
        .iter()
        .filter(|&&instance| {
            !items.functions[instance.function].is_extern
                && items.functions[instance.function].link_name.is_some()
                && compilation.is_none_or(|c| owns_instance(items, instance, c))
        })
        .map(|instance| symbols[&instance.function].clone())
        .collect();
    let layouts = Layouts::new(items, &typed.sigs, &mir.discriminants);
    for (id, function) in hir.items.functions.iter() {
        // The same C function may be declared in several modules.
        if function.is_extern && declared.insert(symbols[&id].clone()) {
            declared_abort |= function.name.as_str() == "abort";
            module.decls.push(FuncDecl {
                attrs: attrs(&typed.sigs[id], mir.panic_strategy, function.abi),
                name: symbols[&id].clone(),
                sig: if function.abi.is_c() {
                    c_signature(&typed.sigs, &layouts, &typed.sigs[id], function.is_variadic)
                } else {
                    signature(&typed.sigs, &layouts, &typed.sigs[id], function.is_variadic)
                },
            });
        }
    }
    // Float remainder follows C fmod's truncation rule, including signed zero.
    for (name, scalar) in [("fmodf", Type::F32), ("fmod", Type::F64)] {
        if declared.insert(name.to_owned()) {
            module.decls.push(FuncDecl {
                name: name.into(),
                sig: Signature::new(&[scalar, scalar], &[scalar]),
                attrs: FunctionAttrs {
                    nounwind: true,
                    ..FunctionAttrs::default()
                },
            });
        }
    }
    // Each static's symbol, from its path as a function's is.
    let statics: FxHashMap<flux_hir::ConstId, String> = items
        .consts
        .iter()
        .filter(|(_, c)| c.is_static.is_some())
        .map(|(id, c)| {
            if let Some(symbol) = c.extern_symbol {
                return (id, symbol.to_string());
            }
            let mut path = Vec::new();
            let mut m = Some(c.loc.module);
            while let Some(id) = m {
                path.push(items.modules[id].name.as_str());
                m = items.modules[id].parent;
            }
            path.reverse();
            path.push(c.name.map_or("_", |n| n.as_str()));
            let identity;
            if compilation.is_some() || id.index() >= items.core.consts as usize {
                identity = if compilation.is_some() {
                    if let Some(key) = symbols.queries.as_ref().and_then(|queries| {
                        queries.definition(items, flux_hir::flib::IdKind::Const, id.into_raw())
                    }) {
                        content_symbol("query_definition", key)
                    } else {
                        definition_symbol(
                            &mut symbols.definitions.borrow_mut(),
                            flux_hir::flib::IdKind::Const,
                            id.into_raw(),
                        )
                    }
                } else {
                    format!("__flux_s{}", id.index())
                };
                path.push(&identity);
            }
            (id, mangle(&path))
        })
        .collect();
    let names = |id: flux_hir::ConstId| items.const_name(id);
    let mut ctx = Ctx {
        panic_strategy: mir.panic_strategy,
        indirect_thunks: FxHashMap::default(),
        generated_functions: Vec::new(),
        builder_context: FunctionBuilderContext::new(),
        hir,
        typed,
        compilation,
        retained_statics,
        pending_statics: if compilation.is_some() {
            static_roots
        } else {
            statics.keys().copied().collect()
        },
        const_values: &mir.const_values,
        discriminants: &mir.discriminants,
        evaluator: flux_mir::eval::Evaluator::new(
            items,
            &typed.sigs,
            &mir.bodies,
            &mir.consts,
            &names,
        )
        .with_imported(&mir.const_values, &mir.discriminants),
        instance_values: collected.const_values,
        resolved_instances: FxHashMap::default(),
        normalized_signatures: FxHashMap::default(),
        symbols: &symbols,
        statics: &statics,
        reified: collected.reified,
        uses_abort: false,
        locations: FxHashMap::default(),
        line_starts: FxHashMap::default(),
        strings: FxHashMap::default(),
        vtables: FxHashMap::default(),
        promoted: FxHashMap::default(),
        referenced_storage: FxHashMap::default(),
        named_constants: FxHashMap::default(),
        data: Vec::new(),
        data_errors: Vec::new(),
        layouts,
    };
    for instance in instances {
        if compilation.is_some_and(|c| !owns_instance(items, instance, c)) {
            let sig = flux_solve::traits::instance_sig(
                items,
                &typed.sigs,
                instance.function,
                instance.args,
            );
            let name = instance_symbol(hir, &symbols, instance);
            // A core extern bridge can name the same definition supplied by
            // the separately compiled builtin object. Keep one declaration.
            if declared.insert(name.clone()) {
                module.decls.push(FuncDecl {
                    name,
                    sig: ctx.instance_signature(instance, &sig, false),
                    attrs: attrs(
                        &sig,
                        mir.panic_strategy,
                        items.functions[instance.function].abi,
                    ),
                });
            }
            if items.functions[instance.function].track_caller {
                module.decls.push(FuncDecl {
                    name: ctx.pointer_symbol(instance),
                    sig: ctx.instance_signature(instance, &sig, true),
                    attrs: attrs(
                        &sig,
                        mir.panic_strategy,
                        items.functions[instance.function].abi,
                    ),
                });
            }
            continue;
        }
        let start = timings.is_some().then(std::time::Instant::now);
        let body = flux_mir::mono::body_of(hir, &typed.sigs, mir, instance);
        let substituted = start.map(|start| (start.elapsed(), std::time::Instant::now()));
        flux_mir::validate::assert_valid(
            &hir.items,
            &typed.sigs,
            &[],
            &body,
            &hir.items.functions[instance.function].name,
        );
        if let (Some(timings), Some((substitute, start))) = (timings.as_deref_mut(), substituted) {
            timings.substitute += substitute;
            timings.validate += start.elapsed();
        }
        let function = ctx.function(instance, &body, false);
        module.functions.push(function);
        if compilation.is_some() && items.functions[instance.function].track_caller {
            module.functions.push(ctx.function(instance, &body, true));
        }
    }
    // Constants whose values depend on generic arguments fail only
    // once code needs them, as in rustc.
    if !ctx.evaluator.failures.is_empty() {
        let mut errors: Vec<Diagnostic> = ctx
            .evaluator
            .failures
            .iter()
            .map(|(c, e)| {
                let mut d = Diagnostic::new(e.range, e.message.clone());
                d.file = items.consts[c.konst].loc.file;
                d
            })
            .collect();
        Diagnostic::sort(&mut errors);
        errors.dedup();
        return Err(errors);
    }
    // Every static, with its initial value, in memory a `static mut`
    // may write.
    let mut emitted_statics = FxHashSet::default();
    while let Some(id) = ctx.pending_statics.pop() {
        if !emitted_statics.insert(id)
            || items.consts[id].extern_symbol.is_some()
            || compilation.is_some_and(|c| !owns_static(items, id, c))
        {
            continue;
        }
        let symbol = &statics[&id];
        let Some(value) = mir.const_values.items.get(id) else {
            continue;
        };
        let ty = typed.sigs.consts[id];
        let (bytes, align) = ctx.value_bytes(value, ty);
        ctx.data.push(DataObject {
            name: symbol.clone(),
            thread_local: false,
            local: compilation.is_none(),
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: compilation.is_some(),
            init: Some(DataInit {
                constant: !items.consts[id].is_static_mut()
                    && !flux_solve::traits::has_interior_mutability(items, &typed.sigs, ty),
                align,
                items: bytes,
            }),
        });
    }
    if !ctx.data_errors.is_empty() {
        Diagnostic::sort(&mut ctx.data_errors);
        ctx.data_errors.dedup();
        return Err(ctx.data_errors);
    }
    ctx.data.sort_by(|a, b| a.name.cmp(&b.name));
    module.data = ctx.data;
    if ctx.uses_abort && !declared_abort {
        module.decls.push(FuncDecl {
            attrs: FunctionAttrs {
                noreturn: true,
                nounwind: true,
                ..FunctionAttrs::default()
            },
            name: "abort".to_owned(),
            sig: Signature::new(&[], &[]),
        });
    }
    let root = items.module_of(items.main_crate());
    let main = items.functions.iter().find(|(id, f)| {
        f.name.as_str() == items.entry_function_name(items.main_crate())
            && f.loc.module == root
            && !f.is_extern
            && f.owner.is_none()
            && mir.bodies.get(*id).is_some()
    });
    if let Some((id, _)) =
        main.filter(|_| compilation.is_none_or(|c| matches!(c.kind, OutputKind::Executable)))
    {
        let instance = Instance {
            function: id,
            args: &[],
        };
        let sig = &flux_solve::traits::instance_sig(items, &typed.sigs, id, &[]);
        let main_sig = ctx_signature(
            &typed.sigs,
            &ctx.layouts,
            sig,
            compilation.is_some() || ctx.reified.contains(&instance),
        );
        let flush = items
            .lang_fns
            .get(&flux_intern::Symbol::intern("flush_stdout"))
            .filter(|&&f| mir.bodies.get(f).is_some())
            .map(|&f| {
                let instance = Instance {
                    function: f,
                    args: &[],
                };
                let flush_sig = ctx_signature(
                    &typed.sigs,
                    &ctx.layouts,
                    &typed.sigs[f],
                    compilation.is_some(),
                );
                (instance_symbol(hir, &symbols, instance), flush_sig)
            });
        let init = items
            .lang_fns
            .get(&flux_intern::Symbol::intern("args_init"))
            .filter(|&&f| mir.bodies.get(f).is_some())
            .map(|&f| {
                let instance = Instance {
                    function: f,
                    args: &[],
                };
                (
                    instance_symbol(hir, &symbols, instance),
                    ctx_signature(
                        &typed.sigs,
                        &ctx.layouts,
                        &typed.sigs[f],
                        compilation.is_some(),
                    ),
                )
            });
        let runner = if sig.ret != Ty::UNIT && sig.ret != Ty::NEVER {
            flux_ty::entry::main_runner(items, &typed.sigs).map(|function| {
                let instance = Instance {
                    function,
                    args: Ty::list(&[flux_solve::traits::reveal(&typed.sigs, sig.ret)]),
                };
                let signature =
                    flux_solve::traits::instance_sig(items, &typed.sigs, function, instance.args);
                (
                    instance_symbol(hir, &symbols, instance),
                    ctx_signature(&typed.sigs, &ctx.layouts, &signature, compilation.is_some()),
                )
            })
        } else {
            None
        };
        let entry = entry_point(
            &symbols[&id],
            &main_sig,
            sig.ret == Ty::NEVER,
            flush,
            runner,
            init,
            (mir.panic_strategy == flux_mir::PanicStrategy::Unwind)
                .then_some("__flux_panic_finish"),
        );
        module.functions.push(entry);
    }
    module.functions.append(&mut ctx.generated_functions);
    unwind::declare_personality(&mut module);
    module.sync_function_attrs();
    Ok(LoweredCrate {
        module,
        required_instances,
    })
}

fn content_symbol(domain: &str, bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut name = format!("__flux_{domain}_");
    for byte in flux_hir::flib::digest(bytes) {
        write!(&mut name, "{byte:02x}").expect("writing to a string succeeds");
    }
    name
}

fn data_key(items: &[DataItem], align: u32) -> Vec<u8> {
    let mut key = align.to_le_bytes().to_vec();
    for item in items {
        match item {
            DataItem::Bytes(bytes) => {
                key.push(0);
                key.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                key.extend_from_slice(bytes);
            }
            DataItem::Int { ty, value } => {
                key.push(1);
                let width: u32 = match ty {
                    Type::I8 => 1,
                    Type::I16 => 2,
                    Type::I32 => 4,
                    Type::I64 => 8,
                    _ => unreachable!("integer data has an integer type"),
                };
                key.extend_from_slice(&width.to_le_bytes());
                key.extend_from_slice(&value.to_le_bytes());
            }
            DataItem::Zero(len) => {
                key.push(2);
                key.extend_from_slice(&len.to_le_bytes());
            }
            DataItem::Ptr { symbol, addend } => {
                key.push(3);
                key.extend_from_slice(&(symbol.len() as u64).to_le_bytes());
                key.extend_from_slice(symbol.as_bytes());
                key.extend_from_slice(&addend.to_le_bytes());
            }
            DataItem::RttiName { symbol } => {
                key.push(4);
                key.extend_from_slice(&(symbol.len() as u64).to_le_bytes());
                key.extend_from_slice(symbol.as_bytes());
            }
        }
    }
    key
}

struct QuerySymbols {
    definitions: query_identity::DefinitionKeys,
    sources: FxHashSet<flux_hir::CrateId>,
}

impl QuerySymbols {
    fn new(items: &flux_hir::ItemTree, compilation: Compilation<'_>) -> Self {
        use flux_hir::flib::{Encode, Encoder};
        let mut sources = FxHashSet::default();
        let namespaces: Vec<_> = items
            .crates
            .iter()
            .enumerate()
            .filter_map(|(index, data)| {
                let krate = flux_hir::CrateId(index as u32);
                let source = data.root.index() >= items.core.modules as usize
                    && !compilation.imported.contains(&krate)
                    && match compilation.kind {
                        OutputKind::Library(owner) => krate == owner,
                        OutputKind::Executable => true,
                        OutputKind::BuiltinSuite => false,
                    };
                let mut namespace = Encoder::new();
                if source {
                    "flux-unit-source-v1".encode(&mut namespace).ok()?;
                    data.name.encode(&mut namespace).ok()?;
                    let root = &items.modules[data.root];
                    items
                        .files
                        .get(&root.file)?
                        .path
                        .encode(&mut namespace)
                        .ok()?;
                    format!("{:?}", data.edition).encode(&mut namespace).ok()?;
                    sources.insert(krate);
                } else {
                    // Imported type references retain exact artifact ownership even
                    // inside a source function's edit-stable instance identity.
                    "flux-unit-artifact-reference-v1"
                        .encode(&mut namespace)
                        .ok()?;
                    let (_, hash) = compilation
                        .crate_hashes
                        .iter()
                        .find(|(id, _)| *id == krate)?;
                    hash.as_slice().encode(&mut namespace).ok()?;
                }
                Some((krate, namespace.into_bytes()))
            })
            .collect();
        let namespaces: Vec<_> = namespaces
            .iter()
            .map(|(id, key)| (*id, key.as_slice()))
            .collect();
        Self {
            definitions: query_identity::DefinitionKeys::new(items, &namespaces),
            sources,
        }
    }

    fn definition(
        &self,
        items: &flux_hir::ItemTree,
        kind: flux_hir::flib::IdKind,
        raw: u32,
    ) -> Option<&[u8]> {
        let module = match kind {
            flux_hir::flib::IdKind::Function if raw >= items.core.functions => {
                items
                    .functions
                    .get(flux_arena::Idx::from_raw(raw))?
                    .loc
                    .module
            }
            flux_hir::flib::IdKind::Const if raw >= items.core.consts => {
                items.consts.get(flux_arena::Idx::from_raw(raw))?.loc.module
            }
            _ => return None,
        };
        if self.sources.contains(&items.modules[module].krate) {
            self.definitions.key(kind, raw)
        } else {
            None
        }
    }
}

struct Symbols<'a> {
    names: FxHashMap<FunctionId, String>,
    separate: bool,
    /// The keys of definitions, under the compilation's crate identities.
    definitions: std::cell::RefCell<DefinitionKeys<'a>>,
    queries: Option<QuerySymbols>,
    query_functions: FxHashSet<FunctionId>,
    query_all_instances: bool,
    query_type_keys: std::cell::RefCell<FxHashMap<Ty, Vec<u8>>>,
    type_keys: std::cell::RefCell<FxHashMap<Ty, Vec<u8>>>,
    /// The symbol of each instance named so far.
    instances: std::cell::RefCell<FxHashMap<Instance, String>>,
}

impl std::ops::Index<&FunctionId> for Symbols<'_> {
    type Output = String;
    fn index(&self, id: &FunctionId) -> &String {
        &self.names[id]
    }
}

fn definition_symbol(
    definitions: &mut DefinitionKeys<'_>,
    kind: flux_hir::flib::IdKind,
    raw: u32,
) -> String {
    let key = definitions
        .key(kind, raw)
        .expect("code generation uses valid definition identities");
    content_symbol("definition", key)
}

fn generic_instance(items: &flux_hir::ItemTree, instance: Instance) -> bool {
    let function = &items.functions[instance.function];
    !instance.args.is_empty()
        || !function.generics.is_empty()
        || matches!(function.owner, Some(Owner::Trait(_)))
        || items
            .owner_generics(function.owner)
            .is_some_and(|g| !g.is_empty())
}

fn owns_instance(
    items: &flux_hir::ItemTree,
    instance: Instance,
    compilation: Compilation<'_>,
) -> bool {
    let owner = items.modules[items.functions[instance.function].loc.module].krate;
    match compilation.kind {
        OutputKind::Library(krate) => {
            instance.function.index() >= items.core.functions as usize
                && (owner == krate || generic_instance(items, instance))
        }
        OutputKind::Executable => {
            generic_instance(items, instance)
                || (!compilation.imported.contains(&owner)
                    && (!compilation.builtin_object
                        || instance.function.index() >= items.core.functions as usize))
        }
        OutputKind::BuiltinSuite => instance.function.index() < items.core.functions as usize,
    }
}

fn owns_static(
    items: &flux_hir::ItemTree,
    id: flux_hir::ConstId,
    compilation: Compilation<'_>,
) -> bool {
    let owner = items.modules[items.consts[id].loc.module].krate;
    match compilation.kind {
        OutputKind::Library(krate) => owner == krate,
        OutputKind::Executable => {
            !compilation.imported.contains(&owner)
                && (!compilation.builtin_object || id.index() >= items.core.consts as usize)
        }
        OutputKind::BuiltinSuite => id.index() < items.core.consts as usize,
    }
}

/// The symbol of `instance`, disambiguating generic arguments and impls.
/// Separate objects hash stable identities. Private whole-program instances
/// use an ordinal from this module's instance table.
fn instance_symbol(hir: &Hir, symbols: &Symbols<'_>, instance: Instance) -> String {
    if let Some(symbol) = symbols.instances.borrow().get(&instance) {
        return symbol.clone();
    }
    let symbol = instance_symbol_uncached(hir, symbols, instance);
    (symbols.instances.borrow_mut()).insert(instance, symbol.clone());
    symbol
}

fn instance_symbol_uncached(hir: &Hir, symbols: &Symbols<'_>, instance: Instance) -> String {
    let base = &symbols[&instance.function];
    let items = &hir.items;
    // A trait's blocks for different types each have its methods.
    let block = items.impl_of(instance.function).filter(|&owner| {
        let imp = &items.impls[owner];
        match imp.target {
            Some(ImplTarget::Struct(s)) => !items.structs[s].generics.is_empty(),
            Some(ImplTarget::Object(_)) => true,
            _ => imp.trait_ref.is_some(),
        }
    });
    if instance.args.is_empty() && block.is_none() {
        return base.clone();
    }
    if !symbols.separate {
        // Private whole-program instances only need distinct names within this
        // module. Avoid encoding and hashing their complete type identities.
        let suffix = format!("__flux_instance{}", symbols.instances.borrow().len());
        return format!("{}{}{suffix}E", &base[..base.len() - 1], suffix.len());
    }
    // The encoding uses stable item identities in separate objects.
    let mut key = Vec::new();
    let mut feed = |bytes: &[u8]| key.extend_from_slice(bytes);
    feed(base.as_bytes());
    for &arg in instance.args {
        if symbols.query_all_instances || symbols.query_functions.contains(&instance.function) {
            encode_query_type(arg, symbols, &mut feed);
        } else {
            // Imported and built-in instances must retain exactly the names
            // expected by their separately compiled objects.
            encode_type(arg, symbols, &mut feed);
        }
    }
    use std::fmt::Write;
    let mut hash = String::new();
    for byte in flux_hir::flib::digest(&key) {
        write!(&mut hash, "{byte:02x}").expect("writing to a string succeeds");
    }
    format!("{}65h{hash}E", &base[..base.len() - 1])
}

fn encode_type(ty: Ty, symbols: &Symbols<'_>, feed: &mut impl FnMut(&[u8])) {
    let mut keys = symbols.type_keys.borrow_mut();
    let key = keys.entry(ty).or_insert_with(|| {
        (symbols.definitions.borrow_mut().stable_key(ty))
            .expect("code generation uses fully resolved types")
    });
    feed(key);
}

fn encode_query_type(ty: Ty, symbols: &Symbols<'_>, feed: &mut impl FnMut(&[u8])) {
    let mut keys = symbols.query_type_keys.borrow_mut();
    let key = keys.entry(ty).or_insert_with(|| {
        symbols
            .queries
            .as_ref()
            .and_then(|queries| queries.definitions.type_key(ty))
            .unwrap_or_else(|| {
                let artifact = symbols
                    .definitions
                    .borrow_mut()
                    .stable_key(ty)
                    .expect("code generation uses fully resolved types");
                let mut key = b"flux-unit-artifact-type-fallback-v1".to_vec();
                key.extend_from_slice(&(artifact.len() as u64).to_le_bytes());
                key.extend_from_slice(&artifact);
                key
            })
    });
    feed(key);
}

/// A legacy-style mangled name, `_ZN<len>crate<len>nameE` (with the
/// type's name between for an associated function), which
/// demanglers show as `crate::name`.
fn mangle(path: &[&str]) -> String {
    let mut symbol = "_ZN".to_owned();
    for segment in path {
        symbol.push_str(&format!("{}{segment}", segment.len()));
    }
    symbol.push('E');
    symbol
}

/// The keel type of a scalar: an integer, `bool` or pointer.
fn keel_type(ty: Ty) -> Option<Type> {
    match ty.kind() {
        TyKind::Int(int) => Some(int_type(int)),
        TyKind::Float(flux_ty::FloatTy::F32) => Some(Type::F32),
        TyKind::Float(flux_ty::FloatTy::F64) => Some(Type::F64),
        TyKind::Bool => Some(Type::I8),
        TyKind::Char => Some(Type::I32),
        TyKind::RawPtr(..) | TyKind::Ref(..) | TyKind::FnPtr(_) => Some(Type::Ptr),
        // A function item has no value: it is its type.
        TyKind::FnDef(_) => None,
        TyKind::Tuple(_)
        | TyKind::Adt(..)
        | TyKind::Array(..)
        | TyKind::Slice(_)
        | TyKind::Str
        | TyKind::Dynamic(_)
        | TyKind::Closure(..)
        | TyKind::Param(_)
        | TyKind::Alias(_)
        | TyKind::Opaque(_)
        | TyKind::Never
        | TyKind::CStr
        | TyKind::Infer(_)
        | TyKind::Value(_)
        | TyKind::Error => None,
    }
}

/// The length of an array type, `len`: known once monomorphized.
fn array_len(len: Ty) -> u64 {
    len.as_usize()
        .expect("arrays' lengths are known once monomorphized")
}

/// The type of the discriminant of `ty`, an enum.
fn discriminant(sigs: &Signatures, ty: Ty) -> flux_ty::IntTy {
    sigs.discriminant_ty(ty).unwrap_or(flux_ty::IntTy::Isize)
}

/// The unsigned integer type of a tag `bytes` wide.
fn tag_int(bytes: u8) -> IntTy {
    match bytes {
        1 => IntTy::U8,
        2 => IntTy::U16,
        4 => IntTy::U32,
        8 => IntTy::U64,
        _ => IntTy::U128,
    }
}

/// Whether values of `ty` contain an array, so live in memory.
fn is_memory(sigs: &Signatures, ty: Ty) -> bool {
    shapes::is_memory(sigs, ty, is_memory_uncached)
}

fn is_memory_uncached(sigs: &Signatures, ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Array(..) => true,
        TyKind::Adt(adt, _) if sigs.unions.contains(&adt.strukt) => true,
        TyKind::Tuple(_) | TyKind::Adt(..) | TyKind::Closure(..) => {
            flux_solve::contents(sigs, ty)
                .is_some_and(|fields| fields.iter().any(|&f| is_memory(sigs, f)))
                || is_large_enum(sigs, ty)
        }
        _ => false,
    }
}

/// An enum with more leaves than this lives in memory. Held in variables,
/// an enum carries every variant's fields, and each copy, argument and
/// return moves all of them, though only one variant's mean anything.
/// In memory, a copy moves its bytes instead, and a variable's value is
/// a stack slot's. Measured on the self-built compiler, 40 to 64 put its
/// largest enums (`TyKind`, MIR's `Rvalue`, `Result`s of whole crates) in
/// memory and keep keel's small instruction enums in variables; 8 and 16
/// made it slower.
const ENUM_MEMORY_LEAVES: usize = 48;

/// keel expands a `memmove` of up to this many bytes into loads and
/// stores itself, after inlining, and calls the C library's for more.
const KEEL_COPY_EXPAND_BYTES: u64 = 32;

/// Larger copies of values of memory types, up to this many bytes, are
/// loads and stores here rather than calls of `memmove`.
const COPY_EXPAND_BYTES: u64 = 128;

/// Whether `ty` is an enum of several variants with more than
/// [`ENUM_MEMORY_LEAVES`] leaves, its discriminant included.
fn is_large_enum(sigs: &Signatures, ty: Ty) -> bool {
    match flux_solve::variants(sigs, ty) {
        Some(variants) if variants.len() > 1 => {
            let fields: usize = variants
                .iter()
                .flat_map(|fields| fields.iter())
                .map(|&f| leaf_count(sigs, f))
                .sum();
            1 + fields > ENUM_MEMORY_LEAVES
        }
        _ => false,
    }
}

/// A pointer to a slice, `str` or trait object: its address, then its
/// length or vtable.
fn is_fat(sigs: &Signatures, ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Ref(_, p) | TyKind::RawPtr(_, p) if sigs.is_unsized(p))
}

/// A pointer to a trait object: its address, then its vtable's.
fn is_dyn_ptr(sigs: &Signatures, ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Ref(_, p) | TyKind::RawPtr(_, p) if sigs.metadata(p) == flux_ty::Metadata::VTable)
}

/// What a pointer or `Box` of type `ty` points to.
fn pointee_of(ty: Ty) -> Option<Ty> {
    match ty.kind() {
        TyKind::Ref(_, p) | TyKind::RawPtr(_, p) => Some(p),
        _ => flux_solve::box_pointee(ty),
    }
}

/// A `Box` of a trait object.
fn is_dyn_box(sigs: &Signatures, ty: Ty) -> bool {
    flux_solve::box_pointee(ty).is_some_and(|p| sigs.metadata(p) == flux_ty::Metadata::VTable)
}

/// The second leaf of a fat pointer: a length, or a vtable's address.
fn metadata_leaf(sigs: &Signatures, ty: Ty) -> Ty {
    if is_dyn_ptr(sigs, ty) {
        address_leaf()
    } else {
        Ty::int(IntTy::Usize)
    }
}

/// The leaf of a value of a memory type: the address of its bytes.
fn address_leaf() -> Ty {
    Ty::raw_ptr(Mutability::Not, Ty::UNIT)
}

/// The scalar leaves of a value of `ty`, in field order: none for `()`
/// and `!`, one for a scalar; an enum's discriminant, then every
/// variant's fields; the address of a value of a memory type; the
/// address and length of a pointer to a slice.
fn leaves(sigs: &Signatures, ty: Ty, out: &mut TypeLeaves) {
    shapes::leaves(sigs, ty, out, leaves_uncached);
}

fn leaves_uncached(sigs: &Signatures, ty: Ty, out: &mut TypeLeaves) {
    if is_memory(sigs, ty) {
        out.push(address_leaf());
        return;
    }
    if is_fat(sigs, ty) {
        out.push(address_leaf());
        out.push(metadata_leaf(sigs, ty));
        return;
    }
    if let Some(variants) = flux_solve::variants(sigs, ty) {
        if !variants.is_empty() {
            out.push(Ty::int(discriminant(sigs, ty)));
        }
        for fields in variants {
            for &field in fields {
                leaves(sigs, field, out);
            }
        }
        return;
    }
    match flux_solve::fields(sigs, ty) {
        Some(fields) => {
            for &field in fields {
                leaves(sigs, field, out);
            }
        }
        None if keel_type(ty).is_some() => out.push(ty),
        None => {}
    }
}

fn leaves_of(sigs: &Signatures, ty: Ty) -> TypeLeaves {
    let mut out = TypeLeaves::new();
    leaves(sigs, ty, &mut out);
    out
}

fn leaf_count(sigs: &Signatures, ty: Ty) -> usize {
    shapes::leaf_count(sigs, ty, leaf_count_uncached)
}

fn leaf_count_uncached(sigs: &Signatures, ty: Ty) -> usize {
    if is_memory(sigs, ty) {
        return 1;
    }
    if is_fat(sigs, ty) {
        return 2;
    }
    if let Some(variants) = flux_solve::variants(sigs, ty) {
        return usize::from(!variants.is_empty())
            + variants
                .iter()
                .flat_map(|fields| fields.iter())
                .map(|&f| leaf_count(sigs, f))
                .sum::<usize>();
    }
    match flux_solve::fields(sigs, ty) {
        Some(fields) => fields.iter().map(|&f| leaf_count(sigs, f)).sum(),
        None => usize::from(keel_type(ty).is_some()),
    }
}

/// A keel layout of `size` bytes aligned to `align`, for an `sret`
/// pointer to a value of a memory type: keel needs only its bytes.
fn bytes_layout(size: u64, align: u64) -> Layout {
    for scalar in [Type::I64, Type::I32, Type::I16] {
        let bytes = Layout::Scalar(scalar).size();
        if align >= bytes && size.is_multiple_of(bytes) {
            return Layout::Array(Box::new(Layout::Scalar(scalar)), (size / bytes) as u32);
        }
    }
    Layout::Array(Box::new(Layout::Scalar(Type::I8)), size as u32)
}

/// The fields of a value of `place`'s type: of the variant it was
/// downcast to, or its own.
fn place_fields(sigs: &Signatures, place: PlaceTy) -> &'static [Ty] {
    flux_solve::variant_fields(sigs, place.ty, place.variant.unwrap_or(0))
        .expect("fields are of aggregates")
}

/// Where a place's value is.
#[derive(Clone, Copy)]
enum Location {
    /// Leaves `start..start + len` of a local held in variables.
    Vars {
        local: Local,
        start: usize,
        len: usize,
    },
    /// In memory, at an address.
    Memory(Value),
}

fn int_type(int: IntTy) -> Type {
    match int.bits() {
        8 => Type::I8,
        16 => Type::I16,
        32 => Type::I32,
        64 => Type::I64,
        _ => Type::I128,
    }
}

/// The width of an integer, `char` or `bool` value.
fn bits(ty: Ty) -> u32 {
    match ty.kind() {
        TyKind::Char => 32,
        _ => ty.as_int().map_or(8, IntTy::bits),
    }
}

/// `int main(int argc, char **argv)`: calls the Rust `main`, then
/// `flush`, which writes out what stdout holds, as std's runtime does,
/// and returns 0.
fn entry_point(
    rust_main: &str,
    main_sig: &Signature,
    never: bool,
    flush: Option<(String, Signature)>,
    runner: Option<(String, Signature)>,
    init: Option<(String, Signature)>,
    unwind_finish: Option<&str>,
) -> Function {
    let mut function = Function::new(
        "main",
        Signature::new(&[Type::I32, Type::Ptr], &[Type::I32]),
    );
    if unwind_finish.is_some() {
        function.personality = Some(unwind::PERSONALITY.into());
        function.attrs.nounwind = true;
    }
    let mut b = FunctionBuilder::new(&mut function);
    let entry = b.create_block();
    let landing = unwind_finish.map(|_| b.create_block());
    b.switch_to_block(entry);
    let argc = b.append_block_param(entry, Type::I32);
    let argv = b.append_block_param(entry, Type::Ptr);
    if let Some((name, sig)) = init {
        let init = b.import_function(&name, sig);
        unwind::entry_call(&mut b, init, &[argc, argv], landing);
    }
    let callee = b.import_function(rust_main, main_sig.clone());
    let status = if let Some((name, sig)) = runner {
        let address = b.ins().symbol_addr(keel_ir::Symbol::Func(callee));
        let runner = b.import_function(&name, sig);
        let call = unwind::entry_call(&mut b, runner, &[address], landing);
        Some(b.function().inst_results(call)[0])
    } else {
        unwind::entry_call(&mut b, callee, &[], landing);
        None
    };
    if let Some((flush, flush_sig)) = flush.as_ref().filter(|_| !never) {
        let flush = b.import_function(flush, flush_sig.clone());
        unwind::entry_call(&mut b, flush, &[], landing);
    }
    if never {
        b.ins().trap();
    } else {
        let code = status.unwrap_or_else(|| b.ins().iconst(Type::I32, 0));
        b.ins().return_(&[code]);
    }
    if let Some((landing, finish)) = landing.zip(unwind_finish) {
        b.switch_to_block(landing);
        let pad = b.ins().build(keel_ir::InstData::LandingPad {
            clauses: vec![keel_ir::Clause::Catch(None)],
        });
        let exception = b.function().inst_results(pad)[0];
        let finish = b.import_function(finish, Signature::new(&[Type::Ptr], &[Type::I32]));
        let status = b.ins().call(finish, &[exception]);
        let status = b.function().inst_results(status)[0];
        if let Some((name, sig)) = flush {
            let flush = b.import_function(&name, sig);
            b.ins().call(flush, &[]);
        }
        b.ins().return_(&[status]);
    }
    b.seal_all_blocks();
    b.finalize();
    function
}

struct ReferencedStorage {
    // Keep the allocation alive while its address is a cache key.
    _owner: Arc<flux_mir::eval::Value>,
    symbol: String,
    length: Option<u64>,
}

struct Ctx<'a> {
    panic_strategy: flux_mir::PanicStrategy,
    indirect_thunks: FxHashMap<Signature, String>,
    generated_functions: Vec<Function>,
    /// Reuse SSA construction tables across functions without retaining their state.
    builder_context: FunctionBuilderContext,
    hir: &'a Hir,
    typed: &'a TypedFile,
    compilation: Option<Compilation<'a>>,
    retained_statics: FxHashSet<flux_hir::ConstId>,
    pending_statics: Vec<flux_hir::ConstId>,
    /// The values of constants that code without generic parameters
    /// names.
    const_values: &'a flux_mir::eval::ConstValues,
    /// The discriminants of enums that are not literals'.
    discriminants: &'a flux_mir::Discriminants,
    /// Evaluates the others, for the instances that use them.
    evaluator: flux_mir::eval::Evaluator<'a>,
    instance_values: FxHashMap<flux_ty::ConstInstance, flux_mir::eval::Value>,
    /// Trait selection and argument normalization are fixed during lowering.
    resolved_instances: FxHashMap<Instance, Instance>,
    /// Normalization reads the fixed items and signatures of this lowering.
    normalized_signatures: FxHashMap<Instance, FnSig>,
    symbols: &'a Symbols<'a>,
    /// The data object of each static.
    statics: &'a FxHashMap<flux_hir::ConstId, String>,
    /// The instances whose address the program takes.
    reified: FxHashSet<Instance>,
    uses_abort: bool,
    /// The data object of each `Location` a `#[track_caller]` function
    /// is passed, by file and offset.
    locations: FxHashMap<(flux_hir::FileId, u32), String>,
    /// Where each line of a file of a `Location` starts.
    line_starts: FxHashMap<flux_hir::FileId, Box<[u32]>>,
    /// The data object holding each distinct C string, by its bytes.
    strings: FxHashMap<Box<[u8]>, String>,
    /// The vtable of each type as each trait object: `(object, type)`.
    vtables: FxHashMap<(Ty, Ty), String>,
    /// The data objects of promoted constants, by their bytes and
    /// alignment.
    promoted: FxHashMap<(Vec<u8>, u32), String>,
    /// Allocation identity is only a process-local shortcut before content hashing.
    referenced_storage: FxHashMap<(usize, Ty), ReferencedStorage>,
    /// Monomorphic constant addresses also form DAG edges by item identity.
    named_constants: FxHashMap<flux_hir::ConstId, String>,
    data: Vec<DataObject>,
    data_errors: Vec<Diagnostic>,
    /// How values are laid out in memory, as rustc lays them out.
    layouts: Layouts<'a>,
}

impl Ctx<'_> {
    fn resolve_instance(&mut self, instance: Instance) -> Instance {
        if instance.args.is_empty()
            && !matches!(
                self.hir.items.functions[instance.function].owner,
                Some(Owner::Trait(_))
            )
        {
            return instance;
        }
        *self.resolved_instances.entry(instance).or_insert_with(|| {
            flux_solve::traits::resolve_instance(&self.hir.items, &self.typed.sigs, instance)
        })
    }

    fn normalized_signature(&mut self, instance: Instance) -> FnSig {
        self.normalized_signatures
            .entry(instance)
            .or_insert_with(|| {
                flux_solve::traits::instance_sig(
                    &self.hir.items,
                    &self.typed.sigs,
                    instance.function,
                    instance.args,
                )
            })
            .clone()
    }

    /// The discriminant of variant `variant` of enum `strukt`, as bits.
    fn discriminant(&mut self, strukt: flux_hir::StructId, variant: u32) -> u128 {
        let items = &self.hir.items;
        flux_mir::discriminant_bits(items, self.discriminants, strukt, variant)
            .or_else(|| self.evaluator.discriminant(strukt, variant).ok())
            .expect("discriminants are evaluated before code generation")
    }

    /// `instance`, whose MIR is `body` with its generic arguments
    /// substituted.
    fn function(&mut self, instance: Instance, body: &Body, pointer_wrapper: bool) -> Function {
        let sig = &self.normalized_signature(instance);
        let takes_location = !pointer_wrapper && self.takes_location(instance);
        let c_abi = self.hir.items.functions[instance.function].abi.is_c();
        let signature = self.instance_signature(instance, sig, pointer_wrapper);
        let mut function = Function::new(
            if pointer_wrapper {
                self.pointer_symbol(instance)
            } else {
                instance_symbol(self.hir, self.symbols, instance)
            },
            signature,
        );
        function.local = self.compilation.is_none()
            && self.hir.items.functions[instance.function]
                .link_name
                .is_none();
        function.attrs = attrs(
            sig,
            self.panic_strategy,
            self.hir.items.functions[instance.function].abi,
        );
        if body.blocks.values().any(|data| {
            matches!(
                data.terminator.unwind,
                flux_mir::UnwindAction::Cleanup(_) | flux_mir::UnwindAction::Terminate
            )
        }) {
            function.personality = Some(unwind::PERSONALITY.into());
        }
        if !pointer_wrapper {
            let data = &self.hir.items.functions[instance.function];
            match data.inline {
                Some(flux_hir::InlineHint::Hint) => function.attrs.inlinehint = true,
                Some(flux_hir::InlineHint::Always) => function.attrs.alwaysinline = true,
                Some(flux_hir::InlineHint::Never) => function.attrs.noinline = true,
                None => {}
            }
            function.attrs.cold = data.cold;
        }
        if cfg!(target_arch = "x86_64") {
            let owner = body.owner.unwrap_or(instance.function);
            let features = self.hir.items.functions[owner]
                .target_features
                .as_deref()
                .unwrap_or(&[]);
            function.attrs.target_features =
                keel_ir::X86TargetFeatures::new(features.iter().filter_map(|feature| {
                    match feature.as_str() {
                        "sse" | "sse2" => None, // Part of Keel's x86-64 baseline.
                        "sse4.1" => Some("sse41".to_owned()),
                        "sse4.2" => Some("sse42".to_owned()),
                        feature => Some(feature.to_owned()),
                    }
                }))
                .expect("validated x86 target features");
        }
        function.weak_odr =
            self.compilation.is_some() && generic_instance(&self.hir.items, instance);
        // Separate instances coalesce inside the executable and cannot be
        // interposed by another image. Hidden binding permits direct addresses.
        function.hidden = self.compilation.is_some();
        // Hidden strong definitions bind within this executable, so the
        // optimizer may substitute their bodies at direct calls. The linker may
        // keep another object's copy of a weak ODR instance instead, so those
        // promise only that every copy has an equivalent body.
        function.attrs.noninterposable = function.hidden && !function.weak_odr;
        function.attrs.no_semantic_interposition = function.hidden && function.weak_odr;
        let typed = self.typed;
        let owner = body.owner.unwrap_or(instance.function);
        let file = self.hir.items.functions[owner].loc.file;
        let convention = if c_abi {
            Convention::C
        } else if self.compilation.is_some() || self.reified.contains(&instance) {
            Convention::RustReified
        } else {
            Convention::RustFast
        };
        let builder_context = std::mem::take(&mut self.builder_context);
        let lower = Lower {
            ctx: self,
            sigs: &typed.sigs,
            body,
            file,
            convention,
            takes_location,
            caller_location: None,
            b: FunctionBuilder::with_context(&mut function, builder_context),
            blocks: Vec::new(),
            vars: Vec::new(),
            variable_lists: SlicePool::default(),
            slots: Vec::new(),
            sret: None,
            direct_sret: Vec::new(),
            placed: Vec::new(),
            callees: FxHashMap::default(),
            address_align: FxHashMap::default(),
            panic_block: None,
            unwind_action: flux_mir::UnwindAction::Unreachable,
            exception_slot: None,
            cleanup_propagates: None,
        };
        self.builder_context = lower.body();
        function
    }

    fn instance_signature(
        &self,
        instance: Instance,
        sig: &FnSig,
        pointer_wrapper: bool,
    ) -> Signature {
        let mut signature = if self.hir.items.functions[instance.function].abi.is_c() {
            c_signature(&self.typed.sigs, &self.layouts, sig, false)
        } else {
            ctx_signature(
                &self.typed.sigs,
                &self.layouts,
                sig,
                self.compilation.is_some() || self.reified.contains(&instance),
            )
        };
        if !pointer_wrapper && self.takes_location(instance) {
            signature.params.push(AbiParam::new(Type::Ptr));
        }
        signature
    }

    /// Whether `instance` takes where it is called from, as a pointer
    /// to a `Location` after its parameters: a `#[track_caller]`
    /// function's, unless its address is taken, as a pointer's
    /// signature has no room for it.
    fn takes_location(&self, instance: Instance) -> bool {
        self.hir.items.functions[instance.function].track_caller
            && (self.compilation.is_some() || !self.reified.contains(&instance))
    }

    fn pointer_symbol(&self, instance: Instance) -> String {
        let mut symbol = instance_symbol(self.hir, self.symbols, instance);
        if self.compilation.is_some() && self.hir.items.functions[instance.function].track_caller {
            symbol.push_str(".__flux_reify");
        }
        symbol
    }

    /// The data object of the `Location` of `offset` in `file`: its
    /// path as the compiler was given it, and its one-based line and
    /// column, in characters, as rustc counts them.
    fn location(&mut self, file: flux_hir::FileId, offset: u32) -> String {
        let items = &self.hir.items;
        let (file, offset) = items.source_offset(file, offset);
        if let Some(name) = self.locations.get(&(file, offset)) {
            return name.clone();
        }
        let (path, line, column) = match items.files.get(&file) {
            Some(source) => {
                let text = source.text();
                let mut at = (offset as usize).min(text.len());
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                let starts = self.line_starts.entry(file).or_insert_with(|| {
                    let ends = text.match_indices('\n').map(|(i, _)| i as u32 + 1);
                    std::iter::once(0).chain(ends).collect()
                });
                // The lines that start at or before `at`; the first starts at 0.
                let line = starts.partition_point(|&start| start as usize <= at);
                let line_start = starts[line - 1] as usize;
                let column = text[line_start..at].chars().count() + 1;
                (source.path.display().to_string(), line, column)
            }
            None => (String::new(), 1, 1),
        };
        use flux_mir::eval::Value as V;
        let bytes = path
            .bytes()
            .map(|b| V::Int {
                bits: u128::from(b),
                ty: IntTy::U8,
            })
            .collect();
        let value = V::Aggregate(
            vec![
                V::Ref(Arc::new(V::Aggregate(bytes))),
                V::Int {
                    bits: line as u128,
                    ty: IntTy::U32,
                },
                V::Int {
                    bits: column as u128,
                    ty: IntTy::U32,
                },
            ]
            .into(),
        );
        let strukt = items.lang_enums[&flux_intern::Symbol::intern("Location")];
        let ty = Ty::adt(strukt, items.structs[strukt].name, &[]);
        let name = self.promoted(&value, ty);
        self.locations.insert((file, offset), name.clone());
        name
    }

    /// `value`, of type `ty`, laid out in memory: its bytes and
    /// alignment.
    fn value_bytes(&mut self, value: &flux_mir::eval::Value, ty: Ty) -> (Vec<DataItem>, u32) {
        let ty = flux_solve::traits::normalize(&self.hir.items, &self.typed.sigs, &[], ty);
        let layout = self.layouts.of(ty);
        let mut bytes = vec![0u8; layout.size.max(1) as usize];
        // The addresses, which the linker fills in, by offset.
        let mut addresses: Vec<(usize, String, u32)> = Vec::new();
        self.write_bytes(value, ty, 0, &mut bytes, &mut addresses);
        let mut items = Vec::new();
        let mut at = 0;
        addresses.sort();
        for (offset, symbol, addend) in addresses {
            if offset > at {
                items.push(DataItem::Bytes(bytes[at..offset].to_vec()));
            }
            items.push(DataItem::Ptr { symbol, addend });
            at = offset + 8;
        }
        if at < bytes.len() {
            items.push(DataItem::Bytes(bytes[at..].to_vec()));
        }
        (items, layout.align as u32)
    }

    /// Named constants borrowed by promotion share the static projection
    /// representation, but their data is emitted only when referenced.
    fn constant_storage(&mut self, id: flux_hir::ConstId) -> String {
        if let Some(name) = self.statics.get(&id) {
            let imported = self.hir.items.consts[id].extern_symbol.is_some()
                || self
                    .compilation
                    .is_some_and(|c| !owns_static(&self.hir.items, id, c));
            if !imported && self.retained_statics.insert(id) {
                self.pending_statics.push(id);
            }
            if imported && !self.data.iter().any(|data| data.name == *name) {
                self.data.push(DataObject {
                    name: name.clone(),
                    thread_local: false,
                    local: false,
                    weak_odr: false,
                    weak: false,
                    section: None,
                    retain: false,
                    hidden: false,
                    init: None,
                });
            }
            return name.clone();
        }
        if let Some(name) = self.named_constants.get(&id) {
            return name.clone();
        }
        let value = self
            .evaluator
            .eval_const(id)
            .expect("promoted constant was evaluated");
        let name = self.promoted(&value, self.typed.sigs.consts[id]);
        self.named_constants.insert(id, name.clone());
        name
    }

    fn static_part(
        &self,
        id: flux_hir::ConstId,
        path: &[flux_mir::eval::StaticProjection],
    ) -> (u64, Ty) {
        use flux_mir::eval::StaticProjection as P;
        let mut ty = self.typed.sigs.consts[id];
        let mut variant = 0;
        let mut offset = 0;
        for p in path {
            ty = flux_solve::traits::normalize(&self.hir.items, &self.typed.sigs, &[], ty);
            match *p {
                P::Downcast(v) => variant = v,
                P::Field(i) => {
                    offset += self.layouts.of(ty).field_offset(variant, i);
                    ty = flux_solve::variant_fields(&self.typed.sigs, ty, variant).unwrap()
                        [i as usize];
                    variant = 0;
                }
                P::Index(i) => {
                    ty = ty.sequence_elem().unwrap();
                    offset += self.layouts.of(ty).size * i;
                }
                P::Subslice { from, len } => {
                    let elem = ty.sequence_elem().unwrap();
                    offset += self.layouts.of(elem).size * from;
                    ty = Ty::array(elem, len);
                }
            }
        }
        (offset, ty)
    }

    /// Writes `value`, of type `ty`, into `bytes` at `at`, laid out as
    /// in memory; the addresses it holds go in `addresses`.
    fn write_bytes(
        &mut self,
        value: &flux_mir::eval::Value,
        ty: Ty,
        at: usize,
        bytes: &mut [u8],
        addresses: &mut Vec<(usize, String, u32)>,
    ) {
        use flux_mir::eval::Value as V;
        let ty = flux_solve::traits::normalize(&self.hir.items, &self.typed.sigs, &[], ty);
        let put = |bytes: &mut [u8], at: usize, bits: u128, width: usize| {
            bytes[at..at + width].copy_from_slice(&bits.to_le_bytes()[..width]);
        };
        let layout = self.layouts.of(ty);
        match value {
            V::Uninit(_) => {}
            V::Float { bits, .. } => put(bytes, at, u128::from(*bits), layout.size as usize),
            V::Int { bits, .. } => put(bytes, at, *bits, layout.size as usize),
            V::RawAddr(address) => put(bytes, at, u128::from(*address), 8),
            V::Bool(b) => bytes[at] = u8::from(*b),
            V::Function(instance) => {
                addresses.push((at, self.pointer_symbol(*instance), 0));
            }
            V::DynPointer {
                pointer,
                concrete,
                object,
                mutable,
            } => {
                self.write_bytes(
                    pointer,
                    Ty::new(TyKind::Ref(*mutable, *concrete)),
                    at,
                    bytes,
                    addresses,
                );
                let vtable = self.vtable(*object, *concrete);
                addresses.push((at + 8, vtable, 0));
            }
            V::Static(id) => {
                addresses.push((at, self.constant_storage(*id), 0));
                if let Some(len) =
                    static_slice_metadata(&self.typed.sigs, ty, self.typed.sigs.consts[*id])
                {
                    put(bytes, at + 8, u128::from(len), 8);
                }
            }
            V::StaticPart { id, path } => {
                let (offset, stored) = self.static_part(*id, path);
                let addend = u32::try_from(offset).unwrap_or_else(|_| {
                    let loc = &self.hir.items.consts[*id];
                    let mut d = Diagnostic::new(
                        loc.def_range,
                        "static pointer addends larger than u32 are not supported yet",
                    );
                    d.file = loc.loc.file;
                    self.data_errors.push(d);
                    0
                });
                addresses.push((at, self.constant_storage(*id), addend));
                if let Some(len) = static_slice_metadata(&self.typed.sigs, ty, stored) {
                    put(bytes, at + 8, u128::from(len), 8);
                }
            }
            V::FramePointer { .. } | V::InteriorRef(_) | V::LocalRef { .. } => {
                unreachable!("escaping transient borrows are rejected")
            }
            V::PointerOffset {
                pointer,
                source,
                offset,
                len,
                ..
            } => {
                let mut original = vec![0; self.layouts.of(*source).size as usize];
                let mut pointers = Vec::new();
                self.write_bytes(pointer, *source, 0, &mut original, &mut pointers);
                bytes[at..at + 8].copy_from_slice(&original[..8]);
                for (_, symbol, addend) in pointers.into_iter().filter(|(at, _, _)| *at == 0) {
                    let addend = u64::from(addend)
                        .checked_add(*offset)
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or_else(|| {
                            let mut diagnostic = Diagnostic::new(
                                Default::default(),
                                "constant pointer addends larger than u32 are not supported yet",
                            );
                            if let V::Static(id) = pointer.as_ref() {
                                let declaration = &self.hir.items.consts[*id];
                                diagnostic.file = declaration.loc.file;
                                diagnostic.range = declaration.def_range;
                            }
                            self.data_errors.push(diagnostic);
                            0
                        });
                    addresses.push((at, symbol, addend));
                }
                if layout.size > 8 {
                    put(
                        bytes,
                        at + 8,
                        u128::from(len.expect("wide offset pointer has metadata")),
                        8,
                    );
                }
            }
            V::PointerView {
                pointer, source, ..
            } => {
                let stored_size = self.layouts.of(*source).size as usize;
                let size = layout.size as usize;
                let mut storage = vec![0; stored_size];
                let mut pointers = Vec::new();
                self.write_bytes(pointer, *source, 0, &mut storage, &mut pointers);
                // Pointer casts preserve the original allocation and may drop
                // wide metadata, never grow a pointer without new metadata.
                assert!(size <= stored_size);
                bytes[at..at + size].copy_from_slice(&storage[..size]);
                addresses.extend(
                    pointers
                        .into_iter()
                        .filter(|(offset, _, _)| *offset < size)
                        .map(|(offset, symbol, addend)| (at + offset, symbol, addend)),
                );
            }
            V::Ref(pointee) => {
                let (symbol, len) = self.referenced(pointee, ty);
                addresses.push((at, symbol, 0));
                if let Some(len) = len {
                    put(bytes, at + 8, u128::from(len), 8);
                }
            }
            V::Union { field, value } => {
                let ty = flux_solve::fields(&self.typed.sigs, ty).unwrap()[*field as usize];
                self.write_bytes(value, ty, at, bytes, addresses);
            }
            V::Aggregate(elements) if matches!(ty.kind(), TyKind::Array(..)) => {
                let elem = ty.sequence_elem().unwrap_or(Ty::ERROR);
                let stride = self.layouts.of(elem).size as usize;
                for (i, element) in elements.iter().enumerate() {
                    self.write_bytes(element, elem, at + i * stride, bytes, addresses);
                }
            }
            V::Aggregate(fields) => {
                let types = flux_solve::fields(&self.typed.sigs, ty).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let offset = layout.field_offset(0, i as u32) as usize;
                    self.write_bytes(field, field_ty, at + offset, bytes, addresses);
                }
            }
            V::Variant {
                strukt,
                index,
                fields,
            } => {
                let types =
                    flux_solve::variant_fields(&self.typed.sigs, ty, *index).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let offset = layout.field_offset(*index, i as u32) as usize;
                    self.write_bytes(field, field_ty, at + offset, bytes, addresses);
                }
                if let Some((offset, width, bits)) = self.tag_store(ty, *strukt, *index) {
                    put(bytes, at + offset as usize, bits, usize::from(width));
                }
            }
        }
    }

    /// What a value of variant `variant` of `ty`, an enum of `strukt`,
    /// has in its tag: its offset, width and bits; none where the tag
    /// is the variant's own fields (a niche-filling enum's untagged
    /// variant) or there is none.
    fn tag_store(
        &mut self,
        ty: Ty,
        strukt: flux_hir::StructId,
        variant: u32,
    ) -> Option<(u64, u8, u128)> {
        let layout = self.layouts.of(ty);
        let Shape::Enum { tag: Some(tag), .. } = &layout.shape else {
            return None;
        };
        let mask = if tag.bytes >= 16 {
            u128::MAX
        } else {
            (1u128 << (8 * u32::from(tag.bytes))) - 1
        };
        let bits = match tag.encoding {
            TagEncoding::Direct => self.discriminant(strukt, variant),
            TagEncoding::Niche { untagged, .. } if untagged == variant => return None,
            TagEncoding::Niche { first, start, .. } => {
                u128::from(variant.wrapping_sub(first)).wrapping_add(start)
            }
        };
        Some((tag.offset, tag.bytes, bits & mask))
    }

    /// The name of the read-only data object holding `value`, of type
    /// `ty`: a promoted constant, shared by identical ones.
    fn promoted(&mut self, value: &flux_mir::eval::Value, ty: Ty) -> String {
        let (items, align) = self.value_bytes(value, ty);
        let key = (data_key(&items, align), align);
        if let Some(name) = self.promoted.get(&key) {
            return name.clone();
        }
        let name = content_symbol("promoted", &key.0);
        self.promoted.insert(key, name.clone());
        self.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
            init: Some(DataInit {
                constant: true,
                align,
                items,
            }),
        });
        name
    }

    /// The data object a reference of type `ty` to `pointee`, in a
    /// constant, points to, and for a slice its length.
    fn referenced(
        &mut self,
        pointee: &Arc<flux_mir::eval::Value>,
        ty: Ty,
    ) -> (String, Option<u64>) {
        let key = (Arc::as_ptr(pointee) as usize, ty);
        if let Some(stored) = self.referenced_storage.get(&key) {
            return (stored.symbol.clone(), stored.length);
        }
        let target = match ty.kind() {
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t,
            _ => Ty::ERROR,
        };
        let (symbol, length) = match (target.kind(), pointee.as_ref()) {
            (TyKind::Slice(elem), flux_mir::eval::Value::Aggregate(elements)) => {
                let len = elements.len() as u64;
                let array = Ty::array(elem, len);
                (self.promoted(pointee, array), Some(len))
            }
            // A `str` is its UTF-8 bytes.
            (TyKind::Str, flux_mir::eval::Value::Aggregate(bytes)) => {
                let len = bytes.len() as u64;
                let array = Ty::array(Ty::int(IntTy::U8), len);
                (self.promoted(pointee, array), Some(len))
            }
            _ => (self.promoted(pointee, target), None),
        };
        self.referenced_storage.insert(
            key,
            ReferencedStorage {
                _owner: Arc::clone(pointee),
                symbol: symbol.clone(),
                length,
            },
        );
        (symbol, length)
    }

    /// The name of the read-only data object of `concrete`'s vtable as
    /// trait object `object` ([`flux_mir::vtable`]): the function that
    /// drops a value of the type in place (if it needs one and `core`
    /// has `ptr::drop_in_place`), its size and alignment, then its
    /// slots.
    fn vtable(&mut self, object: Ty, concrete: Ty) -> String {
        if let Some(name) = self.vtables.get(&(object, concrete)) {
            return name.clone();
        }
        let mut key = Vec::new();
        encode_type(object, self.symbols, &mut |bytes| {
            key.extend_from_slice(bytes)
        });
        encode_type(concrete, self.symbols, &mut |bytes| {
            key.extend_from_slice(bytes)
        });
        let name = content_symbol("vtable", &key);
        self.vtables.insert((object, concrete), name.clone());
        let sigs = &self.typed.sigs;
        let layout = self.layouts.of(concrete);
        let word = |value: u64| DataItem::Int {
            ty: Type::I64,
            value: value as i64,
        };
        let drop = match flux_mir::vtable::drop_entry(&self.hir.items, sigs, concrete) {
            Some(instance) => DataItem::Ptr {
                symbol: self.pointer_symbol(instance),
                addend: 0,
            },
            None => DataItem::Zero(8),
        };
        let mut items = vec![drop, word(layout.size), word(layout.align)];
        let entries = flux_mir::vtable::entries(&self.hir.items, sigs, object, concrete);
        for entry in entries {
            items.push(match entry {
                flux_mir::vtable::Entry::Null => DataItem::Zero(8),
                flux_mir::vtable::Entry::Method(instance) => DataItem::Ptr {
                    symbol: self.pointer_symbol(instance),
                    addend: 0,
                },
                flux_mir::vtable::Entry::Super(sup) => DataItem::Ptr {
                    symbol: self.vtable(sup, concrete),
                    addend: 0,
                },
            });
        }
        self.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
            init: Some(DataInit {
                constant: true,
                align: 8,
                items,
            }),
        });
        name
    }

    /// The name of the read-only data object holding `bytes` and a
    /// terminating NUL, shared by identical strings.
    fn c_string(&mut self, bytes: &[u8]) -> String {
        if let Some(name) = self.strings.get(bytes) {
            return name.clone();
        }
        let name = content_symbol("str", bytes);
        let mut contents = bytes.to_vec();
        contents.push(0);
        self.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
            init: Some(DataInit {
                constant: true,
                align: 1,
                items: vec![DataItem::Bytes(contents)],
            }),
        });
        self.strings.insert(bytes.into(), name.clone());
        name
    }
}

/// Runtime storage and convention for one call's result.
struct CallResult {
    ty: Ty,
    sret: Option<(Value, Layout)>,
    convention: Convention,
}

struct Lower<'a, 'c, 'f> {
    ctx: &'c mut Ctx<'a>,
    sigs: &'a Signatures,
    body: &'c Body,
    /// The file the body's ranges are in.
    file: flux_hir::FileId,
    convention: Convention,
    /// Whether the function takes where it was called from.
    takes_location: bool,
    /// That parameter, a pointer to a `Location`.
    caller_location: Option<Value>,
    b: FunctionBuilder<'f>,
    /// The keel block for each MIR block.
    blocks: Vec<Block>,
    /// The keel variables of each MIR local held in variables, one per
    /// leaf.
    vars: Vec<SliceId<Variable>>,
    variable_lists: SlicePool<Variable>,
    /// The storage address of each local that is borrowed or lives in memory.
    slots: Vec<Option<Value>>,
    /// Where to store the return value, and its layout, when it is
    /// returned through memory.
    sret: Option<(Value, Layout)>,
    /// Fresh, unborrowed locals whose storage can receive a call result.
    direct_sret: Vec<bool>,
    /// Locals stored in the field of the aggregate they are moved into.
    placed: Vec<bool>,
    callees: FxHashMap<Instance, FuncRef>,
    /// Reduced alignment of addresses derived from packed storage.
    address_align: FxHashMap<Value, u64>,
    /// Calls `abort`; shared by every failed assertion.
    panic_block: Option<Block>,
    unwind_action: flux_mir::UnwindAction,
    exception_slot: Option<Value>,
    cleanup_propagates: Option<Vec<bool>>,
}

/// Where locals of memory types are stored, from [`memory_storage`].
struct MemoryStorage {
    borrowed: Vec<bool>,
    definitions: Vec<usize>,
    /// The local whose storage each local shares.
    roots: Vec<usize>,
    /// Locals stored inside the aggregate they are an operand of: the
    /// local, the aggregate's local and the operand's index in it.
    fields: Vec<(usize, usize, FieldOf)>,
}

/// Which field of an aggregate an operand becomes.
#[derive(Clone, Copy)]
enum FieldOf {
    /// Field `field` of variant `variant` (0 for a struct, tuple or closure).
    Field { variant: u32, field: u32 },
    /// Element `index` of an array of `elem`s.
    Element { elem: Ty, index: u64 },
}

/// Coalesce an aggregate temporary with its sole consumer only when neither
/// storage address escapes and both locals have one definition. Cyclic bodies
/// keep separate slots because a later iteration can overlap their lifetimes.
/// The same holds for a temporary moved whole into an aggregate built in
/// memory: it is stored where that field is, so it is built there.
fn memory_storage(body: &Body, sigs: &Signatures) -> MemoryStorage {
    let count = body.locals.len();
    let mut borrowed = vec![false; count];
    let mut definitions = vec![0; count];
    let mut uses = vec![0; count];
    let mut copies = Vec::new();
    let mut operands = Vec::new();
    for param in body.params() {
        definitions[param.index()] += 1;
    }
    let write = |place: &Place, definitions: &mut [usize], uses: &mut [usize]| {
        if place.projection.is_empty() {
            definitions[place.local.index()] += 1;
        } else {
            uses[place.local.index()] += 1;
        }
    };
    let operand = |operand: &Operand, uses: &mut [usize]| {
        if let Operand::Copy(place) | Operand::Move(place) = operand {
            uses[place.local.index()] += 1;
        }
    };
    let mut incoming = vec![0; body.blocks.len()];
    for block in body.blocks.values() {
        for next in block.terminator.successors() {
            incoming[next.index()] += 1;
        }
        for statement in &block.statements {
            if let StatementKind::Assign(dest, value) = &statement.kind {
                write(dest, &mut definitions, &mut uses);
                for op in value.operands() {
                    operand(op, &mut uses);
                }
                match value {
                    Rvalue::Ref { place, .. } => {
                        uses[place.local.index()] += 1;
                        if !place.has_deref() {
                            borrowed[place.local.index()] = true;
                        }
                    }
                    Rvalue::Discriminant(place)
                    | Rvalue::DiscriminantForDrop(place)
                    | Rvalue::Len { place, .. } => uses[place.local.index()] += 1,
                    Rvalue::Use(Operand::Copy(src) | Operand::Move(src))
                        if dest.projection.is_empty()
                            && src.projection.is_empty()
                            && body.locals[dest.local].ty == body.locals[src.local].ty
                            && is_memory(sigs, body.locals[src.local].ty) =>
                    {
                        copies.push((src.local.index(), dest.local.index()))
                    }
                    Rvalue::Aggregate(kind, ops)
                        if dest.projection.is_empty()
                            && is_memory(sigs, body.locals[dest.local].ty) =>
                    {
                        for (i, op) in ops.iter().enumerate() {
                            let (Operand::Copy(src) | Operand::Move(src)) = op else {
                                continue;
                            };
                            if !src.projection.is_empty()
                                || src.local == dest.local
                                || !is_memory(sigs, body.locals[src.local].ty)
                            {
                                continue;
                            }
                            let field = match *kind {
                                AggregateKind::Union(..) => continue,
                                AggregateKind::Array(elem) => FieldOf::Element {
                                    elem,
                                    index: i as u64,
                                },
                                AggregateKind::Variant(_, variant) => FieldOf::Field {
                                    variant,
                                    field: i as u32,
                                },
                                AggregateKind::Tuple
                                | AggregateKind::Struct(_)
                                | AggregateKind::Closure => FieldOf::Field {
                                    variant: 0,
                                    field: i as u32,
                                },
                            };
                            operands.push((src.local.index(), dest.local.index(), field));
                        }
                    }
                    _ => {}
                }
            }
        }
        for op in block.terminator.kind.operands() {
            operand(op, &mut uses);
        }
        match &block.terminator.kind {
            TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } => {
                write(dest, &mut definitions, &mut uses)
            }
            TerminatorKind::Drop { place, .. } => uses[place.local.index()] += 1,
            TerminatorKind::Return => uses[Body::RETURN.index()] += 1,
            _ => {}
        }
    }
    // Kahn's algorithm also detects loops outside the entry component.
    let mut queue: Vec<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    let mut next = 0;
    while next < queue.len() {
        let block = flux_mir::BasicBlock::from_raw(queue[next] as u32);
        next += 1;
        for successor in body.blocks[block].terminator.successors() {
            incoming[successor.index()] -= 1;
            if incoming[successor.index()] == 0 {
                queue.push(successor.index());
            }
        }
    }
    let mut roots: Vec<_> = (0..count).collect();
    fn root(mut local: usize, roots: &mut [usize]) -> usize {
        let mut representative = local;
        while roots[representative] != representative {
            representative = roots[representative];
        }
        while roots[local] != local {
            let next = roots[local];
            roots[local] = representative;
            local = next;
        }
        representative
    }
    let acyclic = queue.len() == body.blocks.len();
    if acyclic {
        for (src, dest) in copies {
            if uses[src] == 1
                && definitions[src] == 1
                && definitions[dest] == 1
                && !borrowed[src]
                && !borrowed[dest]
            {
                let source = root(src, &mut roots);
                let destination = root(dest, &mut roots);
                if source != destination {
                    roots[source] = destination;
                }
            }
        }
    }
    for local in 0..count {
        roots[local] = root(local, &mut roots);
    }
    // A temporary's only use builds the aggregate, after it is defined,
    // and the aggregate's local is defined there alone, so their bytes
    // are never live at once. Each is the last of its copy chain.
    let mut fields = Vec::new();
    if acyclic {
        for (src, dest, field) in operands {
            if uses[src] == 1
                && definitions[src] == 1
                && definitions[dest] == 1
                && !borrowed[src]
                && !borrowed[dest]
                && src != Body::RETURN.index()
                && roots[src] == src
                && roots[dest] != src
            {
                fields.push((src, dest, field));
            }
        }
    }
    MemoryStorage {
        borrowed,
        definitions,
        roots,
        fields,
    }
}

/// The storage of a body's locals, while [`Lower::body`] assigns it.
struct Storage {
    types: Vec<Ty>,
    roots: Vec<usize>,
    placed: Vec<Option<(usize, FieldOf)>>,
    addresses: Vec<Option<Value>>,
}

impl Lower<'_, '_, '_> {
    /// The address of the storage that local `root` and the locals
    /// coalesced with it share: the caller's return memory, a field of
    /// an aggregate's or a new stack slot.
    fn storage(&mut self, root: usize, storage: &mut Storage) -> Value {
        if let Some(address) = storage.addresses[root] {
            return address;
        }
        let ty = storage.types[root];
        let address = if let Some((dest, field)) = storage.placed[root] {
            let base = self.storage(storage.roots[dest], storage);
            let offset = match field {
                FieldOf::Field { variant, field } => self
                    .ctx
                    .layouts
                    .of(storage.types[dest])
                    .field_offset(variant, field),
                FieldOf::Element { elem, index } => self.ctx.layouts.of(elem).size * index,
            };
            self.offset(base, offset)
        } else if root == Body::RETURN.index() && is_memory(self.sigs, ty) {
            match &self.sret {
                Some((pointer, _)) => *pointer,
                None => self.new_slot(ty),
            }
        } else {
            self.new_slot(ty)
        };
        storage.addresses[root] = Some(address);
        address
    }

    fn body(mut self) -> FunctionBuilderContext {
        let body = self.body;
        let sigs = self.sigs;
        // The parameters are stored where they live first, which may
        // take blocks of its own (an enum's variants), then `bb0` runs.
        let entry = self.b.create_block();
        self.blocks = body.blocks.iter().map(|_| self.b.create_block()).collect();
        let MemoryStorage {
            borrowed,
            definitions,
            roots,
            fields,
        } = memory_storage(body, sigs);
        let mut placed = vec![None; body.locals.len()];
        for &(src, dest, field) in &fields {
            placed[src] = Some((dest, field));
        }
        self.placed = placed.iter().map(Option::is_some).collect();
        self.direct_sret = definitions
            .iter()
            .zip(&borrowed)
            .map(|(&n, &borrowed)| n == 1 && !borrowed)
            .collect();
        self.b.switch_to_block(entry);
        if body.has_unwind() {
            self.exception_slot = Some(self.aligned_slot(8, 8));
        }
        // A memory return is built directly in the caller's return slot.
        // Its input operands still have their own by-value storage.
        if let Return::Sret(layout) | Return::Memory(layout) =
            self.convention
                .result(sigs, &self.ctx.layouts, body.locals[Body::RETURN].ty)
        {
            let pointer = self.b.append_block_param(entry, Type::Ptr);
            self.sret = Some((pointer, layout));
        }
        let mut storage = Storage {
            types: body.locals.iter().map(|(_, decl)| decl.ty).collect(),
            roots,
            placed,
            addresses: vec![None; body.locals.len()],
        };
        for (local, decl) in body.locals.iter() {
            if borrowed[local.index()] || is_memory(sigs, decl.ty) {
                let root = storage.roots[local.index()];
                let address = self.storage(root, &mut storage);
                self.slots.push(Some(address));
                self.vars.push(SliceId::default());
            } else {
                let vars = self
                    .variable_lists
                    .alloc(leaves_of(sigs, decl.ty).into_iter().map(|leaf| {
                        self.b
                            .declare_var(keel_type(leaf).expect("leaves are scalars"))
                    }));
                self.slots.push(None);
                self.vars.push(vars);
            }
        }
        for param in body.params() {
            let ty = body.locals[param].ty;
            match self.convention.argument(sigs, &self.ctx.layouts, ty) {
                Argument::Ignore => continue,
                Argument::Indirect | Argument::ByVal(_) => {
                    let pointer = self.b.append_block_param(entry, Type::Ptr);
                    let values = self.load(Location::Memory(pointer), ty);
                    self.write(&param.into(), &values);
                    continue;
                }
                Argument::Scalar(scalar) => {
                    let value = self.b.append_block_param(entry, keel_type(scalar).unwrap());
                    let values = self.decode_c_scalar(value, scalar, ty);
                    self.write(&param.into(), &values);
                    continue;
                }
                Argument::Direct => {}
            }
            let leaves = leaves_of(sigs, body.locals[param].ty);
            let values: Values = leaves
                .into_iter()
                .map(|leaf| {
                    let ty = keel_type(leaf).expect("leaves are scalars");
                    self.b.append_block_param(entry, ty)
                })
                .collect();
            self.write(&param.into(), &values);
        }
        if self.takes_location {
            self.caller_location = Some(self.b.append_block_param(entry, Type::Ptr));
        }
        self.b.ins().jump(self.blocks[0], &[]);
        for (block, data) in body.blocks.iter() {
            self.b.switch_to_block(self.blocks[block.index()]);
            for statement in &data.statements {
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    let ty = body.place_ty(dest, sigs);
                    if is_memory(sigs, ty)
                        && let Rvalue::Aggregate(..) | Rvalue::Repeat(..) = rvalue
                    {
                        let (location, _) = self.locate(dest);
                        let Location::Memory(address) = location else {
                            unreachable!("memory types are in memory")
                        };
                        self.build_in_memory(address, rvalue, ty);
                        continue;
                    }
                    let values = self.rvalue(rvalue, ty);
                    self.write(dest, &values);
                }
            }
            self.terminator(&data.terminator.kind, data.terminator.range.start(), block);
        }
        if let Some(panic) = self.panic_block {
            self.b.switch_to_block(panic);
            let abort = self.b.import_function("abort", Signature::new(&[], &[]));
            self.b.ins().call(abort, &[]);
            self.b.ins().trap();
        }
        self.b.seal_all_blocks();
        self.b.finalize_with_context()
    }

    /// Where `place`'s value is, and its type. Going through a pointer
    /// reads the pointer.
    fn locate(&mut self, place: &Place) -> (Location, Ty) {
        let sigs = self.sigs;
        let mut ty = PlaceTy::from(self.body.locals[place.local].ty);
        let mut location = match self.slots[place.local.index()] {
            Some(address) => Location::Memory(address),
            None => Location::Vars {
                local: place.local,
                start: 0,
                len: self.variable_lists[self.vars[place.local.index()]].len(),
            },
        };
        for (i, &elem) in place.projection.iter().enumerate() {
            let elem_ty = ty.project(elem, sigs);
            location = match (elem, location) {
                (ProjectionElem::Field(field), Location::Vars { local, start, .. }) => {
                    let fields = place_fields(sigs, ty);
                    let before: usize = fields[..field as usize]
                        .iter()
                        .map(|&f| leaf_count(sigs, f))
                        .sum();
                    Location::Vars {
                        local,
                        start: start + before,
                        len: leaf_count(sigs, elem_ty.ty),
                    }
                }
                (ProjectionElem::Field(field), Location::Memory(address)) => {
                    let layout = self.ctx.layouts.of(ty.ty);
                    let offset = layout.field_offset(ty.variant.unwrap_or(0), field);
                    if matches!(
                        ty.ty.kind(),
                        TyKind::Adt(..) | TyKind::Tuple(_) | TyKind::Closure(..)
                    ) {
                        self.address_align
                            .entry(address)
                            .and_modify(|a| *a = (*a).min(layout.align))
                            .or_insert(layout.align);
                    }
                    Location::Memory(self.offset(address, offset))
                }
                (ProjectionElem::Downcast(variant), Location::Vars { local, start, .. }) => {
                    let variants =
                        flux_solve::variants(sigs, ty.ty).expect("downcasts are of enums");
                    let count = |fields: &[Ty]| -> usize {
                        fields.iter().map(|&f| leaf_count(sigs, f)).sum()
                    };
                    let before: usize = variants[..variant as usize].iter().map(|f| count(f)).sum();
                    Location::Vars {
                        local,
                        start: start + 1 + before,
                        len: count(variants[variant as usize]),
                    }
                }
                // A variant's fields' offsets are from the enum's start.
                (ProjectionElem::Downcast(_), Location::Memory(address)) => {
                    Location::Memory(address)
                }
                (ProjectionElem::Deref | ProjectionElem::BoxDeref, location) => {
                    let pointer = self.load(location, ty.ty)[0];
                    Location::Memory(pointer)
                }
                (ProjectionElem::Index(index), Location::Memory(address)) => {
                    let size = self.ctx.layouts.of(elem_ty.ty).size;
                    let index = self.read(&index.into())[0];
                    Location::Memory(self.element(address, index, size))
                }
                (
                    ProjectionElem::ConstantIndex {
                        offset, from_end, ..
                    },
                    Location::Memory(address),
                ) => {
                    let size = self.ctx.layouts.of(elem_ty.ty).size;
                    let index = if from_end {
                        // `len - offset`, the length an array's or the
                        // slice's pointer's.
                        let len = match ty.ty.kind() {
                            TyKind::Array(_, n) => {
                                self.b.ins().iconst(Type::I64, array_len(n) as i64)
                            }
                            _ => {
                                let base = Place {
                                    local: place.local,
                                    projection: place.projection.prefix(i),
                                };
                                self.slice_parts(&base)[1]
                            }
                        };
                        let offset = self.b.ins().iconst(Type::I64, i64::from(offset));
                        self.b.ins().isub(len, offset)
                    } else {
                        self.b.ins().iconst(Type::I64, i64::from(offset))
                    };
                    Location::Memory(self.element(address, index, size))
                }
                (ProjectionElem::Subslice { from, .. }, Location::Memory(address)) => {
                    let elem = match ty.ty.kind() {
                        TyKind::Array(elem, _) | TyKind::Slice(elem) => elem,
                        _ => unreachable!("subslices are of arrays and slices"),
                    };
                    let size = self.ctx.layouts.of(elem).size;
                    Location::Memory(self.offset(address, u64::from(from) * size))
                }
                (
                    ProjectionElem::Index(_)
                    | ProjectionElem::ConstantIndex { .. }
                    | ProjectionElem::Subslice { .. },
                    Location::Vars { .. },
                ) => {
                    unreachable!("arrays are in memory")
                }
            };
            ty = elem_ty;
        }
        (location, ty.ty)
    }

    /// The pointer and length of the unsized place `place` (or the
    /// pointer and vtable of a trait object): those of the pointer it is
    /// behind, or for a slice's part (`rest @ ..`), moved past its first
    /// elements and shortened by those at both ends.
    fn slice_parts(&mut self, place: &Place) -> Values {
        match &*place.projection {
            [base @ .., ProjectionElem::Subslice { from, to, .. }] => {
                let base = Place {
                    local: place.local,
                    projection: place.projection.prefix(base.len()),
                };
                let parts = self.slice_parts(&base);
                let elem = match self.body.place_ty(&base, self.sigs).kind() {
                    TyKind::Slice(elem) => elem,
                    _ => unreachable!("a slice's part is of a slice"),
                };
                let size = self.ctx.layouts.of(elem).size;
                let pointer = self.offset(parts[0], u64::from(*from) * size);
                let cut = self.b.ins().iconst(Type::I64, i64::from(from + to));
                let len = self.b.ins().isub(parts[1], cut);
                smallvec![pointer, len]
            }
            _ => {
                let deref = place
                    .projection
                    .iter()
                    .rposition(|p| matches!(p, ProjectionElem::Deref | ProjectionElem::BoxDeref))
                    .expect("DSTs are behind pointers");
                let pointer = Place {
                    local: place.local,
                    projection: place.projection.prefix(deref),
                };
                let parts = self.read(&pointer);
                let (Location::Memory(address), _) = self.locate(place) else {
                    unreachable!("DST place is in memory");
                };
                smallvec![address, parts[1]]
            }
        }
    }

    /// The address of element `index` of the array or slice at
    /// `address`, whose elements are `size` bytes apart.
    fn element(&mut self, address: Value, index: Value, size: u64) -> Value {
        if size == 0 {
            return address;
        }
        let stride = size;
        let size = self.b.ins().iconst(Type::I64, size as i64);
        let offset = self.b.ins().imul(index, size);
        let result = self.b.ins().ptradd(address, offset);
        if let Some(&align) = self.address_align.get(&address) {
            self.address_align
                .insert(result, align.min(1u64 << stride.trailing_zeros()));
        }
        result
    }

    /// Copies a value of `ty`, a memory type, from `from` to `to`.
    fn copy(&mut self, to: Value, from: Value, ty: Ty) {
        let layout = self.ctx.layouts.of(ty);
        let (size, align) = (layout.size, layout.align);
        if size == 0 || to == from {
            return;
        }
        if (KEEL_COPY_EXPAND_BYTES + 1..=COPY_EXPAND_BYTES).contains(&size) {
            // Every chunk is loaded before any is stored, as `memmove`.
            let mut chunks = Vec::new();
            let mut offset = 0;
            for (bytes, ty) in [
                (16, Type::Vector(keel_ir::VectorType::I8x16)),
                (8, Type::I64),
                (4, Type::I32),
                (2, Type::I16),
                (1, Type::I8),
            ] {
                while offset + bytes <= size {
                    let at = self.offset(from, offset);
                    let flags = self.chunk_flags(at, align, offset, bytes);
                    chunks.push((offset, bytes, self.b.ins().load_with_flags(ty, at, flags)));
                    offset += bytes;
                }
            }
            for (offset, bytes, value) in chunks {
                let at = self.offset(to, offset);
                let flags = self.chunk_flags(at, align, offset, bytes);
                self.b.ins().store_with_flags(value, at, flags);
            }
            return;
        }
        let len = self.b.ins().iconst(Type::I64, size as i64);
        self.b.ins().intrinsic(Intrinsic::Memmove, &[to, from, len]);
    }

    /// Flags for `bytes` at `at`, `offset` bytes into a value aligned
    /// to `align`.
    fn chunk_flags(&mut self, at: Value, align: u64, offset: u64, bytes: u64) -> MemoryFlags {
        let mut align = align.min(bytes);
        if offset != 0 {
            align = align.min(1 << offset.trailing_zeros());
        }
        if let Some(&known) = self.address_align.get(&at) {
            align = align.min(known);
        }
        MemoryFlags {
            align: align as u32,
            ..MemoryFlags::default()
        }
    }

    /// The leaves of the `ty` at `location`: for a memory type, its
    /// address.
    fn load(&mut self, location: Location, ty: Ty) -> Values {
        if is_memory(self.sigs, ty) {
            let Location::Memory(address) = location else {
                unreachable!("memory types are in memory")
            };
            return smallvec![address];
        }
        match location {
            Location::Vars { local, start, len } => (start..start + len)
                .map(|i| {
                    let var = self.variable_lists[self.vars[local.index()]][i];
                    self.b.use_var(var)
                })
                .collect(),
            Location::Memory(address) => self.load_value(address, ty),
        }
    }

    fn read(&mut self, place: &Place) -> Values {
        let (location, ty) = self.locate(place);
        self.load(location, ty)
    }

    fn write(&mut self, place: &Place, values: &[Value]) {
        let (location, ty) = self.locate(place);
        match location {
            Location::Vars { local, start, len } => {
                for (i, &value) in (start..start + len).zip(values) {
                    let var = self.variable_lists[self.vars[local.index()]][i];
                    self.b.def_var(var, value);
                }
            }
            Location::Memory(address) if is_memory(self.sigs, ty) => {
                let size = self.ctx.layouts.of(ty).size;
                // A value with no size (`[T; 0]`) may come as no value.
                if let Some(&value) = values.first()
                    && size > 0
                {
                    self.copy(address, value, ty);
                }
            }
            Location::Memory(address) => self.store(address, ty, values),
        }
    }

    /// A new stack slot for a value of `ty`, and its address.
    fn new_slot(&mut self, ty: Ty) -> Value {
        let layout = self.ctx.layouts.of(ty);
        self.aligned_slot(layout.size, layout.align)
    }

    /// Keel's stack frame is aligned to 16 bytes. Reserve padding and
    /// round an interior pointer for types with a larger alignment.
    fn aligned_slot(&mut self, size: u64, align: u64) -> Value {
        let padding = if align > 16 { align - 1 } else { 0 };
        let reserve = u32::try_from(
            size.max(1)
                .checked_add(padding)
                .expect("stack size overflow"),
        )
        .expect("stack object exceeds the IR size limit");
        let slot = self.b.create_stack_slot(reserve, align.min(16) as u32);
        let base = self.b.ins().stack_addr(slot);
        if align <= 16 {
            return base;
        }
        let bits = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, base);
        let padding = self.b.ins().iconst(Type::I64, padding as i64);
        let upper = self.b.ins().iadd(bits, padding);
        let mask = self.b.ins().iconst(Type::I64, -(align as i64));
        let rounded = self.b.ins().and(upper, mask);
        self.b
            .ins()
            .convert(ConvertOp::IntToPtr, Type::Ptr, rounded)
    }

    /// Stores at `address` the value of `rvalue`, an aggregate or repeat
    /// expression of `ty`, a memory type: each part where it goes.
    fn build_in_memory(&mut self, address: Value, rvalue: &Rvalue, ty: Ty) {
        if matches!(
            ty.kind(),
            TyKind::Adt(..) | TyKind::Tuple(_) | TyKind::Closure(..)
        ) {
            let align = self.ctx.layouts.of(ty).align;
            self.address_align
                .entry(address)
                .and_modify(|a| *a = (*a).min(align))
                .or_insert(align);
        }
        let sigs = self.sigs;
        match rvalue {
            Rvalue::Aggregate(AggregateKind::Union(_, field), operands) => {
                let field_ty = flux_solve::fields(sigs, ty).unwrap()[*field as usize];
                let values = self.operand(&operands[0]);
                self.store(address, field_ty, &values);
            }
            Rvalue::Aggregate(AggregateKind::Array(elem), operands) => {
                let size = self.ctx.layouts.of(*elem).size;
                for (i, operand) in operands.iter().enumerate() {
                    if self.is_placed(operand) {
                        continue;
                    }
                    let values = self.operand(operand);
                    let at = self.offset(address, size * i as u64);
                    self.store(at, *elem, &values);
                }
            }
            Rvalue::Aggregate(AggregateKind::Variant(strukt, index), operands) => {
                let layout = self.ctx.layouts.of(ty);
                let fields = flux_solve::variant_fields(sigs, ty, *index).unwrap_or_default();
                for (i, (operand, &field)) in operands.iter().zip(fields).enumerate() {
                    if self.is_placed(operand) {
                        continue;
                    }
                    let values = self.operand(operand);
                    let at = self.offset(address, layout.field_offset(*index, i as u32));
                    self.store(at, field, &values);
                }
                self.store_tag(address, ty, *strukt, *index);
            }
            Rvalue::Aggregate(_, operands) => {
                let layout = self.ctx.layouts.of(ty);
                let fields = flux_solve::fields(sigs, ty).unwrap_or_default();
                for (i, (operand, &field)) in operands.iter().zip(fields).enumerate() {
                    if self.is_placed(operand) {
                        continue;
                    }
                    let values = self.operand(operand);
                    let at = self.offset(address, layout.field_offset(0, i as u32));
                    self.store(at, field, &values);
                }
            }
            Rvalue::Repeat(operand, count) => {
                let TyKind::Array(elem, _) = ty.kind() else {
                    unreachable!("repeats make arrays")
                };
                let values = self.operand(operand);
                self.repeat(address, elem, &values, array_len(*count));
            }
            _ => unreachable!("only aggregates are built in memory"),
        }
    }

    /// Whether `operand` is a local already stored in the field of the
    /// aggregate being built that it is moved into.
    fn is_placed(&self, operand: &Operand) -> bool {
        matches!(operand, Operand::Copy(p) | Operand::Move(p)
            if p.projection.is_empty() && self.placed[p.local.index()])
    }

    /// Stores `values`, the leaves of a `ty`, at `address`, laid out as
    /// rustc lays it out; an enum's variant is the one its first leaf,
    /// the discriminant, says.
    fn store(&mut self, address: Value, ty: Ty, values: &[Value]) {
        if matches!(
            ty.kind(),
            TyKind::Adt(..) | TyKind::Tuple(_) | TyKind::Closure(..)
        ) {
            let align = self.ctx.layouts.of(ty).align;
            self.address_align
                .entry(address)
                .and_modify(|a| *a = (*a).min(align))
                .or_insert(align);
        }
        let sigs = self.sigs;
        if is_memory(sigs, ty) {
            if let Some(&value) = values.first() {
                self.copy(address, value, ty);
            }
            return;
        }
        if is_fat(self.sigs, ty) {
            self.b.ins().store_with_flags(
                values[0],
                address,
                address_flags(&self.address_align, address, Type::Ptr),
            );
            let at = self.offset(address, 8);
            self.b.ins().store_with_flags(
                values[1],
                at,
                address_flags(&self.address_align, at, Type::I64),
            );
            return;
        }
        if let Some(variants) = flux_solve::variants(sigs, ty) {
            self.store_enum(address, ty, &variants, values);
            return;
        }
        if let Some(fields) = flux_solve::fields(sigs, ty) {
            let layout = self.ctx.layouts.of(ty);
            let mut at = 0;
            for (i, &field) in fields.iter().enumerate() {
                let count = leaf_count(sigs, field);
                let to = self.offset(address, layout.field_offset(0, i as u32));
                self.store(to, field, &values[at..at + count]);
                at += count;
            }
            return;
        }
        if let (Some(scalar), Some(&value)) = (keel_type(ty), values.first()) {
            self.b.ins().store_with_flags(
                value,
                address,
                address_flags(&self.address_align, address, scalar),
            );
        }
    }

    /// Stores an enum's leaves: the fields of the variant its
    /// discriminant says, and its tag. Variants share their bytes, so
    /// only that variant's are stored: where several have fields, the
    /// discriminant picks which code runs.
    fn store_enum(&mut self, address: Value, ty: Ty, variants: &[&'static [Ty]], values: &[Value]) {
        let sigs = self.sigs;
        let layout = self.ctx.layouts.of(ty);
        let TyKind::Adt(adt, _) = ty.kind() else {
            return;
        };
        let strukt = adt.strukt;
        // Each variant's leaves, after the discriminant.
        let mut ranges = Vec::new();
        let mut at = 1;
        for fields in variants {
            let count: usize = fields.iter().map(|&f| leaf_count(sigs, f)).sum();
            ranges.push(at..at + count);
            at += count;
        }
        let store_variant = |this: &mut Self, v: usize| {
            let mut leaf = ranges[v].start;
            for (i, &field) in variants[v].iter().enumerate() {
                let count = leaf_count(sigs, field);
                let to = this.offset(address, layout.field_offset(v as u32, i as u32));
                this.store(to, field, &values[leaf..leaf + count]);
                leaf += count;
            }
        };
        let with_data: Vec<usize> = (0..variants.len())
            .filter(|&v| !ranges[v].is_empty())
            .collect();
        let tag = match &layout.shape {
            Shape::Enum { tag, present, .. } => tag.map(|t| (t, *present)),
            _ => None,
        };
        let Some((tag, _)) = tag else {
            // One variant may hold a value: it is that one.
            if let Shape::Enum { present, .. } = layout.shape {
                store_variant(self, present as usize);
            }
            return;
        };
        // One variant with fields, the tag outside them: its fields are
        // stored whatever the variant, which nothing else's overlap.
        if tag.encoding == TagEncoding::Direct && with_data.len() <= 1 {
            if let Some(&v) = with_data.first() {
                store_variant(self, v);
            }
            self.store_tag_of(address, ty, values[0]);
            return;
        }
        let join = self.b.create_block();
        let blocks: Vec<Block> = variants.iter().map(|_| self.b.create_block()).collect();
        let cases: Vec<(u128, Block)> = (0..variants.len())
            .map(|v| (self.ctx.discriminant(strukt, v as u32), blocks[v]))
            .collect();
        let default = *blocks.last().expect("an enum with a tag has variants");
        self.switch_bits(values[0], default, &cases);
        for (v, &block) in blocks.iter().enumerate() {
            self.b.switch_to_block(block);
            store_variant(self, v);
            self.store_tag(address, ty, strukt, v as u32);
            self.b.ins().jump(join, &[]);
        }
        self.b.switch_to_block(join);
    }

    /// Branches to the block of the case in `cases` (bits of `value`'s
    /// type) equal to `value`, or to `default`. keel's `switch` takes at
    /// most 64 bits, so a wider `value` is compared with each case in turn.
    fn switch_bits(&mut self, value: Value, default: Block, cases: &[(u128, Block)]) {
        if self.b.function().value_type(value) != Type::I128 {
            let cases = cases
                .iter()
                .map(|&(bits, block)| {
                    let call = BlockCall {
                        block,
                        args: Vec::new(),
                    };
                    (bits as i64, call)
                })
                .collect();
            let default = BlockCall {
                block: default,
                args: Vec::new(),
            };
            self.b.ins().switch(value, default, cases);
            return;
        }
        for &(bits, block) in cases {
            let case = self.int_const(bits, IntTy::U128);
            let equal = self.b.ins().icmp(IntCC::Eq, value, case);
            let next = self.b.create_block();
            self.b.ins().brif(equal, block, &[], next, &[]);
            self.b.switch_to_block(next);
        }
        self.b.ins().jump(default, &[]);
    }

    /// Stores the tag of variant `variant` of `ty`, an enum of `strukt`,
    /// at `address`, its fields there already.
    fn store_tag(&mut self, address: Value, ty: Ty, strukt: flux_hir::StructId, variant: u32) {
        if let Some((offset, bytes, bits)) = self.ctx.tag_store(ty, strukt, variant) {
            let int = tag_int(bytes);
            let value = self.int_const(bits, int);
            let at = self.offset(address, offset);
            self.b.ins().store_with_flags(
                value,
                at,
                address_flags(&self.address_align, at, int_type(int)),
            );
        }
    }

    /// Stores a direct tag, `discriminant` (of the enum's discriminant
    /// type) truncated or extended to its width. A tag widened to a
    /// field's alignment is wider than the discriminant.
    fn store_tag_of(&mut self, address: Value, ty: Ty, value: Value) {
        let layout = self.ctx.layouts.of(ty);
        let Shape::Enum { tag: Some(tag), .. } = layout.shape else {
            return;
        };
        let from = discriminant(self.sigs, ty);
        let int = tag_int(tag.bytes);
        let value = if int.bits() < from.bits() {
            self.b.ins().trunc(int_type(int), value)
        } else if int.bits() == from.bits() {
            value
        } else if from.is_signed() {
            self.b.ins().sext(int_type(int), value)
        } else {
            self.b.ins().uext(int_type(int), value)
        };
        let at = self.offset(address, tag.offset);
        self.b.ins().store_with_flags(
            value,
            at,
            address_flags(&self.address_align, at, int_type(int)),
        );
    }

    /// The discriminant of the enum `ty` at `address`, of its
    /// discriminant type: the tag extended, or decoded from the niche.
    fn load_discriminant(&mut self, address: Value, ty: Ty) -> Value {
        let layout = self.ctx.layouts.of(ty);
        self.address_align
            .entry(address)
            .and_modify(|a| *a = (*a).min(layout.align))
            .or_insert(layout.align);
        let leaf = discriminant(self.sigs, ty);
        let TyKind::Adt(adt, _) = ty.kind() else {
            return self.int_const(0, leaf);
        };
        let strukt = adt.strukt;
        let (tag, present) = match &layout.shape {
            Shape::Enum { tag, present, .. } => (*tag, *present),
            _ => (None, 0),
        };
        let Some(tag) = tag else {
            let bits = self.ctx.discriminant(strukt, present);
            return self.int_const(bits, leaf);
        };
        let int = tag_int(tag.bytes);
        let at = self.offset(address, tag.offset);
        let raw = self.b.ins().load_with_flags(
            int_type(int),
            at,
            address_flags(&self.address_align, at, int_type(int)),
        );
        // `raw` in the discriminant's type.
        let widen = |this: &mut Self, value: Value, signed: bool| -> Value {
            if leaf.bits() > int.bits() {
                if signed {
                    this.b.ins().sext(int_type(leaf), value)
                } else {
                    this.b.ins().uext(int_type(leaf), value)
                }
            } else if leaf.bits() < int.bits() {
                this.b.ins().trunc(int_type(leaf), value)
            } else {
                value
            }
        };
        match tag.encoding {
            TagEncoding::Direct => widen(self, raw, tag.signed),
            TagEncoding::Niche {
                untagged,
                first,
                last,
                start,
            } => {
                let start = self.int_const(start, int);
                let relative = self.b.ins().isub(raw, start);
                let span = self.int_const(u128::from(last - first), int);
                let in_niche = self.b.ins().icmp(IntCC::Ule, relative, span);
                let untagged_bits = self.ctx.discriminant(strukt, untagged);
                let mut result = self.int_const(untagged_bits, leaf);
                // Discriminants that are the variants' indices, as they
                // are without explicit ones: the niche's offset plus the
                // first index, unless it is the untagged variant's.
                if (first..=last)
                    .chain([untagged])
                    .all(|v| self.ctx.discriminant(strukt, v) == u128::from(v))
                {
                    let mut is = in_niche;
                    if (first..=last).contains(&untagged) {
                        let index = self.int_const(u128::from(untagged - first), int);
                        let other = self.b.ins().icmp(IntCC::Ne, relative, index);
                        is = self.b.ins().and(is, other);
                    }
                    let index = widen(self, relative, false);
                    let first = self.int_const(u128::from(first), leaf);
                    let value = self.b.ins().iadd(index, first);
                    return self.b.ins().select(is, value, result);
                }
                for v in first..=last {
                    if v == untagged {
                        continue;
                    }
                    let index = self.int_const(u128::from(v - first), int);
                    let is = self.b.ins().icmp(IntCC::Eq, relative, index);
                    let is = self.b.ins().and(is, in_niche);
                    let bits = self.ctx.discriminant(strukt, v);
                    let value = self.int_const(bits, leaf);
                    result = self.b.ins().select(is, value, result);
                }
                result
            }
        }
    }

    /// The leaves of a `ty` at `address`: for an enum, its discriminant,
    /// then every variant's fields, those of the variants it does not
    /// hold meaningless.
    fn load_value(&mut self, address: Value, ty: Ty) -> Values {
        let mut out = Values::new();
        self.load_into(address, ty, &mut out);
        out
    }

    fn load_into(&mut self, address: Value, ty: Ty, out: &mut Values) {
        if matches!(
            ty.kind(),
            TyKind::Adt(..) | TyKind::Tuple(_) | TyKind::Closure(..)
        ) {
            let align = self.ctx.layouts.of(ty).align;
            self.address_align
                .entry(address)
                .and_modify(|a| *a = (*a).min(align))
                .or_insert(align);
        }
        let sigs = self.sigs;
        if is_memory(sigs, ty) {
            out.push(address);
            return;
        }
        if is_fat(self.sigs, ty) {
            out.push(self.b.ins().load_with_flags(
                Type::Ptr,
                address,
                address_flags(&self.address_align, address, Type::Ptr),
            ));
            let at = self.offset(address, 8);
            let meta = if is_dyn_ptr(sigs, ty) {
                Type::Ptr
            } else {
                Type::I64
            };
            out.push(self.b.ins().load_with_flags(
                meta,
                at,
                address_flags(&self.address_align, at, meta),
            ));
            return;
        }
        if let Some(variants) = flux_solve::variants(sigs, ty) {
            // An empty enum has no discriminant or scalar leaves. It may be
            // a field of an inactive variant whose flattened leaves we load.
            if variants.is_empty() {
                return;
            }
            out.push(self.load_discriminant(address, ty));
            let layout = self.ctx.layouts.of(ty);
            for (v, fields) in variants.iter().enumerate() {
                for (i, &field) in fields.iter().enumerate() {
                    let at = self.offset(address, layout.field_offset(v as u32, i as u32));
                    self.load_into(at, field, out);
                }
            }
            return;
        }
        if let Some(fields) = flux_solve::fields(sigs, ty) {
            let layout = self.ctx.layouts.of(ty);
            for (i, &field) in fields.iter().enumerate() {
                let at = self.offset(address, layout.field_offset(0, i as u32));
                self.load_into(at, field, out);
            }
            return;
        }
        if let Some(scalar) = keel_type(ty) {
            out.push(self.b.ins().load_with_flags(
                scalar,
                address,
                address_flags(&self.address_align, address, scalar),
            ));
        }
    }

    /// Stores `count` copies of `values`, the leaves of an `elem`, from
    /// `address` on: unrolled when there are few, else in a loop.
    fn repeat(&mut self, address: Value, elem: Ty, values: &[Value], count: u64) {
        let size = self.ctx.layouts.of(elem).size;
        if size == 0 {
            return;
        }
        if count <= 8 {
            for i in 0..count {
                let at = self.offset(address, size * i);
                self.store(at, elem, values);
            }
            return;
        }
        // for (p = address; p != end; p += size) *p = value
        let head = self.b.create_block();
        let body = self.b.create_block();
        let exit = self.b.create_block();
        let total = self.b.ins().iconst(Type::I64, (size * count) as i64);
        let end = self.b.ins().ptradd(address, total);
        self.b.ins().jump(head, &[address]);
        self.b.switch_to_block(head);
        let at = self.b.append_block_param(head, Type::Ptr);
        let done = self.b.ins().icmp(IntCC::Eq, at, end);
        self.b.ins().brif(done, exit, &[], body, &[]);
        self.b.switch_to_block(body);
        self.store(at, elem, values);
        let next = self.offset(at, size);
        self.b.ins().jump(head, &[next]);
        self.b.switch_to_block(exit);
    }

    /// An operand's leaves.
    fn operand(&mut self, operand: &Operand) -> Values {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.read(place),
            Operand::Const(
                Constant::Item { id, args, ty } | Constant::ItemRef { id, args, ty },
            ) => {
                let c = flux_solve::traits::resolve_const(
                    &self.ctx.hir.items,
                    self.sigs,
                    flux_ty::ConstInstance { konst: *id, args },
                );
                let known = self.ctx.const_values.get(c.konst, c.args);
                let value = match known.or_else(|| self.ctx.instance_values.get(&c)) {
                    Some(value) => value.clone(),
                    None => match self.ctx.evaluator.eval_instance(c) {
                        Ok(value) => {
                            self.ctx.instance_values.insert(c, value.clone());
                            value
                        }
                        // Reported once code generation is done.
                        Err(_) => return self.zeroes(*ty),
                    },
                };
                let value = if matches!(operand, Operand::Const(Constant::ItemRef { .. })) {
                    flux_mir::eval::Value::Ref(Arc::new(value))
                } else {
                    value
                };
                if is_memory(self.sigs, *ty) {
                    let address = self.new_slot(*ty);
                    self.store_value(address, &value, *ty);
                    return smallvec![address];
                }
                let mut leaves = Values::new();
                self.value_leaves(&value, *ty, &mut leaves);
                leaves
            }
            Operand::Const(Constant::Inline { index, ty }) => {
                let value = self
                    .ctx
                    .evaluator
                    .eval_inline_constant(
                        &self.body.inline_constants[*index as usize],
                        Default::default(),
                    )
                    .expect("inline constant passed instance collection");
                if is_memory(self.sigs, *ty) {
                    let address = self.new_slot(*ty);
                    self.store_value(address, &value, *ty);
                    return smallvec![address];
                }
                let mut leaves = Values::new();
                self.value_leaves(&value, *ty, &mut leaves);
                leaves
            }
            // A reference to a slice or `str` is two leaves.
            Operand::Const(Constant::Promoted { index, ty }) => {
                let value = flux_mir::eval::Value::Ref(Arc::new(
                    self.body.promoted[*index as usize].clone(),
                ));
                let mut leaves = Values::new();
                self.value_leaves(&value, *ty, &mut leaves);
                leaves
            }
            Operand::Const(Constant::CStr(index)) => {
                let pointer = self
                    .constant(Constant::CStr(*index))
                    .expect("C string constants have a data pointer");
                // CStr stores the byte length including its terminating NUL.
                let len = self.body.c_strings[*index as usize].len() + 1;
                let len = self.int_const(len as u128, IntTy::Usize);
                smallvec![pointer, len]
            }
            Operand::Const(constant) => self.constant(*constant).into_iter().collect(),
        }
    }

    /// Stores `value`, a constant of type `ty`, at `address`.
    fn store_value(&mut self, address: Value, value: &flux_mir::eval::Value, ty: Ty) {
        use flux_mir::eval::Value as V;
        if matches!(value, V::Uninit(_)) {
            return;
        }
        let sigs = self.sigs;
        if !is_memory(sigs, ty) {
            let mut leaves = Values::new();
            self.value_leaves(value, ty, &mut leaves);
            self.store(address, ty, &leaves);
            return;
        }
        let layout = self.ctx.layouts.of(ty);
        match (value, ty.kind()) {
            (V::Union { field, value }, _) => {
                let ty = flux_solve::fields(sigs, ty).unwrap()[*field as usize];
                self.store_value(address, value, ty);
            }
            (V::Aggregate(elements), TyKind::Array(elem, _)) => {
                let size = self.ctx.layouts.of(elem).size;
                for (i, element) in elements.iter().enumerate() {
                    let at = self.offset(address, size * i as u64);
                    self.store_value(at, element, elem);
                }
            }
            (V::Aggregate(fields), _) => {
                let types = flux_solve::fields(sigs, ty).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let at = self.offset(address, layout.field_offset(0, i as u32));
                    self.store_value(at, field, field_ty);
                }
            }
            (
                V::Variant {
                    strukt,
                    index,
                    fields,
                },
                _,
            ) => {
                let types = flux_solve::variant_fields(sigs, ty, *index).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let at = self.offset(address, layout.field_offset(*index, i as u32));
                    self.store_value(at, field, field_ty);
                }
                self.store_tag(address, ty, *strukt, *index);
            }
            _ => unreachable!("memory types are aggregates"),
        }
    }

    /// Zero leaves of type `ty`: a value for code that is never output.
    fn zeroes(&mut self, ty: Ty) -> Values {
        if is_memory(self.sigs, ty) {
            return smallvec![self.new_slot(ty)];
        }
        leaves_of(self.sigs, ty)
            .into_iter()
            .filter_map(keel_type)
            .map(|t| {
                // keel has no pointer constants: a null pointer is an
                // integer converted.
                if t == Type::Ptr {
                    let zero = self.b.ins().iconst(Type::I64, 0);
                    self.b
                        .ins()
                        .convert(keel_ir::ConvertOp::IntToPtr, Type::Ptr, zero)
                } else if t == Type::F32 {
                    self.b.ins().f32const(0.0)
                } else if t == Type::F64 {
                    self.b.ins().f64const(0.0)
                } else {
                    self.b.ins().iconst(t, 0)
                }
            })
            .collect()
    }

    /// A constant item's value, of type `ty`, as leaves.
    fn value_leaves(&mut self, value: &flux_mir::eval::Value, ty: Ty, out: &mut Values) {
        use flux_mir::eval::Value as V;
        match value {
            V::Uninit(_) => out.extend(self.zeroes(ty)),
            V::Float { bits, ty } => out.push(self.float_const(*bits, *ty)),
            V::Int { bits, ty } => out.push(self.int_const(*bits, *ty)),
            V::RawAddr(address) => {
                let bits = self.int_const(u128::from(*address), IntTy::Usize);
                out.push(
                    self.b
                        .ins()
                        .convert(keel_ir::ConvertOp::IntToPtr, Type::Ptr, bits),
                );
            }
            V::Bool(b) => out.push(self.b.ins().iconst(Type::I8, i64::from(*b))),
            V::Function(instance) => {
                let callee = self.pointer_callee(instance.function, instance.args);
                out.push(self.b.ins().symbol_addr(keel_ir::Symbol::Func(callee)));
            }
            V::DynPointer {
                pointer,
                concrete,
                object,
                mutable,
            } => {
                self.value_leaves(pointer, Ty::new(TyKind::Ref(*mutable, *concrete)), out);
                let vtable = self.ctx.vtable(*object, *concrete);
                let data = self.b.import_data(&vtable);
                out.push(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)));
            }
            V::Static(id) => {
                out.push(self.static_addr(*id));
                if let Some(len) = static_slice_metadata(self.sigs, ty, self.sigs.consts[*id]) {
                    out.push(self.int_const(u128::from(len), IntTy::Usize));
                }
            }
            V::StaticPart { id, path } => {
                let (offset, stored) = self.ctx.static_part(*id, path);
                let address = self.static_addr(*id);
                out.push(self.offset(address, offset));
                if let Some(len) = static_slice_metadata(self.sigs, ty, stored) {
                    out.push(self.int_const(u128::from(len), IntTy::Usize));
                }
            }
            V::FramePointer { .. } | V::InteriorRef(_) | V::LocalRef { .. } => {
                unreachable!("escaping transient borrows are rejected")
            }
            V::PointerOffset {
                pointer,
                source,
                offset,
                len,
                ..
            } => {
                let mut original = Values::new();
                self.value_leaves(pointer, *source, &mut original);
                out.push(self.offset(original[0], *offset));
                if self.ctx.layouts.of(ty).size > 8 {
                    out.push(self.int_const(
                        u128::from(len.expect("wide offset pointer has metadata")),
                        IntTy::Usize,
                    ));
                }
            }
            V::PointerView {
                pointer, source, ..
            } => {
                let mut original = Values::new();
                self.value_leaves(pointer, *source, &mut original);
                let count = (self.ctx.layouts.of(ty).size / 8) as usize;
                assert!(count <= original.len());
                out.extend(original.into_iter().take(count));
            }
            V::Ref(pointee) => {
                let (symbol, len) = self.ctx.referenced(pointee, ty);
                let data = self.b.import_data(&symbol);
                out.push(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)));
                if let Some(len) = len {
                    out.push(self.int_const(u128::from(len), IntTy::Usize));
                }
            }
            V::Union { .. } => unreachable!("union values use memory storage"),
            V::Aggregate(fields) => {
                let types = flux_solve::fields(self.sigs, ty).unwrap_or_default();
                for (field, &field_ty) in fields.iter().zip(types) {
                    self.value_leaves(field, field_ty, out);
                }
            }
            V::Variant {
                strukt,
                index,
                fields,
            } => {
                let bits = self.ctx.discriminant(*strukt, *index);
                let int = flux_ty::discriminant_ty(&self.ctx.hir.items, *strukt);
                out.push(self.int_const(bits, int));
                let variants = flux_solve::variants(self.sigs, ty).unwrap_or_default();
                for (v, types) in variants.into_iter().enumerate() {
                    if v as u32 == *index {
                        for (field, &field_ty) in fields.iter().zip(types) {
                            self.value_leaves(field, field_ty, out);
                        }
                    } else {
                        for &field_ty in types {
                            out.extend(self.zeroes(field_ty));
                        }
                    }
                }
            }
        }
    }

    /// A scalar operand's value; `None` for `()`.
    fn scalar(&mut self, operand: &Operand) -> Option<Value> {
        self.operand(operand).first().copied()
    }

    fn float_const(&mut self, bits: u64, ty: flux_ty::FloatTy) -> Value {
        match ty {
            flux_ty::FloatTy::F32 => self.b.ins().f32const(f32::from_bits(bits as u32)),
            flux_ty::FloatTy::F64 => self.b.ins().f64const(f64::from_bits(bits)),
        }
    }

    fn constant(&mut self, constant: Constant) -> Option<Value> {
        match constant {
            Constant::Float { bits, ty } => Some(self.float_const(bits, ty)),
            Constant::Int { bits, ty } => Some(self.int_const(bits, ty)),
            Constant::Bool(value) => Some(self.b.ins().iconst(Type::I8, i64::from(value))),
            Constant::Char(c) => Some(self.b.ins().iconst(Type::I32, i64::from(u32::from(c)))),
            Constant::Unit | Constant::FnItem(_) => None,
            Constant::CStr(index) => {
                let name = self.ctx.c_string(&self.body.c_strings[index as usize]);
                let data = self.b.import_data(&name);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)))
            }
            Constant::Item { .. } | Constant::ItemRef { .. } | Constant::Inline { .. } => {
                unreachable!("handled by `operand`")
            }
            Constant::TyConst { .. } => unreachable!("monomorphized constants are known"),
            Constant::Static { id, .. } => Some(self.static_addr(id)),
            Constant::Promoted { index, ty } => {
                let pointee = match ty.kind() {
                    TyKind::Ref(_, pointee) => pointee,
                    _ => unreachable!("promoted constants are references"),
                };
                let value = self.body.promoted[index as usize].clone();
                let name = self.ctx.promoted(&value, pointee);
                let data = self.b.import_data(&name);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)))
            }
            Constant::Fn { function, args, .. } => {
                let callee = self.pointer_callee(function, args);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Func(callee)))
            }
        }
    }

    /// The address of static `id`.
    fn static_addr(&mut self, id: flux_hir::ConstId) -> Value {
        let name = self.ctx.constant_storage(id);
        let data = self.b.import_data(&name);
        self.b.ins().symbol_addr(keel_ir::Symbol::Data(data))
    }

    /// An integer constant from its bits; 128-bit values that do not fit
    /// a sign-extended `i64` immediate are built from two halves.
    fn int_const(&mut self, bits: u128, int: IntTy) -> Value {
        let ty = int_type(int);
        let as_signed = if int.bits() < 128 {
            bits as i64
        } else {
            match i64::try_from(bits as i128) {
                Ok(value) => value,
                Err(_) => {
                    let low = self.b.ins().iconst(Type::I64, bits as u64 as i64);
                    let low = self.b.ins().uext(Type::I128, low);
                    let high = self.b.ins().iconst(Type::I128, (bits >> 64) as u64 as i64);
                    let shift = self.b.ins().iconst(Type::I128, 64);
                    let high = self.b.ins().shl(high, shift);
                    return self.b.ins().or(high, low);
                }
            }
        };
        self.b.ins().iconst(ty, as_signed)
    }

    /// The leaves of an rvalue of type `ty`.
    fn rvalue(&mut self, rvalue: &Rvalue, ty: Ty) -> Values {
        match rvalue {
            Rvalue::WithAddr(pointer, address) => {
                let mut values = self.operand(pointer);
                let address = self.scalar(address).expect("address is scalar");
                // Keep the original pointer as the address calculation's base.
                // An arbitrary wrapping byte displacement changes only its
                // address; the remaining metadata leaves stay untouched.
                let original = self
                    .b
                    .ins()
                    .convert(ConvertOp::PtrToInt, Type::I64, values[0]);
                let displacement = self.b.ins().isub(address, original);
                values[0] = self.b.ins().ptradd(values[0], displacement);
                values
            }
            Rvalue::OffsetOf(container, fields) => {
                let mut ty = *container;
                let mut offset = 0u64;
                for &field in fields {
                    ty = flux_solve::traits::normalize(&self.ctx.hir.items, self.sigs, &[], ty);
                    offset += self.ctx.layouts.of(ty).field_offset(0, field);
                    ty = flux_solve::fields(self.sigs, ty).expect("checked field offset type")
                        [field as usize];
                }
                smallvec![self.int_const(offset as u128, IntTy::Usize)]
            }
            Rvalue::InlineAsm {
                template,
                registers,
                options,
                inputs,
            } => self.inline_asm(template, registers, *options, inputs, ty),
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::Aggregate(AggregateKind::Variant(strukt, index), operands) => {
                let bits = self.ctx.discriminant(*strukt, *index);
                let int = flux_ty::discriminant_ty(&self.ctx.hir.items, *strukt);
                let mut leaves = smallvec![self.int_const(bits, int)];
                let variants = flux_solve::variants(self.sigs, ty).unwrap_or_default();
                for (v, types) in variants.into_iter().enumerate() {
                    if v as u32 == *index {
                        for operand in operands {
                            leaves.extend(self.operand(operand));
                        }
                    } else {
                        for &field_ty in types {
                            leaves.extend(self.zeroes(field_ty));
                        }
                    }
                }
                leaves
            }
            Rvalue::Aggregate(_, operands) => {
                operands.iter().flat_map(|o| self.operand(o)).collect()
            }
            Rvalue::Discriminant(place) | Rvalue::DiscriminantForDrop(place) => {
                let (location, _) = self.locate(place);
                let tag = match location {
                    Location::Vars { local, start, .. } => {
                        let var = self.variable_lists[self.vars[local.index()]][start];
                        self.b.use_var(var)
                    }
                    Location::Memory(address) => {
                        let enum_ty = self.body.place_ty(place, self.sigs);
                        self.load_discriminant(address, enum_ty)
                    }
                };
                smallvec![tag]
            }
            // A reference to a slice is the pointer it is behind.
            Rvalue::Ref { place, .. }
                if self.sigs.is_unsized(self.body.place_ty(place, self.sigs)) =>
            {
                self.slice_parts(place)
            }
            Rvalue::Ref { place, .. } => match self.locate(place).0 {
                Location::Memory(address) => smallvec![address],
                Location::Vars { .. } => unreachable!("borrowed locals are in memory"),
            },
            Rvalue::Len { place, .. } => match self.body.place_ty(place, self.sigs).kind() {
                TyKind::Array(_, len) => {
                    smallvec![self.int_const(u128::from(array_len(len)), IntTy::Usize)]
                }
                _ => smallvec![self.slice_parts(place)[1]],
            },
            // To a trait object: the pointer and the type's vtable; from
            // one, upcast, the pointer and the supertrait's vtable, which
            // the subtrait's has.
            Rvalue::Unsize(operand, to)
                if flux_ty::shared_pointee(&self.ctx.hir.items, *to).is_some() =>
            {
                let from = self.body.operand_ty(operand, self.sigs);
                let source =
                    flux_ty::shared_pointee(&self.ctx.hir.items, from).expect("Arc or Rc source");
                let target =
                    flux_ty::shared_pointee(&self.ctx.hir.items, *to).expect("Arc or Rc target");
                // Header and data addresses survive the ownership transfer.
                let values = self.operand(operand);
                let metadata = match (source.kind(), target.kind()) {
                    (TyKind::Array(_, len), TyKind::Slice(_)) => {
                        self.int_const(u128::from(array_len(len)), IntTy::Usize)
                    }
                    (_, TyKind::Dynamic(_)) if source.is_dyn() => {
                        if flux_ty::objects::same_principal(source, target)
                            || flux_ty::objects::is_auto_object(&self.ctx.hir.items, target)
                        {
                            return values;
                        }
                        let index = flux_mir::vtable::super_index(
                            &self.ctx.hir.items,
                            self.sigs,
                            source,
                            target,
                        )
                        .expect("object supertrait");
                        let address = self.offset(values[2], 8 * index as u64);
                        self.b.ins().load_with_flags(
                            Type::Ptr,
                            address,
                            address_flags(&self.address_align, address, Type::Ptr),
                        )
                    }
                    (_, TyKind::Dynamic(_)) => {
                        let name = self.ctx.vtable(target, source);
                        let data = self.b.import_data(&name);
                        self.b.ins().symbol_addr(keel_ir::Symbol::Data(data))
                    }
                    _ => unreachable!("represented Arc or Rc unsizing"),
                };
                smallvec![values[0], values[1], metadata]
            }
            Rvalue::Unsize(operand, to)
                if is_dyn_ptr(self.sigs, *to) || is_dyn_box(self.sigs, *to) =>
            {
                let from = self.body.operand_ty(operand, self.sigs);
                let (Some(pointee_of), Some(object)) = (pointee_of(from), pointee_of(*to)) else {
                    unreachable!("unsizing is of pointers")
                };
                let (pointee_of, object) = flux_solve::transparent_object_unsize(
                    &self.ctx.hir.items,
                    &self.sigs.structs,
                    pointee_of,
                    object,
                )
                .expect("validated transparent object unsizing");
                let leaves = self.operand(operand);
                // Weakening a raw pointer's mutability retains the same
                // object and vtable; it is not a supertrait upcast.
                if pointee_of == object
                    || flux_ty::objects::same_principal(pointee_of, object)
                    || pointee_of.is_dyn()
                        && flux_ty::objects::is_auto_object(&self.ctx.hir.items, object)
                {
                    return leaves;
                }
                if pointee_of.is_dyn() {
                    let index = flux_mir::vtable::super_index(
                        &self.ctx.hir.items,
                        self.sigs,
                        pointee_of,
                        object,
                    )
                    .expect("upcasting is to a supertrait");
                    let address = self.offset(leaves[1], 8 * index as u64);
                    let vtable = self.b.ins().load_with_flags(
                        Type::Ptr,
                        address,
                        address_flags(&self.address_align, address, Type::Ptr),
                    );
                    return smallvec![leaves[0], vtable];
                }
                let name = self.ctx.vtable(object, pointee_of);
                let data = self.b.import_data(&name);
                let vtable = self.b.ins().symbol_addr(keel_ir::Symbol::Data(data));
                smallvec![leaves[0], vtable]
            }
            Rvalue::Unsize(operand, _) => {
                let from = self.body.operand_ty(operand, self.sigs);
                let len = match pointee_of(from)
                    .or_else(|| flux_ty::nonnull_pointee(&self.ctx.hir.items, from))
                    .map(|ty| self.sigs.structural_tail(ty).kind())
                {
                    Some(TyKind::Array(_, len)) => array_len(len),
                    _ => 0,
                };
                let pointer = self.operand(operand)[0];
                smallvec![pointer, self.int_const(u128::from(len), IntTy::Usize)]
            }
            // The address of element `start`, and `end - start` elements.
            Rvalue::Subslice { slice, start, end } => {
                let elem = match self.body.operand_ty(slice, self.sigs).kind() {
                    TyKind::Ref(_, s) | TyKind::RawPtr(_, s) => {
                        s.sequence_elem().unwrap_or(Ty::UNIT)
                    }
                    _ => Ty::UNIT,
                };
                let size = self.ctx.layouts.of(elem).size;
                let pointer = self.operand(slice)[0];
                let start = self.operand(start)[0];
                let end = self.operand(end)[0];
                let address = self.element(pointer, start, size);
                let len = self.b.ins().isub(end, start);
                smallvec![address, len]
            }
            Rvalue::Repeat(..) => unreachable!("arrays are built in memory"),
            // A pointer to a slice or `str` as one to a slice or `str`
            // keeps its length; as a thin pointer, drops it.
            Rvalue::Cast(operand, to)
                if is_fat(self.sigs, self.body.operand_ty(operand, self.sigs)) =>
            {
                let mut leaves = self.operand(operand);
                if !is_fat(self.sigs, *to) {
                    leaves.truncate(1);
                }
                leaves
            }
            _ => self.scalar_rvalue(rvalue).into_iter().collect(),
        }
    }

    fn inline_asm(
        &mut self,
        template: &[flux_hir::AsmTemplatePiece],
        registers: &[flux_hir::AsmRegister],
        options: flux_hir::AsmOptions,
        inputs: &[Operand],
        result_ty: Ty,
    ) -> Values {
        use flux_hir::{AsmRegister, AsmTemplatePiece};
        let mut arguments = Vec::new();
        let mut types = Vec::new();
        let mut transports = Vec::new();
        let mut constraints = Vec::new();
        let output_count = registers
            .iter()
            .filter(|register| !register.input_only())
            .count();
        let mut input_constraints = Vec::new();
        let mut template_indices = Vec::new();
        for (operand, register) in inputs.iter().zip(registers) {
            if *register == AsmRegister::Const {
                template_indices.push(usize::MAX);
                continue;
            }
            let ty = self.body.operand_ty(operand, self.sigs);
            let mut value = self.operand(operand)[0];
            let original = self.b.function().value_type(value);
            let mut widened = None;
            let vector = flux_ty::inline_asm::is_vector(&self.ctx.hir.items, ty);
            if vector {
                // Rust SIMD wrappers use aggregate storage. Pack only at the
                // assembly boundary, preserving every byte of the register.
                let packed = Type::Vector(match self.ctx.layouts.of(ty).size {
                    8 => keel_ir::VectorType::I8x8,
                    16 => keel_ir::VectorType::I8x16,
                    _ => unreachable!("validated assembly vector width"),
                });
                value = self.b.ins().load_with_flags(
                    packed,
                    value,
                    address_flags(&self.address_align, value, packed),
                );
            } else if register.class() == AsmRegister::Reg && ty.as_float().is_some() {
                let int = if ty.as_float() == Some(flux_ty::FloatTy::F32) {
                    Type::I32
                } else {
                    Type::I64
                };
                value = self.b.ins().convert(ConvertOp::Bitcast, int, value);
            } else if matches!(register.class(), AsmRegister::Vreg | AsmRegister::XmmReg)
                && let Some(int) = ty.as_int()
            {
                let float = if int.bits() <= 32 {
                    Type::F32
                } else {
                    Type::F64
                };
                if int.bits() < 32 {
                    widened = Some(Type::I32);
                    value = self.b.ins().convert(ConvertOp::Uext, Type::I32, value);
                }
                value = self.b.ins().convert(ConvertOp::Bitcast, float, value);
            }
            arguments.push(value);
            let class = if matches!(register.class(), AsmRegister::Reg | AsmRegister::RegByte) {
                "r"
            } else {
                "w"
            };
            if register.input_only() {
                template_indices.push(output_count + arguments.len() - 1);
                input_constraints.push(class.to_owned());
            } else {
                template_indices.push(types.len());
                input_constraints.push(types.len().to_string());
                types.push(self.b.function().value_type(value));
                transports.push((original, widened, vector.then_some(ty)));
                // Keep outputs distinct from unrelated inputs even when their
                // incoming values are equal. This is conservative for late outputs.
                constraints.push(format!("=&{class}"));
            }
        }
        constraints.extend(input_constraints);
        if !options.nomem {
            constraints.push("~{memory}".to_owned());
        }
        if !options.preserves_flags {
            constraints.push("~{cc}".to_owned());
        }
        // Keel expands GNU register operands with percent prefixes. Intel
        // prefix syntax preserves Rust's operand order while accepting them.
        let mut text = if cfg!(target_arch = "x86_64") {
            String::from(".intel_syntax prefix\n")
        } else {
            String::new()
        };
        for piece in template {
            match piece {
                AsmTemplatePiece::Text(literal) => {
                    text.push_str(&literal.as_str().replace('%', "%%"))
                }
                AsmTemplatePiece::Operand { index, modifier } => {
                    use std::fmt::Write;
                    if registers[*index as usize] == AsmRegister::Const {
                        let Operand::Const(Constant::Int { bits, .. }) = inputs[*index as usize]
                        else {
                            unreachable!("assembly constant is an integer literal");
                        };
                        write!(text, "{bits}").unwrap();
                        continue;
                    }

                    let modifier = if cfg!(target_arch = "x86_64") {
                        match registers[*index as usize].class() {
                            AsmRegister::RegByte => 'b',
                            AsmRegister::XmmReg => 'x',
                            _ => match modifier {
                                Some('l') => 'b',
                                Some('x') => 'w',
                                Some('e') => 'k',
                                Some('r') | None => 'q',
                                _ => unreachable!("validated x86 register modifier"),
                            },
                        }
                    } else {
                        modifier.unwrap_or(match registers[*index as usize].class() {
                            AsmRegister::Reg => 'x',
                            AsmRegister::Vreg => 'v',
                            _ => unreachable!(),
                        })
                    };
                    let index = template_indices[*index as usize];
                    write!(text, "%{modifier}[{index}]").unwrap();
                }
            }
        }
        let asm = keel_ir::InlineAsm::new(&text, &constraints.join(","), &types)
            .expect("checked assembly operands");
        let instruction = self.b.ins().inline_asm(asm, &arguments);
        let results = self.b.function().inst_results(instruction).to_vec();
        // Any vector makes the whole result tuple memory-resident. Build one
        // tuple allocation, rather than returning separate vector addresses
        // that could be mistaken for the address of that entire tuple.
        let address = is_memory(self.sigs, result_ty).then(|| self.new_slot(result_ty));
        let layout = self.ctx.layouts.of(result_ty);
        let fields = flux_solve::fields(self.sigs, result_ty).unwrap_or_default();
        let mut values = Values::new();
        for (index, (mut value, (original, widened, vector))) in
            results.into_iter().zip(transports).enumerate()
        {
            let actual = self.b.function().value_type(value);
            if vector.is_none() {
                value = if let Some(wide) = widened {
                    value = self.b.ins().convert(ConvertOp::Bitcast, wide, value);
                    self.b.ins().convert(ConvertOp::Trunc, original, value)
                } else if original != actual {
                    self.b.ins().convert(ConvertOp::Bitcast, original, value)
                } else {
                    value
                };
            }
            if let Some(address) = address {
                let field = self.offset(address, layout.field_offset(0, index as u32));
                if vector.is_some() {
                    self.b.ins().store_with_flags(
                        value,
                        field,
                        address_flags(&self.address_align, field, actual),
                    );
                } else {
                    self.store(field, fields[index], &[value]);
                }
            } else {
                values.push(value);
            }
        }
        if let Some(address) = address {
            values.push(address);
        }
        values
    }

    fn scalar_rvalue(&mut self, rvalue: &Rvalue) -> Option<Value> {
        match rvalue {
            Rvalue::WithAddr(..)
            | Rvalue::OffsetOf(..)
            | Rvalue::Use(_)
            | Rvalue::InlineAsm { .. }
            | Rvalue::Aggregate(..)
            | Rvalue::Ref { .. }
            | Rvalue::Discriminant(_)
            | Rvalue::DiscriminantForDrop(_)
            | Rvalue::Len { .. }
            | Rvalue::Repeat(..)
            | Rvalue::Unsize(..)
            | Rvalue::Subslice { .. } => {
                unreachable!("not scalar operations")
            }
            Rvalue::UnaryOp(op, operand) => {
                let ty = self.body.operand_ty(operand, self.sigs);
                let value = self.scalar(operand)?;
                Some(match op {
                    UnOp::Neg if ty.as_float().is_some() => self.b.ins().fneg(value),
                    UnOp::Neg => self.b.ins().ineg(value),
                    UnOp::Not if ty == Ty::BOOL => {
                        let one = self.b.ins().iconst(Type::I8, 1);
                        self.b.ins().xor(value, one)
                    }
                    UnOp::Not => self.b.ins().bnot(value),
                })
            }
            Rvalue::BinaryOp(op, l, r) => self.binary(*op, l, r),
            Rvalue::Cast(operand, to) => {
                let from = self.body.operand_ty(operand, self.sigs);
                let value = self.scalar(operand)?;
                let to_ty = keel_type(*to)?;
                if from.as_float().is_some() || to.as_float().is_some() {
                    return Some(self.float_cast(value, from, *to));
                }
                // Pointers are 64-bit addresses: a pointer as another is
                // itself, as an integer its address truncated, and an
                // integer as a pointer is extended to 64 bits first.
                let (from_ptr, to_ptr) = (
                    from.is_pointer() || matches!(from.kind(), TyKind::FnPtr(_)),
                    to.is_pointer() || matches!(to.kind(), TyKind::FnPtr(_)),
                );
                if from_ptr && to_ptr {
                    return Some(value);
                }
                if from_ptr {
                    let address =
                        self.b
                            .ins()
                            .convert(keel_ir::ConvertOp::PtrToInt, Type::I64, value);
                    return Some(match bits(*to) {
                        64 => address,
                        128 => self.b.ins().uext(to_ty, address),
                        _ => self.b.ins().trunc(to_ty, address),
                    });
                }
                if to_ptr {
                    let address = match bits(from) {
                        64 => value,
                        128 => self.b.ins().trunc(Type::I64, value),
                        _ if from.is_signed_int() => self.b.ins().sext(Type::I64, value),
                        _ => self.b.ins().uext(Type::I64, value),
                    };
                    return Some(self.b.ins().convert(
                        keel_ir::ConvertOp::IntToPtr,
                        Type::Ptr,
                        address,
                    ));
                }
                let (from_bits, to_bits) = (bits(from), bits(*to));
                Some(if from_bits == to_bits {
                    value
                } else if to_bits < from_bits {
                    self.b.ins().trunc(to_ty, value)
                } else if from.is_signed_int() {
                    self.b.ins().sext(to_ty, value)
                } else {
                    self.b.ins().uext(to_ty, value)
                })
            }
        }
    }

    fn float_cast(&mut self, value: Value, from: Ty, to: Ty) -> Value {
        use keel_ir::ConvertOp;
        let target = keel_type(to).expect("numeric scalar");
        if from.as_int().is_some_and(|i| i.bits() == 128) {
            return self.wide_int_to_float(value, from, to);
        }
        if to.as_int().is_some_and(|i| i.bits() == 128) {
            return self.float_to_wide_int(value, from, to);
        }
        match (from.as_float(), to.as_float()) {
            (Some(a), Some(b)) if a == b => value,
            (Some(a), Some(b)) => self.b.ins().convert(
                if a.bits() < b.bits() {
                    ConvertOp::Fpext
                } else {
                    ConvertOp::Fptrunc
                },
                target,
                value,
            ),
            (None, Some(_)) => self.b.ins().convert(
                if from.is_signed_int() {
                    ConvertOp::Sitofp
                } else {
                    ConvertOp::Uitofp
                },
                target,
                value,
            ),
            (Some(_), None) => {
                let wide = if bits(to) < 32 { Type::I32 } else { target };
                let converted = self.b.ins().convert(
                    if to.is_signed_int() {
                        ConvertOp::Fptosi
                    } else {
                        ConvertOp::Fptoui
                    },
                    wide,
                    value,
                );
                if bits(to) >= 32 {
                    return converted;
                }
                let maximum = self.b.ins().iconst(
                    wide,
                    ((1u64 << (bits(to) - u32::from(to.is_signed_int()))) - 1) as i64,
                );
                let minimum = self.b.ins().iconst(
                    wide,
                    if to.is_signed_int() {
                        -(1i64 << (bits(to) - 1))
                    } else {
                        0
                    },
                );
                let below = self.b.ins().icmp(
                    if to.is_signed_int() {
                        IntCC::Slt
                    } else {
                        IntCC::Ult
                    },
                    converted,
                    minimum,
                );
                let lower = self.b.ins().select(below, minimum, converted);
                let above = self.b.ins().icmp(
                    if to.is_signed_int() {
                        IntCC::Sgt
                    } else {
                        IntCC::Ugt
                    },
                    lower,
                    maximum,
                );
                let clamped = self.b.ins().select(above, maximum, lower);
                self.b.ins().trunc(target, clamped)
            }
            _ => unreachable!("a float cast has a float operand or result"),
        }
    }

    /// Round the integer significand once, to nearest-even, before an exact
    /// power-of-two scaling. Splitting into two float additions double-rounds.
    fn wide_int_to_float(&mut self, value: Value, from: Ty, to: Ty) -> Value {
        use keel_ir::ConvertOp;
        let zero = self.int_const(0, IntTy::U128);
        let negative = self.b.ins().icmp(IntCC::Slt, value, zero);
        let magnitude = if from.is_signed_int() {
            let neg = self.b.ins().ineg(value);
            self.b.ins().select(negative, neg, value)
        } else {
            value
        };
        let sixtyfour = self.int_const(64, IntTy::U128);
        let high = self.b.ins().ushr(magnitude, sixtyfour);
        let high = self.b.ins().trunc(Type::I64, high);
        let low = self.b.ins().trunc(Type::I64, magnitude);
        let highzero = self.b.ins().iconst(Type::I64, 0);
        let has_high = self.b.ins().icmp(IntCC::Ne, high, highzero);
        let hc = self.b.ins().clz(high);
        let lc = self.b.ins().clz(low);
        let c128 = self.b.ins().iconst(Type::I64, 128);
        let c64 = self.b.ins().iconst(Type::I64, 64);
        let hlen = self.b.ins().isub(c128, hc);
        let llen = self.b.ins().isub(c64, lc);
        let length = self.b.ins().select(has_high, hlen, llen);
        let float = to.as_float().expect("float target");
        let precision = if float == flux_ty::FloatTy::F32 {
            24
        } else {
            53
        };
        let p = self.b.ins().iconst(Type::I64, precision);
        let large = self.b.ins().icmp(IntCC::Ugt, length, p);
        let difference = self.b.ins().isub(length, p);
        let shift = self.b.ins().select(large, difference, highzero);
        let shift128 = self.b.ins().uext(Type::I128, shift);
        let quotient = self.b.ins().ushr(magnitude, shift128);
        let one = self.int_const(1, IntTy::U128);
        let power = self.b.ins().shl(one, shift128);
        let mask = self.b.ins().isub(power, one);
        let remainder = self.b.ins().and(magnitude, mask);
        let previous = self.b.ins().isub(shift128, one);
        let half = self.b.ins().shl(one, previous);
        let above = self.b.ins().icmp(IntCC::Ugt, remainder, half);
        let tie = self.b.ins().icmp(IntCC::Eq, remainder, half);
        let odd = self.b.ins().and(quotient, one);
        let odd = self.b.ins().icmp(IntCC::Ne, odd, zero);
        let tie_odd = self.b.ins().and(tie, odd);
        let up = self.b.ins().or(above, tie_odd);
        let increment = self.b.ins().uext(Type::I128, up);
        let rounded = self.b.ins().iadd(quotient, increment);
        let rounded = self.b.ins().trunc(Type::I64, rounded);
        let target = keel_type(to).expect("float target");
        let rounded = self.b.ins().convert(ConvertOp::Uitofp, target, rounded);
        let bias = self
            .b
            .ins()
            .iconst(Type::I64, if precision == 24 { 127 } else { 1023 });
        let exponent = self.b.ins().iadd(shift, bias);
        let fraction = self.b.ins().iconst(Type::I64, precision - 1);
        let scale = self.b.ins().shl(exponent, fraction);
        let scale = if precision == 24 {
            self.b.ins().trunc(Type::I32, scale)
        } else {
            scale
        };
        let scale = self.b.ins().convert(ConvertOp::Bitcast, target, scale);
        let result = self.b.ins().fmul(rounded, scale);
        if from.is_signed_int() {
            let neg = self.b.ins().fneg(result);
            self.b.ins().select(negative, neg, result)
        } else {
            result
        }
    }

    /// Decode IEEE bits and truncate the exact significand, then clamp to the
    /// integer range. This also handles NaN and infinity without a backend
    /// float-to-i128 conversion or an unchecked native conversion helper.
    fn float_to_wide_int(&mut self, value: Value, from: Ty, to: Ty) -> Value {
        use keel_ir::ConvertOp;
        let single = from.as_float() == Some(flux_ty::FloatTy::F32);
        let raw = self.b.ins().convert(
            ConvertOp::Bitcast,
            if single { Type::I32 } else { Type::I64 },
            value,
        );
        let raw = if single {
            self.b.ins().uext(Type::I64, raw)
        } else {
            raw
        };
        let precision = if single { 23 } else { 52 };
        let exponent_mask = if single { 255 } else { 2047 };
        let sign_bit = self.b.ins().iconst(Type::I64, if single { 31 } else { 63 });
        let sign = self.b.ins().ushr(raw, sign_bit);
        let zero64 = self.b.ins().iconst(Type::I64, 0);
        let negative = self.b.ins().icmp(IntCC::Ne, sign, zero64);
        let fraction_mask = self.b.ins().iconst(Type::I64, (1i64 << precision) - 1);
        let fraction = self.b.ins().and(raw, fraction_mask);
        let precision_value = self.b.ins().iconst(Type::I64, precision);
        let encoded_exp = self.b.ins().ushr(raw, precision_value);
        let exp_mask = self.b.ins().iconst(Type::I64, exponent_mask);
        let encoded_exp = self.b.ins().and(encoded_exp, exp_mask);
        let exp_all = self.b.ins().icmp(IntCC::Eq, encoded_exp, exp_mask);
        let frac_nonzero = self.b.ins().icmp(IntCC::Ne, fraction, zero64);
        let nan = self.b.ins().and(exp_all, frac_nonzero);
        let bias = self
            .b
            .ins()
            .iconst(Type::I64, if single { 127 } else { 1023 });
        let exponent = self.b.ins().isub(encoded_exp, bias);
        let hidden = self.b.ins().iconst(Type::I64, 1i64 << precision);
        let significand = self.b.ins().or(fraction, hidden);
        let significand = self.b.ins().uext(Type::I128, significand);
        let shift = self.b.ins().isub(exponent, precision_value);
        let left = self.b.ins().icmp(IntCC::Sge, shift, zero64);
        let neg_shift = self.b.ins().ineg(shift);
        let left_shift = self.b.ins().uext(Type::I128, shift);
        let right_shift = self.b.ins().uext(Type::I128, neg_shift);
        let l = self.b.ins().shl(significand, left_shift);
        let r = self.b.ins().ushr(significand, right_shift);
        let magnitude = self.b.ins().select(left, l, r);
        let zero = self.int_const(0, IntTy::U128);
        let limit = self
            .b
            .ins()
            .iconst(Type::I64, if to.is_signed_int() { 127 } else { 128 });
        let overflow = self.b.ins().icmp(IntCC::Sge, exponent, limit);
        let mut result = if to.is_signed_int() {
            let negative_value = self.b.ins().ineg(magnitude);
            let signed = self.b.ins().select(negative, negative_value, magnitude);
            let min = self.int_const(1u128 << 127, IntTy::U128);
            let max = self.int_const((1u128 << 127) - 1, IntTy::U128);
            let bound = self.b.ins().select(negative, min, max);
            self.b.ins().select(overflow, bound, signed)
        } else {
            let max = self.int_const(u128::MAX, IntTy::U128);
            let clamped = self.b.ins().select(overflow, max, magnitude);
            self.b.ins().select(negative, zero, clamped)
        };
        let below_one = self.b.ins().icmp(IntCC::Slt, exponent, zero64);
        let zero_result = self.b.ins().or(below_one, nan);
        result = self.b.ins().select(zero_result, zero, result);
        result
    }

    fn binary(&mut self, op: BinOp, l: &Operand, r: &Operand) -> Option<Value> {
        let ty = self.body.operand_ty(l, self.sigs);
        let signed = ty.is_signed_int();
        if ty == Ty::UNIT {
            // `() == ()`: the only value of `()` equals itself.
            let equal = matches!(op, BinOp::Eq | BinOp::Le | BinOp::Ge);
            return Some(self.b.ins().iconst(Type::I8, i64::from(equal)));
        }
        if matches!(ty.kind(), TyKind::RawPtr(..)) && matches!(op, BinOp::Eq | BinOp::Ne) {
            let left = self.operand(l);
            let right = self.operand(r);
            assert_eq!(left.len(), right.len());
            let mut equal = self.b.ins().icmp(IntCC::Eq, left[0], right[0]);
            for (&a, &b) in left.iter().zip(&right).skip(1) {
                let component = self.b.ins().icmp(IntCC::Eq, a, b);
                equal = self.b.ins().and(equal, component);
            }
            return Some(if op == BinOp::Ne {
                {
                    let one = self.b.ins().iconst(Type::I8, 1);
                    self.b.ins().xor(equal, one)
                }
            } else {
                equal
            });
        }
        let a = self.scalar(l)?;
        let b = self.scalar(r)?;
        if ty.as_float().is_some() {
            use keel_ir::FloatCC;
            return Some(match op {
                BinOp::Add => self.b.ins().fadd(a, b),
                BinOp::Sub => self.b.ins().fsub(a, b),
                BinOp::Mul => self.b.ins().fmul(a, b),
                BinOp::Div => self.b.ins().fdiv(a, b),
                BinOp::Eq => self.b.ins().fcmp(FloatCC::Eq, a, b),
                BinOp::Ne => self.b.ins().fcmp(FloatCC::Ne, a, b),
                BinOp::Lt => self.b.ins().fcmp(FloatCC::Lt, a, b),
                BinOp::Le => self.b.ins().fcmp(FloatCC::Le, a, b),
                BinOp::Gt => self.b.ins().fcmp(FloatCC::Gt, a, b),
                BinOp::Ge => self.b.ins().fcmp(FloatCC::Ge, a, b),
                BinOp::Rem => {
                    let scalar = keel_type(ty).expect("float scalar");
                    let callee = self.b.import_function(
                        if scalar == Type::F32 { "fmodf" } else { "fmod" },
                        Signature::new(&[scalar, scalar], &[scalar]),
                    );
                    let call = self.b.ins().call(callee, &[a, b]);
                    self.b.function().inst_results(call)[0]
                }
                _ => unreachable!("typed float operation"),
            });
        }
        let cmp = |signed_cc, unsigned_cc| if signed { signed_cc } else { unsigned_cc };
        Some(match op {
            BinOp::Add => self.b.ins().iadd(a, b),
            BinOp::Sub => self.b.ins().isub(a, b),
            BinOp::Mul => self.b.ins().imul(a, b),
            BinOp::Div if signed => self.b.ins().sdiv(a, b),
            BinOp::Div => self.b.ins().udiv(a, b),
            BinOp::Rem if signed => self.b.ins().srem(a, b),
            BinOp::Rem => self.b.ins().urem(a, b),
            BinOp::BitAnd => self.b.ins().and(a, b),
            BinOp::BitOr => self.b.ins().or(a, b),
            BinOp::BitXor => self.b.ins().xor(a, b),
            // keel takes shift amounts of any integer type, modulo the
            // shifted value's width, which is Rust's masking.
            BinOp::Shl => self.b.ins().shl(a, b),
            BinOp::Shr if signed => self.b.ins().sshr(a, b),
            BinOp::Shr => self.b.ins().ushr(a, b),
            BinOp::Eq => self.b.ins().icmp(IntCC::Eq, a, b),
            BinOp::Ne => self.b.ins().icmp(IntCC::Ne, a, b),
            BinOp::Lt => self.b.ins().icmp(cmp(IntCC::Slt, IntCC::Ult), a, b),
            BinOp::Le => self.b.ins().icmp(cmp(IntCC::Sle, IntCC::Ule), a, b),
            BinOp::Gt => self.b.ins().icmp(cmp(IntCC::Sgt, IntCC::Ugt), a, b),
            BinOp::Ge => self.b.ins().icmp(cmp(IntCC::Sge, IntCC::Uge), a, b),
        })
    }

    fn terminator(&mut self, kind: &TerminatorKind, offset: u32, block: flux_mir::BasicBlock) {
        self.unwind_action = self.body.blocks[block].terminator.unwind;
        match kind {
            TerminatorKind::Goto(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
                variants,
            } => {
                let targets: Vec<(u128, flux_mir::BasicBlock)> = match *variants {
                    Some(strukt) => targets
                        .iter()
                        .map(|&(v, block)| (self.ctx.discriminant(strukt, v as u32), block))
                        .collect(),
                    None => targets.clone(),
                };
                let Some(value) = self.scalar(discr) else {
                    self.b.ins().jump(self.blocks[otherwise.index()], &[]);
                    return;
                };
                let otherwise = self.blocks[otherwise.index()];
                match targets.as_slice() {
                    [(0, zero)] => {
                        let zero = self.blocks[zero.index()];
                        self.b.ins().brif(value, otherwise, &[], zero, &[]);
                    }
                    _ => {
                        let cases: Vec<(u128, Block)> = targets
                            .iter()
                            .map(|&(bits, block)| (bits, self.blocks[block.index()]))
                            .collect();
                        self.switch_bits(value, otherwise, &cases);
                    }
                }
            }
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest,
                target,
            } if flux_mir::vtable::virtual_call(
                &self.ctx.hir.items,
                self.ctx.resolve_instance(Instance {
                    function: *func,
                    args: generic_args,
                }),
            )
            .is_some() =>
            {
                self.virtual_call(*func, generic_args, args, dest, *target);
            }
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest,
                target,
            } => {
                let items = &self.ctx.hir.items;
                let offset = self.body.fn_spans.get(&block).copied().unwrap_or(offset);
                if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::BoxCallOnce) {
                    let trait_ = flux_ty::fn_trait(items, flux_ty::ClosureKind::FnOnce)
                        .expect("embedded FnOnce trait");
                    let method = items.traits[trait_]
                        .function(items, flux_intern::Symbol::intern("call_once"))
                        .expect("embedded FnOnce method");
                    self.virtual_call(
                        method,
                        Ty::list(&[generic_args[1], generic_args[0]]),
                        args,
                        dest,
                        *target,
                    );
                    return;
                }
                if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::PanicCatch) {
                    self.catch_unwind(args, dest, *target);
                    return;
                }
                if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::CallerLocation) {
                    let location = self.location_here(offset);
                    self.write(dest, &[location]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    }
                    return;
                }
                if matches!(
                    items.functions[*func].intrinsic,
                    Some(flux_hir::Intrinsic::Fmaf32 | flux_hir::Intrinsic::Fmaf64)
                ) {
                    let values: Vec<_> = args
                        .iter()
                        .map(|arg| self.scalar(arg).expect("float argument"))
                        .collect();
                    let result = self.b.ins().fma(values[0], values[1], values[2]);
                    self.write(dest, &[result]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if let Some(intrinsic) = items.functions[*func]
                    .intrinsic
                    .filter(|intrinsic| intrinsic.is_bit_count())
                {
                    let ty = keel_type(generic_args[0]).expect("an integer argument");
                    let x = self.scalar(&args[0]).expect("an integer argument");
                    let ins = self.b.ins();
                    let result = match intrinsic {
                        // A byte is its own reverse; keel's `bswap` needs two.
                        flux_hir::Intrinsic::Bswap if ty == Type::I8 => x,
                        flux_hir::Intrinsic::Bswap => ins.bswap(x),
                        // Counts come in the argument's type, and are `u32`s.
                        count => {
                            let n = match count {
                                flux_hir::Intrinsic::Ctlz => ins.clz(x),
                                flux_hir::Intrinsic::Cttz => ins.ctz(x),
                                _ => ins.popcnt(x),
                            };
                            match ty {
                                Type::I32 => n,
                                Type::I8 | Type::I16 => {
                                    self.b.ins().convert(ConvertOp::Uext, Type::I32, n)
                                }
                                _ => self.b.ins().convert(ConvertOp::Trunc, Type::I32, n),
                            }
                        }
                    };
                    self.write(dest, &[result]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if items.functions[*func]
                    .intrinsic
                    .is_some_and(flux_hir::Intrinsic::is_x86)
                {
                    self.x86_intrinsic(*func, generic_args, args, dest, *target);
                    return;
                }
                if items.functions[*func]
                    .intrinsic
                    .is_some_and(flux_hir::Intrinsic::is_neon)
                {
                    self.neon_intrinsic(*func, generic_args, args, dest, *target);
                    return;
                }
                if let Some(
                    intrinsic @ (flux_hir::Intrinsic::Copy
                    | flux_hir::Intrinsic::CopyNonoverlapping),
                ) = items.functions[*func].intrinsic
                {
                    let source = self.scalar(&args[0]).expect("copy source pointer");
                    let destination = self.scalar(&args[1]).expect("copy destination pointer");
                    let count = self.scalar(&args[2]).expect("copy count");
                    let size = self.ctx.layouts.of(generic_args[0]).size;
                    if size != 0 {
                        let stride = self.b.ins().iconst(Type::I64, size as i64);
                        let bytes = self.b.ins().imul(count, stride);
                        let operation = if intrinsic == flux_hir::Intrinsic::Copy {
                            Intrinsic::Memmove
                        } else {
                            Intrinsic::Memcpy
                        };
                        self.b
                            .ins()
                            .intrinsic(operation, &[destination, source, bytes]);
                    }
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::Transmute) {
                    let from = generic_args[0];
                    let to = generic_args[1];
                    let source = self.ctx.layouts.of(from);
                    let destination = self.ctx.layouts.of(to);
                    assert_eq!(source.size, destination.size, "validated transmute sizes");
                    let values = self.operand(&args[0]);
                    // An integer and a float of one size: a register move.
                    let int_float = |a: Ty, b: Ty| a.as_int().is_some() && b.as_float().is_some();
                    if int_float(from, to) || int_float(to, from) {
                        let target = keel_type(to).expect("a numeric scalar");
                        let value = self.b.ins().convert(ConvertOp::Bitcast, target, values[0]);
                        self.write(dest, &[value]);
                    } else {
                        let address =
                            self.aligned_slot(source.size, source.align.max(destination.align));
                        self.store(address, from, &values);
                        let values = self.load(Location::Memory(address), to);
                        self.write(dest, &values);
                    }
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if let Some(
                    intrinsic @ (flux_hir::Intrinsic::ReadVolatile
                    | flux_hir::Intrinsic::WriteVolatile),
                ) = items.functions[*func].intrinsic
                {
                    let ty = generic_args[0];
                    let layout = self.ctx.layouts.of(ty);
                    let scalar = flux_mir::mono::volatile_scalar(self.sigs, ty).filter(|&scalar| {
                        let scalar_layout = self.ctx.layouts.of(scalar);
                        layout.size == scalar_layout.size && layout.align == scalar_layout.align
                    });
                    if scalar.is_none() && layout.size != 0 {
                        let slot = self.aligned_slot(layout.size, layout.align);
                        let address = self.operand(&args[0])[0];
                        let read = intrinsic == flux_hir::Intrinsic::ReadVolatile;
                        if !read {
                            let values = self.operand(&args[1]);
                            self.store(slot, ty, &values);
                        }
                        let (source, destination) = if read {
                            (address, slot)
                        } else {
                            (slot, address)
                        };
                        let mut offset = 0;
                        for (bytes, machine) in [
                            (8, Type::I64),
                            (4, Type::I32),
                            (2, Type::I16),
                            (1, Type::I8),
                        ] {
                            while offset + bytes <= layout.size {
                                let from = self.offset(source, offset);
                                let to = self.offset(destination, offset);
                                let mut load = self.chunk_flags(from, layout.align, offset, bytes);
                                let mut store = self.chunk_flags(to, layout.align, offset, bytes);
                                load.volatile = read;
                                store.volatile = !read;
                                let value = self.b.ins().load_with_flags(machine, from, load);
                                self.b.ins().store_with_flags(value, to, store);
                                offset += bytes;
                            }
                        }
                        if read {
                            let values = self.load(Location::Memory(slot), ty);
                            self.write(dest, &values);
                        }
                    } else if let Some(machine) = scalar.and_then(keel_type) {
                        let address = self.operand(&args[0])[0];
                        let mut flags = aligned(machine);
                        flags.volatile = true;
                        if intrinsic == flux_hir::Intrinsic::ReadVolatile {
                            let value = self.b.ins().load_with_flags(machine, address, flags);
                            self.write(dest, &[value]);
                        } else {
                            let value = self.operand(&args[1])[0];
                            self.b.ins().store_with_flags(value, address, flags);
                        }
                    } else {
                        // Rust zero-sized volatile operations do not access memory.
                        self.write(dest, &[]);
                    }
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::ReadUnaligned) {
                    // Validated after substitution by mono. No reference to the
                    // source is formed. Keel's default load flags promise only
                    // alignment one (and do not promise fault freedom).
                    let ty = keel_type(generic_args[0]).expect("validated unsigned scalar");
                    let address = self.operand(&args[0])[0];
                    let value = self.b.ins().load(ty, address);
                    self.write(dest, &[value]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if items.functions[*func].intrinsic.is_some()
                    && let Some((op, order, word)) =
                        flux_mir::mono::atomic_intrinsic(items.functions[*func].name.as_str())
                {
                    use keel_ir::{Ordering, RmwOp};
                    let ordering = match order {
                        "relaxed" => Ordering::Relaxed,
                        "acquire" => Ordering::Acquire,
                        "release" => Ordering::Release,
                        "acqrel" => Ordering::AcqRel,
                        "seqcst" => Ordering::SeqCst,
                        _ => unreachable!("validated atomic order"),
                    };
                    let values: Vec<_> = args.iter().map(|arg| self.operand(arg)[0]).collect();
                    let word = match word {
                        flux_mir::mono::AtomicWord::U8 => Type::I8,
                        flux_mir::mono::AtomicWord::U16 => Type::I16,
                        flux_mir::mono::AtomicWord::U32 => Type::I32,
                        flux_mir::mono::AtomicWord::Usize => Type::I64,
                        flux_mir::mono::AtomicWord::Ptr => Type::Ptr,
                    };
                    let value = match op {
                        "load" => Some(self.b.ins().atomic_load(word, ordering, values[0])),
                        "store" => {
                            self.b.ins().atomic_store(ordering, values[1], values[0]);
                            None
                        }
                        "add" | "sub" => Some(self.b.ins().atomic_rmw(
                            if op == "add" { RmwOp::Add } else { RmwOp::Sub },
                            ordering,
                            values[0],
                            values[1],
                        )),
                        "cas" => Some(
                            self.b
                                .ins()
                                .atomic_cas(
                                    ordering,
                                    ordering.cas_failure(),
                                    values[0],
                                    values[1],
                                    values[2],
                                )
                                .0,
                        ),
                        "fence" => {
                            self.b.ins().fence(ordering);
                            None
                        }
                        _ => unreachable!("validated atomic operation"),
                    };
                    if let Some(value) = value {
                        self.write(dest, &[value]);
                    }
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::PtrMetadataWord) {
                    let parts = self.operand(&args[0]);
                    let metadata = match self.sigs.metadata(generic_args[0]) {
                        flux_ty::Metadata::Thin => self.b.ins().iconst(Type::I64, 0),
                        flux_ty::Metadata::Length => parts[1],
                        flux_ty::Metadata::VTable => {
                            self.b
                                .ins()
                                .convert(ConvertOp::PtrToInt, Type::I64, parts[1])
                        }
                        flux_ty::Metadata::Generic(_) => {
                            unreachable!("monomorphized pointer metadata")
                        }
                    };
                    self.write(dest, &[metadata]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                // Of a trait object: through its vtable.
                if items.functions[*func]
                    .intrinsic
                    .is_some_and(|intrinsic| !intrinsic.is_coroutine())
                {
                    self.dyn_intrinsic(*func, args, dest, *target);
                    return;
                }
                let callee = self.callee(*func, generic_args);
                let resolved = self.ctx.resolve_instance(Instance {
                    function: *func,
                    args: generic_args,
                });
                let location = self
                    .ctx
                    .takes_location(resolved)
                    .then(|| self.location_here(offset));
                let ret = flux_solve::traits::instance_ty(
                    &self.ctx.hir.items,
                    &self.ctx.typed.sigs,
                    self.ctx.typed.sigs[*func].ret,
                    generic_args,
                );
                // The callee's own convention, as `callee` declared it:
                // the two Rust ones return different shapes.
                let convention = if items.functions[resolved.function].abi.is_c() {
                    Convention::C
                } else if self.ctx.compilation.is_some() || self.ctx.reified.contains(&resolved) {
                    Convention::RustReified
                } else {
                    Convention::RustFast
                };
                self.call_abi(
                    convention,
                    ret,
                    args,
                    dest,
                    *target,
                    Call::Direct { callee, location },
                );
            }
            TerminatorKind::CallIndirect {
                callee,
                args,
                dest,
                target,
            } => {
                let ty = self.body.operand_ty(callee, self.sigs);
                let sig = ty
                    .fn_sig()
                    .expect("indirect calls are of function pointers");
                let is_c = sig.abi.is_c();
                let sig = FnSig {
                    params: sig.params().into(),
                    ret: sig.ret(),
                };
                let signature = if is_c {
                    c_signature(self.sigs, &self.ctx.layouts, &sig, false)
                } else {
                    signature(self.sigs, &self.ctx.layouts, &sig, false)
                };
                let sig_ref = self.b.import_signature(signature);
                let pointer = self.scalar(callee).expect("function pointers are scalars");
                self.call_abi(
                    if is_c {
                        Convention::C
                    } else {
                        Convention::RustReified
                    },
                    sig.ret,
                    args,
                    dest,
                    *target,
                    Call::Indirect {
                        sig: sig_ref,
                        pointer,
                    },
                );
            }
            TerminatorKind::Assert {
                cond,
                expected,
                msg,
                target,
            } => {
                let target = self.blocks[target.index()];
                let Some(cond) = self.scalar(cond) else {
                    self.b.ins().jump(target, &[]);
                    return;
                };
                // `core`'s function for the check, as rustc's.
                let (lang, operands): (&str, Vec<&Operand>) = match msg {
                    AssertMessage::BoundsCheck { len, index } => {
                        ("panic_bounds_check", vec![index, len])
                    }
                    AssertMessage::DivisionByZero(_) => ("panic_const_div_by_zero", vec![]),
                    AssertMessage::RemainderByZero(_) => ("panic_const_rem_by_zero", vec![]),
                    AssertMessage::DivisionOverflow(..) => ("panic_const_div_overflow", vec![]),
                    AssertMessage::RemainderOverflow(..) => ("panic_const_rem_overflow", vec![]),
                };
                let lang_fn = self
                    .ctx
                    .hir
                    .items
                    .lang_fns
                    .get(&flux_intern::Symbol::intern(lang))
                    .copied();
                let panic = match lang_fn {
                    Some(_) => self.b.create_block(),
                    None => self.panic_block(),
                };
                if *expected {
                    self.b.ins().brif(cond, target, &[], panic, &[]);
                } else {
                    self.b.ins().brif(cond, panic, &[], target, &[]);
                }
                if let Some(function) = lang_fn {
                    self.b.switch_to_block(panic);
                    let mut values = Vec::new();
                    for operand in operands {
                        values.extend(self.operand(operand));
                    }
                    values.push(self.location_here(offset));
                    let callee = self.callee(function, Ty::list(&[]));
                    self.emit_call(
                        Call::Direct {
                            callee,
                            location: None,
                        },
                        &values,
                    );
                    self.b.ins().trap();
                }
            }
            TerminatorKind::Return => {
                let values = self.read(&Body::RETURN.into());
                let ret = self.body.locals[Body::RETURN].ty;
                match self.sret.clone() {
                    Some((pointer, _)) if self.convention.is_c() => {
                        self.store(pointer, ret, &values);
                        self.b.ins().return_(&[]);
                    }
                    Some((pointer, _)) if is_memory(self.sigs, ret) => {
                        self.copy(pointer, values[0], ret);
                        self.b.ins().return_(&[]);
                    }
                    Some((pointer, layout)) => {
                        for ((offset, ty), value) in layout.leaves().into_iter().zip(values) {
                            let address = self.offset(pointer, offset);
                            self.b.ins().store_with_flags(
                                value,
                                address,
                                address_flags(&self.address_align, address, ty),
                            );
                        }
                        self.b.ins().return_(&[]);
                    }
                    // A value with no size returns nothing.
                    None if is_memory(self.sigs, ret) => {
                        self.b.ins().return_(&[]);
                    }
                    None if self.convention.is_c()
                        && abi::c_scalar(self.sigs, &self.ctx.layouts, ret).is_some() =>
                    {
                        let scalar = abi::c_scalar(self.sigs, &self.ctx.layouts, ret).unwrap();
                        let value = self.encode_c_scalar(&values, ret, scalar);
                        self.b.ins().return_(&[value]);
                    }
                    None => {
                        self.b.ins().return_(&values);
                    }
                }
            }
            TerminatorKind::UnwindResume => self.resume_unwind(),
            TerminatorKind::UnwindTerminate => self.abort_unwind(),
            TerminatorKind::Unreachable => {
                self.b.ins().trap();
            }
            TerminatorKind::Drop { place, .. } => panic!(
                "unexpanded drop of {} in {:?} at bb{}",
                self.body.place_ty(place, self.sigs),
                self.body
                    .owner
                    .map(|owner| self.ctx.hir.items.functions[owner].name),
                block.index()
            ),
        }
    }

    /// A call returning a `ret`, with arguments `args`, its result
    /// stored in `dest`, continuing at `target`: `make` makes the call
    /// instruction from the argument values, an `sret` pointer first.
    fn call_abi(
        &mut self,
        convention: Convention,
        ret: Ty,
        args: &[flux_mir::Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
        make: Call,
    ) {
        let direct = dest.projection.is_empty()
            && self.direct_sret[dest.local.index()]
            && !args.iter().any(
                |arg| matches!(arg, Operand::Copy(p) | Operand::Move(p) if p.local == dest.local),
            );
        let sret = self.sret_for(ret, direct.then_some(dest), convention);
        let mut values: Values = sret.iter().map(|(pointer, _)| *pointer).collect();
        for arg in args {
            let ty = self.body.operand_ty(arg, self.sigs);
            values.extend(self.argument(arg, ty, convention));
        }
        self.finish_call(
            CallResult {
                ty: ret,
                sret,
                convention,
            },
            values,
            dest,
            target,
            make,
        );
    }

    /// Marshal a Rust argument using a private temporary for an indirectly
    /// passed scalar aggregate, so its bytes never alias another argument.
    fn rust_argument(&mut self, arg: &Operand) -> Values {
        let ty = self.body.operand_ty(arg, self.sigs);
        self.argument(arg, ty, Convention::RustReified)
    }

    fn encode_c_scalar(&mut self, values: &[Value], ty: Ty, scalar: Ty) -> Value {
        let pointer = self.new_slot(ty);
        self.store(pointer, ty, values);
        self.load(Location::Memory(pointer), scalar)[0]
    }

    fn decode_c_scalar(&mut self, value: Value, scalar: Ty, ty: Ty) -> Values {
        let pointer = self.new_slot(ty);
        self.store(pointer, scalar, &[value]);
        self.load(Location::Memory(pointer), ty)
    }

    fn argument(&mut self, arg: &Operand, ty: Ty, convention: Convention) -> Values {
        let mode = convention.argument(self.sigs, &self.ctx.layouts, ty);
        if matches!(mode, Argument::Ignore) {
            return Values::new();
        }
        let values = self.operand(arg);
        match mode {
            Argument::Scalar(scalar) => smallvec![self.encode_c_scalar(&values, ty, scalar)],
            Argument::Indirect => {
                let pointer = self.new_slot(ty);
                self.store(pointer, ty, &values);
                smallvec![pointer]
            }
            Argument::ByVal(_) if !is_memory(self.sigs, ty) => {
                let pointer = self.new_slot(ty);
                self.store(pointer, ty, &values);
                smallvec![pointer]
            }
            _ => values,
        }
    }

    /// Memory for a call's result of type `ret`, if it is returned
    /// through memory.
    fn sret_for(
        &mut self,
        ret: Ty,
        dest: Option<&Place>,
        convention: Convention,
    ) -> Option<(Value, Layout)> {
        let kind = convention.result(self.sigs, &self.ctx.layouts, ret);
        match kind {
            Return::Sret(layout) | Return::Memory(layout) => {
                let align = layout.align().max(self.ctx.layouts.of(ret).align);
                let address = if is_memory(self.sigs, ret)
                    && let Some(dest) = dest
                {
                    let (Location::Memory(address), _) = self.locate(dest) else {
                        unreachable!("memory return destination");
                    };
                    address
                } else {
                    self.aligned_slot(layout.size(), align)
                };
                Some((address, layout))
            }
            Return::Values(_) => None,
        }
    }

    /// [`Self::call`] with the arguments' leaves given: `first`, then
    /// `rest`.
    fn call_values(
        &mut self,
        ret: Ty,
        first: Value,
        rest: Values,
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
        make: Call,
    ) {
        let sret = self.sret_for(ret, None, Convention::RustReified);
        let mut values: Values = sret.iter().map(|(pointer, _)| *pointer).collect();
        values.push(first);
        values.extend(rest);
        self.finish_call(
            CallResult {
                ty: ret,
                sret,
                convention: Convention::RustReified,
            },
            values,
            dest,
            target,
            make,
        );
    }

    /// The call instruction `make` makes from `values`, its result
    /// stored in `dest` (through `sret` if given), continuing at
    /// `target`.
    fn finish_call(
        &mut self,
        result: CallResult,
        values: Values,
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
        make: Call,
    ) {
        let CallResult {
            ty: ret,
            sret,
            convention,
        } = result;
        let memory = is_memory(self.sigs, ret);
        let call = self.emit_call(make, &values);
        // A call that does not return stores nothing: its destination may
        // be of any type (a `match` arm calling `panic`).
        if target.is_some() {
            let results = match &sret {
                Some((pointer, _)) if convention.is_c() => {
                    self.load(Location::Memory(*pointer), ret)
                }
                Some((pointer, _)) if memory => smallvec![*pointer],
                Some((pointer, layout)) => self.load_leaves(*pointer, layout),
                None if convention.is_c()
                    && abi::c_scalar(self.sigs, &self.ctx.layouts, ret).is_some() =>
                {
                    let scalar = abi::c_scalar(self.sigs, &self.ctx.layouts, ret).unwrap();
                    let value = self.b.function().inst_results(call)[0];
                    self.decode_c_scalar(value, scalar, ret)
                }
                None => self
                    .b
                    .function()
                    .inst_results(call)
                    .iter()
                    .copied()
                    .collect(),
            };
            self.write(dest, &results);
        }
        match target {
            Some(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            None => {
                self.b.ins().trap();
            }
        }
    }

    /// The leaves of a value laid out as `layout` at `pointer`.
    fn load_leaves(&mut self, pointer: Value, layout: &Layout) -> Values {
        layout
            .leaves()
            .into_iter()
            .map(|(offset, ty)| {
                let address = self.offset(pointer, offset);
                self.b.ins().load_with_flags(
                    ty,
                    address,
                    address_flags(&self.address_align, address, ty),
                )
            })
            .collect()
    }

    fn offset(&mut self, pointer: Value, offset: u64) -> Value {
        if offset == 0 {
            return pointer;
        }
        let displacement = self.b.ins().iconst(Type::I64, offset as i64);
        let result = self.b.ins().ptradd(pointer, displacement);
        if let Some(&align) = self.address_align.get(&pointer) {
            self.address_align
                .insert(result, align.min(1u64 << offset.trailing_zeros()));
        }
        result
    }

    /// Where code at `offset` in the body's file is, for a
    /// `#[track_caller]` callee: where the function was called from if
    /// it takes that, as rustc passes it on.
    fn location_here(&mut self, offset: u32) -> Value {
        if let Some(location) = self.caller_location {
            return location;
        }
        let name = self.ctx.location(self.file, offset);
        let data = self.b.import_data(&name);
        self.b.ins().symbol_addr(keel_ir::Symbol::Data(data))
    }

    fn pointer_callee(&mut self, function: FunctionId, args: GenericArgs) -> FuncRef {
        let instance = self.ctx.resolve_instance(Instance { function, args });
        if self.ctx.compilation.is_some()
            && self.ctx.hir.items.functions[instance.function].track_caller
        {
            let sig = self.ctx.normalized_signature(instance);
            let sig = ctx_signature(self.sigs, &self.ctx.layouts, &sig, true);
            self.b
                .import_function(&self.ctx.pointer_symbol(instance), sig)
        } else {
            self.callee(function, args)
        }
    }

    fn callee(&mut self, func: FunctionId, generic_args: GenericArgs) -> FuncRef {
        let instance = Instance {
            function: func,
            args: generic_args,
        };
        let instance = self.ctx.resolve_instance(instance);
        if let Some(&callee) = self.callees.get(&instance) {
            return callee;
        }
        let sig = self.ctx.normalized_signature(instance);
        let function = &self.ctx.hir.items.functions[instance.function];
        let mut sig = if function.abi.is_c() {
            c_signature(self.sigs, &self.ctx.layouts, &sig, function.is_variadic)
        } else {
            let reified = self.ctx.compilation.is_some() || self.ctx.reified.contains(&instance);
            ctx_signature(self.sigs, &self.ctx.layouts, &sig, reified)
        };
        if self.ctx.takes_location(instance) {
            sig.params.push(AbiParam::new(Type::Ptr));
        }
        let symbol = instance_symbol(self.ctx.hir, self.ctx.symbols, instance);
        let callee = self.b.import_function(&symbol, sig);
        self.callees.insert(instance, callee);
        callee
    }

    /// A call of trait method `func` on a trait object: the method is
    /// the vtable's, called through a pointer with C's convention. The
    /// receiver (`&self`, `Box<Self>`, `Rc<Self>`, `Pin<&mut Self>`...)
    /// ends with the object's vtable; its other leaves are what the
    /// method's receiver for the type behind it has.
    fn virtual_call(
        &mut self,
        func: FunctionId,
        generic_args: GenericArgs,
        args: &[flux_mir::Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        let items = &self.ctx.hir.items;
        let sigs = &self.ctx.typed.sigs;
        let object = generic_args[0];
        let index = flux_mir::vtable::method_index(items, sigs, object, func)
            .expect("a trait object's method has a slot");
        let sig = flux_solve::traits::instance_sig(items, sigs, func, generic_args);
        let receiver = self.operand(&args[0]);
        let (&vtable, thin_receiver) = receiver.split_last().expect("an object receiver");
        // The receiver's leaves but the vtable, as addresses.
        let mut params = sig.params.to_vec();
        params[0] = Ty::tuple(&vec![address_leaf(); thin_receiver.len()]);
        let thin = FnSig {
            params: params.into(),
            ret: sig.ret,
        };
        let sig_ref =
            self.b
                .import_signature(signature(self.sigs, &self.ctx.layouts, &thin, false));
        let address = self.offset(vtable, 8 * index as u64);
        let pointer = self.b.ins().load_with_flags(
            Type::Ptr,
            address,
            address_flags(&self.address_align, address, Type::Ptr),
        );
        let mut leaves: Values = thin_receiver.iter().copied().collect();
        for arg in &args[1..] {
            leaves.extend(self.rust_argument(arg));
        }
        let (&first, rest) = leaves.split_first().expect("a receiver address");
        let rest: Values = rest.iter().copied().collect();
        self.call_values(
            sig.ret,
            first,
            rest,
            dest,
            target,
            Call::Indirect {
                sig: sig_ref,
                pointer,
            },
        );
    }

    /// Packed values exist only inside the operation; Rust calls retain the
    /// ordinary memory aggregate convention used by the embedded vector types.
    fn x86_intrinsic(
        &mut self,
        func: FunctionId,
        generic_args: &[Ty],
        args: &[Operand],
        dest: &Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        use keel_ir::{BinaryOp, Intrinsic, VectorBinaryOp, VectorType};
        let name = self.ctx.hir.items.functions[func].name;
        let first = self.operand(&args[0])[0];
        if name.as_str() == "__x86_mm_prefetch" {
            let TyKind::Value(strategy) = generic_args[0].kind() else {
                unreachable!("validated prefetch strategy")
            };
            let id = match strategy.bits() {
                0 => Intrinsic::X86Prefetchnta,
                1 => Intrinsic::X86Prefetcht2,
                2 => Intrinsic::X86Prefetcht1,
                3 => Intrinsic::X86Prefetcht0,
                _ => unreachable!("validated prefetch strategy"),
            };
            self.b.ins().intrinsic(id, &[first]);
            match target {
                Some(target) => {
                    self.b.ins().jump(self.blocks[target.index()], &[]);
                }
                None => {
                    self.b.ins().trap();
                }
            }
            return;
        }
        let value = match name.as_str() {
            name if !matches!(
                name,
                "__x86_mm_shuffle_epi32"
                    | "__x86_mm_alignr_epi8"
                    | "__x86_mm256_alignr_epi8"
                    | "__x86_mm_blend_epi16"
                    | "__x86_mm_extract_epi32"
            ) && name.starts_with("__x86_mm") =>
            {
                enum Operation {
                    Load,
                    Binary(VectorBinaryOp),
                    Compare(IntCC),
                    Native(Intrinsic),
                    Native3(Intrinsic),
                    Immediate(Intrinsic),
                    Shift(Intrinsic),
                    Mask(Intrinsic),
                    Shift32(Intrinsic),
                }
                use Operation::*;
                let (shape, operation) = match name {
                    "__x86_mm512_xor_si512" => (VectorType::I64x8, Binary(VectorBinaryOp::Xor)),
                    "__x86_mm512_loadu_si512" => (VectorType::I64x8, Load),
                    "__x86_mm_clmulepi64_si128" => {
                        (VectorType::I64x2, Immediate(Intrinsic::X86Pclmulqdq128))
                    }
                    "__x86_mm256_clmulepi64_epi128" => {
                        (VectorType::I64x4, Immediate(Intrinsic::X86Vpclmulqdq256))
                    }
                    "__x86_mm512_clmulepi64_epi128" => {
                        (VectorType::I64x8, Immediate(Intrinsic::X86Vpclmulqdq512))
                    }
                    "__x86_mm_sha256msg1_epu32" => {
                        (VectorType::I32x4, Native(Intrinsic::X86Sha256msg1))
                    }
                    "__x86_mm_sha256msg2_epu32" => {
                        (VectorType::I32x4, Native(Intrinsic::X86Sha256msg2))
                    }
                    "__x86_mm_sha256rnds2_epu32" => {
                        (VectorType::I32x4, Native3(Intrinsic::X86Sha256rnds2))
                    }
                    "__x86_mm_and_si128" => (VectorType::I8x16, Binary(VectorBinaryOp::And)),
                    "__x86_mm_or_si128" => (VectorType::I8x16, Binary(VectorBinaryOp::Or)),
                    "__x86_mm_xor_si128" => (VectorType::I8x16, Binary(VectorBinaryOp::Xor)),
                    "__x86_mm_add_epi32" => (VectorType::I32x4, Binary(VectorBinaryOp::Iadd)),
                    "__x86_mm_add_epi64" => (VectorType::I64x2, Binary(VectorBinaryOp::Iadd)),
                    "__x86_mm_sub_epi16" => (VectorType::I16x8, Binary(VectorBinaryOp::Isub)),
                    "__x86_mm_mullo_epi16" => (VectorType::I16x8, Binary(VectorBinaryOp::Imul)),
                    "__x86_mm_cmpeq_epi8" => (VectorType::I8x16, Compare(IntCC::Eq)),
                    "__x86_mm_cmpgt_epi8" => (VectorType::I8x16, Compare(IntCC::Sgt)),
                    "__x86_mm_loadu_si128" => (VectorType::I8x16, Load),
                    "__x86_mm_load_si128" => (VectorType::I8x16, Load),
                    "__x86_mm_movemask_epi8" => {
                        (VectorType::I8x16, Mask(Intrinsic::X86SsePmovmskb128))
                    }
                    "__x86_mm_shuffle_epi8" => {
                        (VectorType::I8x16, Native(Intrinsic::X86Ssse3Pshufb128))
                    }
                    "__x86_mm_slli_epi16" => (VectorType::I16x8, Shift(Intrinsic::X86Sse2Psllw128)),
                    "__x86_mm_srli_epi16" => (VectorType::I16x8, Shift(Intrinsic::X86Sse2Psrlw128)),
                    "__x86_mm_slli_epi32" => (VectorType::I32x4, Shift(Intrinsic::X86Sse2Pslld128)),
                    "__x86_mm_srli_epi32" => (VectorType::I32x4, Shift(Intrinsic::X86Sse2Psrld128)),
                    "__x86_mm_slli_epi64" => (VectorType::I64x2, Shift(Intrinsic::X86Sse2Psllq128)),
                    "__x86_mm_srli_epi64" => (VectorType::I64x2, Shift(Intrinsic::X86Sse2Psrlq128)),
                    "__x86_mm256_and_si256" => (VectorType::I8x32, Binary(VectorBinaryOp::And)),
                    "__x86_mm256_or_si256" => (VectorType::I8x32, Binary(VectorBinaryOp::Or)),
                    "__x86_mm256_xor_si256" => (VectorType::I8x32, Binary(VectorBinaryOp::Xor)),
                    "__x86_mm256_add_epi32" => (VectorType::I32x8, Binary(VectorBinaryOp::Iadd)),
                    "__x86_mm256_add_epi64" => (VectorType::I64x4, Binary(VectorBinaryOp::Iadd)),
                    "__x86_mm256_sub_epi16" => (VectorType::I16x16, Binary(VectorBinaryOp::Isub)),
                    "__x86_mm256_mullo_epi16" => (VectorType::I16x16, Binary(VectorBinaryOp::Imul)),
                    "__x86_mm256_cmpeq_epi8" => (VectorType::I8x32, Compare(IntCC::Eq)),
                    "__x86_mm256_cmpgt_epi8" => (VectorType::I8x32, Compare(IntCC::Sgt)),
                    "__x86_mm256_loadu_si256" => (VectorType::I8x32, Load),
                    "__x86_mm256_load_si256" => (VectorType::I8x32, Load),
                    "__x86_mm256_movemask_epi8" => {
                        (VectorType::I8x32, Mask(Intrinsic::X86Avx2Vpmovmskb256))
                    }
                    "__x86_mm256_shuffle_epi8" => {
                        (VectorType::I8x32, Native(Intrinsic::X86Avx2Vpshufb256))
                    }
                    "__x86_mm256_slli_epi16" => {
                        (VectorType::I16x16, Shift(Intrinsic::X86Avx2Vpsllw256))
                    }
                    "__x86_mm256_srli_epi16" => {
                        (VectorType::I16x16, Shift(Intrinsic::X86Avx2Vpsrlw256))
                    }
                    "__x86_mm256_slli_epi32" => {
                        (VectorType::I32x8, Shift(Intrinsic::X86Avx2Vpslld256))
                    }
                    "__x86_mm256_srli_epi32" => {
                        (VectorType::I32x8, Shift(Intrinsic::X86Avx2Vpsrld256))
                    }
                    "__x86_mm256_slli_epi64" => {
                        (VectorType::I64x4, Shift(Intrinsic::X86Avx2Vpsllq256))
                    }
                    "__x86_mm256_srli_epi64" => {
                        (VectorType::I64x4, Shift(Intrinsic::X86Avx2Vpsrlq256))
                    }
                    "__x86_mm_mulhi_epu16" => {
                        (VectorType::I16x8, Native(Intrinsic::X86Sse2Pmulhuw128))
                    }
                    "__x86_mm_mul_epu32" => {
                        (VectorType::I32x4, Native(Intrinsic::X86Sse2Pmuludq128))
                    }
                    "__x86_mm_add_epi8" => (VectorType::I8x16, Binary(VectorBinaryOp::Iadd)),
                    "__x86_mm_cmpeq_epi16" => (VectorType::I16x8, Compare(IntCC::Eq)),
                    "__x86_mm_cmpeq_epi32" => (VectorType::I32x4, Compare(IntCC::Eq)),
                    "__x86_mm_cmpgt_epi16" => (VectorType::I16x8, Compare(IntCC::Sgt)),
                    "__x86_mm_cmpgt_epi32" => (VectorType::I32x4, Compare(IntCC::Sgt)),
                    "__x86_mm_packs_epi16" => {
                        (VectorType::I16x8, Native(Intrinsic::X86Sse2Packsswb128))
                    }
                    "__x86_mm_packs_epi32" => {
                        (VectorType::I32x4, Native(Intrinsic::X86Sse2Packssdw128))
                    }
                    "__x86_mm_srli_si128" => {
                        (VectorType::I8x16, Shift32(Intrinsic::X86Sse2Psrldq128))
                    }
                    _ => unreachable!("registered vector operation"),
                };
                let ty = Type::Vector(shape);
                let a = self.b.ins().load(ty, first);
                match operation {
                    Load => a,
                    Mask(id) => {
                        let inst = self.b.ins().intrinsic(id, &[a]);
                        self.b.function().inst_results(inst)[0]
                    }
                    Shift(id) | Shift32(id) => {
                        let TyKind::Value(count) = generic_args[0].kind() else {
                            unreachable!("validated immediate")
                        };
                        let ty = if matches!(operation, Shift32(_)) {
                            Type::I32
                        } else {
                            Type::I64
                        };
                        let count = self.b.ins().iconst(ty, count.bits() as i64);
                        let inst = self.b.ins().intrinsic(id, &[a, count]);
                        self.b.function().inst_results(inst)[0]
                    }
                    Binary(_) | Compare(_) | Native(_) | Native3(_) | Immediate(_) => {
                        let second = self.operand(&args[1])[0];
                        let b = self.b.ins().load(ty, second);
                        match operation {
                            Binary(op) => self.b.ins().vector_binary(op, a, b),
                            Compare(cc) => self.b.ins().vicmp(cc, a, b),
                            Native(id) => {
                                let inst = self.b.ins().intrinsic(id, &[a, b]);
                                self.b.function().inst_results(inst)[0]
                            }
                            Native3(id) => {
                                let third = self.operand(&args[2])[0];
                                let c = self.b.ins().load(ty, third);
                                let inst = self.b.ins().intrinsic(id, &[a, b, c]);
                                self.b.function().inst_results(inst)[0]
                            }
                            Immediate(id) => {
                                let TyKind::Value(immediate) = generic_args[0].kind() else {
                                    unreachable!("validated immediate")
                                };
                                let immediate =
                                    self.b.ins().iconst(Type::I32, immediate.bits() as i64);
                                let inst = self.b.ins().intrinsic(id, &[a, b, immediate]);
                                self.b.function().inst_results(inst)[0]
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
            "__x86_mm_shuffle_epi32" => {
                let ty = Type::Vector(VectorType::I8x16);
                let a = self.b.ins().load(ty, first);
                let TyKind::Value(immediate) = generic_args[0].kind() else {
                    unreachable!("validated immediate")
                };
                let immediate = immediate.bits() as usize;
                let lanes = (0..16)
                    .map(|i| (((immediate >> (2 * (i / 4))) & 3) * 4 + i % 4) as u8)
                    .collect();
                self.b.ins().vshuffle(a, a, lanes)
            }
            "__x86_mm_alignr_epi8" => {
                let ty = Type::Vector(VectorType::I8x16);
                let a = self.b.ins().load(ty, first);
                let TyKind::Value(immediate) = generic_args[0].kind() else {
                    unreachable!("validated immediate")
                };
                let immediate = immediate.bits() as usize;
                let second = self.operand(&args[1])[0];
                let b = self.b.ins().load(ty, second);
                let zero = self.b.ins().iconst(Type::I8, 0);
                let mut result = self.b.ins().vsplat(ty, zero);
                for i in 0..16 {
                    let source = i % 16 + immediate;
                    if source < 32 {
                        let lane = self.b.ins().vextract(
                            if source < 16 { b } else { a },
                            (i / 16 * 16 + source % 16) as u8,
                        );
                        result = self.b.ins().vinsert(result, lane, i as u8);
                    }
                }
                result
            }
            "__x86_mm256_alignr_epi8" => {
                let ty = Type::Vector(VectorType::I8x32);
                let a = self.b.ins().load(ty, first);
                let TyKind::Value(immediate) = generic_args[0].kind() else {
                    unreachable!("validated immediate")
                };
                let immediate = immediate.bits() as usize;
                let second = self.operand(&args[1])[0];
                let b = self.b.ins().load(ty, second);
                let zero = self.b.ins().iconst(Type::I8, 0);
                let mut result = self.b.ins().vsplat(ty, zero);
                for i in 0..32 {
                    let source = i % 16 + immediate;
                    if source < 32 {
                        let lane = self.b.ins().vextract(
                            if source < 16 { b } else { a },
                            (i / 16 * 16 + source % 16) as u8,
                        );
                        result = self.b.ins().vinsert(result, lane, i as u8);
                    }
                }
                result
            }
            "__x86_mm_blend_epi16" => {
                let ty = Type::Vector(VectorType::I8x16);
                let a = self.b.ins().load(ty, first);
                let TyKind::Value(immediate) = generic_args[0].kind() else {
                    unreachable!("validated immediate")
                };
                let immediate = immediate.bits() as usize;
                let second = self.operand(&args[1])[0];
                let b = self.b.ins().load(ty, second);
                let lanes = (0..16)
                    .map(|i| {
                        (i + if immediate & (1 << (i / 2)) != 0 {
                            16
                        } else {
                            0
                        }) as u8
                    })
                    .collect();
                self.b.ins().vshuffle(a, b, lanes)
            }
            "__x86_mm_extract_epi32" => {
                let TyKind::Value(lane) = generic_args[0].kind() else {
                    unreachable!("validated lane")
                };
                let vector = self.b.ins().load(Type::Vector(VectorType::I32x4), first);
                self.b.ins().vextract(vector, lane.bits() as u8)
            }
            "__x86_cpuid" => {
                let leaf = first;
                let subleaf = self.operand(&args[1])[0];
                let register = self.operand(&args[2])[0];
                let inst = self
                    .b
                    .ins()
                    .intrinsic(Intrinsic::X86Cpuid, &[leaf, subleaf]);
                let result = self.b.function().inst_results(inst)[0];
                let register = self.b.ins().convert(ConvertOp::Uext, Type::I128, register);
                let bits = self.b.ins().iconst(Type::I128, 32);
                let shift = self.b.ins().binary(BinaryOp::Imul, register, bits);
                let result = self.b.ins().binary(BinaryOp::Ushr, result, shift);
                self.b.ins().convert(ConvertOp::Trunc, Type::I32, result)
            }
            "__x86_xgetbv" => {
                let inst = self.b.ins().intrinsic(Intrinsic::X86CpuXcr0, &[]);
                self.b.function().inst_results(inst)[0]
            }
            _ => unreachable!("registered x86 intrinsic"),
        };
        if name.as_str().contains("movemask")
            || matches!(
                name.as_str(),
                "__x86_cpuid" | "__x86_xgetbv" | "__x86_mm_extract_epi32"
            )
        {
            self.write(dest, &[value]);
        } else {
            let (Location::Memory(address), _) = self.locate(dest) else {
                unreachable!("x86 vector aggregates live in memory")
            };
            self.b.ins().store(value, address);
        }
        match target {
            Some(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            None => {
                self.b.ins().trap();
            }
        }
    }

    /// NEON values use the ordinary indirect array aggregate convention at
    /// Rust calls; only the operation itself introduces packed Keel values.
    fn neon_intrinsic(
        &mut self,
        func: FunctionId,
        generic_args: &[Ty],
        args: &[Operand],
        dest: &Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        use keel_ir::{NeonIntrinsic, VectorBinaryOp, VectorType};
        let symbol = self.ctx.hir.items.functions[func].name;
        let name = symbol
            .as_str()
            .strip_prefix("__neon_")
            .expect("NEON intrinsic");
        let first = self.operand(&args[0])[0];
        let bytes = Type::Vector(
            if matches!(
                name,
                "vld1_u8"
                    | "vst1_u8"
                    | "vdup_n_u8"
                    | "vceq_u8"
                    | "vorr_u8"
                    | "vcltz_s8"
                    | "vcgez_s8"
            ) {
                VectorType::I8x8
            } else {
                VectorType::I8x16
            },
        );
        let value = match name {
            "vshrn_n_u16" => {
                let TyKind::Value(shift) = generic_args[0].kind() else {
                    unreachable!("validated constant shift")
                };
                let input = self.b.ins().load(Type::Vector(VectorType::I16x8), first);
                let shift = self.b.ins().iconst(Type::I32, shift.bits() as i64);
                // A D-vector result keeps the store at exactly eight bytes.
                self.b.ins().neon(
                    NeonIntrinsic::Shrn,
                    Type::Vector(VectorType::I8x8),
                    &[input, shift],
                )
            }
            "vpmaxq_u8" => {
                let second = self.operand(&args[1])[0];
                let a = self.b.ins().load(bytes, first);
                let b = self.b.ins().load(bytes, second);
                self.b.ins().vector_binary(VectorBinaryOp::Pumax, a, b)
            }
            "vdup_n_u8" | "vdupq_n_u8" => self.b.ins().vsplat(bytes, first),
            "vcltz_s8" | "vcgez_s8" => {
                let a = self.b.ins().load(bytes, first);
                let zero = self.b.ins().iconst(Type::I8, 0);
                let zero = self.b.ins().vsplat(bytes, zero);
                self.b.ins().vicmp(
                    if name == "vcltz_s8" {
                        IntCC::Slt
                    } else {
                        IntCC::Sge
                    },
                    a,
                    zero,
                )
            }
            "vst1_u8" => {
                let second = self.operand(&args[1])[0];
                let vector = self.b.ins().load(bytes, second);
                self.b.ins().store(vector, first);
                vector
            }
            "vget_lane_u64" => self.b.ins().load(Type::I64, first),
            "vgetq_lane_u64" => {
                let TyKind::Value(lane) = generic_args[0].kind() else {
                    unreachable!("validated constant lane")
                };
                let vector = self.b.ins().load(Type::Vector(VectorType::I64x2), first);
                self.b.ins().vextract(vector, lane.bits() as u8)
            }
            _ => {
                // The load intrinsic explicitly promises no alignment. Using
                // the same conservative flags for aggregate inputs is valid.
                let a = self.b.ins().load(bytes, first);
                if matches!(name, "vld1q_u8" | "vld1_u8") {
                    a
                } else {
                    let second = self.operand(&args[1])[0];
                    let b = self.b.ins().load(bytes, second);
                    match name {
                        "vceq_u8" | "vceqq_u8" => self.b.ins().vicmp(IntCC::Eq, a, b),
                        "vandq_u8" => self.b.ins().vector_binary(VectorBinaryOp::And, a, b),
                        "vorr_u8" | "vorrq_u8" => {
                            self.b.ins().vector_binary(VectorBinaryOp::Or, a, b)
                        }
                        _ => unreachable!("bounded NEON intrinsic"),
                    }
                }
            }
        };
        if matches!(name, "vgetq_lane_u64" | "vget_lane_u64") {
            self.write(dest, &[value]);
        } else if name != "vst1_u8" {
            let (Location::Memory(address), _) = self.locate(dest) else {
                unreachable!("NEON aggregates live in memory")
            };
            self.b.ins().store(value, address);
        }
        match target {
            Some(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            None => {
                self.b.ins().trap();
            }
        }
    }

    /// `size_of_val`, `align_of_val` or `drop_in_place` of a trait
    /// object, which `mono` leaves to code generation: the vtable's
    /// size or alignment word, or a call of its drop function, if it has
    /// one, with the data pointer.
    fn dyn_intrinsic(
        &mut self,
        func: FunctionId,
        args: &[Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        let name = self.ctx.hir.items.functions[func].name;
        let leaves = self.operand(&args[0]);
        let (data, vtable) = (leaves[0], leaves[1]);
        let word = |this: &mut Self, index: u64, ty: Type| {
            let at = this.offset(vtable, 8 * index);
            this.b.ins().load_with_flags(ty, at, aligned(ty))
        };
        match name.as_str() {
            "size_of_val" => {
                let size = word(self, 1, Type::I64);
                self.write(dest, &[size]);
            }
            "align_of_val" => {
                let align = word(self, 2, Type::I64);
                self.write(dest, &[align]);
            }
            _ => {
                // A null slot: nothing to drop.
                let drop = word(self, 0, Type::Ptr);
                let address = self
                    .b
                    .ins()
                    .convert(keel_ir::ConvertOp::PtrToInt, Type::I64, drop);
                let zero = self.b.ins().iconst(Type::I64, 0);
                let null = self.b.ins().icmp(IntCC::Eq, address, zero);
                let call = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().brif(null, done, &[], call, &[]);
                self.b.switch_to_block(call);
                let sig = Signature::new(&[Type::Ptr], &[]);
                let sig_ref = self.b.import_signature(sig);
                self.emit_call(
                    Call::Indirect {
                        sig: sig_ref,
                        pointer: drop,
                    },
                    &[data],
                );
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
            }
        }
        match target {
            Some(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            None => {
                self.b.ins().trap();
            }
        }
    }

    fn panic_block(&mut self) -> Block {
        if let Some(block) = self.panic_block {
            return block;
        }
        self.ctx.uses_abort = true;
        let block = self.b.create_block();
        self.panic_block = Some(block);
        block
    }
}

/// Flags for an access to a `ty` in memory laid out by a [`Layout`],
/// which places each scalar at its natural alignment, its size.
fn address_flags(alignments: &FxHashMap<Value, u64>, address: Value, ty: Type) -> MemoryFlags {
    let mut flags = aligned(ty);
    if let Some(&align) = alignments.get(&address) {
        flags.align = flags.align.min(align as u32);
    }
    flags
}

fn aligned(ty: Type) -> MemoryFlags {
    MemoryFlags {
        align: Layout::Scalar(ty).align() as u32,
        ..MemoryFlags::default()
    }
}

fn static_slice_metadata(sigs: &Signatures, pointer: Ty, stored: Ty) -> Option<u64> {
    let target = match pointer.kind() {
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t,
        _ => return None,
    };
    if sigs.metadata(target) == flux_ty::Metadata::Length
        && let TyKind::Array(_, len) = sigs.structural_tail(stored).kind()
    {
        return len.as_usize();
    }
    None
}

#[cfg(test)]
mod partition_identity_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn graph(source: &str) -> Hir {
        let mut db = flux_hir::SourceDb::new();
        let root = db.add("/partition-identity/lib.rs", source);
        flux_hir::lower_sources(
            &mut db,
            &[flux_hir::SourceCrate {
                root,
                name: flux_intern::Symbol::intern("partition_test"),
                dependencies: Vec::new(),
            }],
        )
    }

    fn lower(source: &str, revision: u8, identity: SymbolIdentity) -> Module {
        lower_with_builtin_object(source, revision, identity, false)
    }

    fn lower_with_builtin_object(
        source: &str,
        revision: u8,
        identity: SymbolIdentity,
        builtin_object: bool,
    ) -> Module {
        let hir = graph(source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let own = hir.items.main_crate();
        let hashes: Vec<_> = hir
            .items
            .crates
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let id = flux_hir::CrateId(index as u32);
                (id, [if id == own { revision } else { 7 }; 32])
            })
            .collect();
        typed.sigs.type_ids.set_crate_hashes(&hashes);
        let (id, item) = hir
            .items
            .structs
            .iter()
            .find(|(_, item)| item.name.as_str() == "Own")
            .unwrap();
        let own_type = Ty::adt(id, item.name, &[]);
        let runtime_id = typed.sigs.type_ids.of(&hir.items, own_type).unwrap();
        let compilation = Compilation {
            kind: OutputKind::Executable,
            crate_hashes: &hashes,
            imported: &[],
            builtin_object,
            required_functions: &[],
            required_instances: &[],
        };
        let module = match identity {
            SymbolIdentity::Artifact => {
                lower_crate_with_requirements(&hir, &typed, &mir, compilation)
                    .unwrap()
                    .module
            }
            SymbolIdentity::Query => {
                lower_partitioned_crate(&hir, &typed, &mir, compilation)
                    .unwrap()
                    .module
            }
        };
        assert_eq!(
            runtime_id,
            typed.sigs.type_ids.of(&hir.items, own_type).unwrap(),
            "backend naming must not change runtime type IDs"
        );
        keel_ir::verify_module(&module).unwrap();
        module
    }

    const SOURCE: &str = "struct Own(u8); static NUMBER: u8 = 7; fn changed() -> u8 { 1 } fn untouched() -> u8 { 3 } fn identity<T>(x: T) -> T { x } fn main() { let value = identity(Own(NUMBER)); println!(\"{} {} {}\", changed(), untouched(), value.0); }";

    #[test]
    fn partitioned_symbols_survive_unrelated_edits_and_crate_revision_changes() {
        let before = lower(SOURCE, 1, SymbolIdentity::Query);
        let after = lower(
            &SOURCE.replace("-> u8 { 1 }", "-> u8 { 2 }"),
            2,
            SymbolIdentity::Query,
        );
        let names = |module: &Module| {
            module
                .functions
                .iter()
                .filter(|f| f.name.contains("__flux_query_definition_"))
                .map(|f| f.name.clone())
                .collect::<BTreeSet<_>>()
        };
        assert!(!names(&before).is_empty());
        assert_eq!(
            names(&before),
            names(&after),
            "body edits must not rename source-owned instances"
        );
        let all_names = |module: &Module| {
            module
                .functions
                .iter()
                .map(|f| f.name.clone())
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            all_names(&before),
            all_names(&after),
            "whole-graph generic wrappers must use edit-stable source type arguments too"
        );
        let data = |module: &Module| {
            module
                .data
                .iter()
                .filter(|d| d.name.contains("__flux_query_definition_"))
                .map(|d| d.name.clone())
                .collect::<BTreeSet<_>>()
        };
        assert!(!data(&before).is_empty());
        assert_eq!(
            data(&before),
            data(&after),
            "body edits must not rename source statics"
        );
        let ordinary = lower(SOURCE, 1, SymbolIdentity::Artifact);
        let ordinary_names: BTreeSet<_> = ordinary.functions.iter().map(|f| &f.name).collect();
        assert!(
            names(&before)
                .iter()
                .all(|name| !ordinary_names.contains(name)),
            "query and artifact source names must occupy separate domains"
        );
    }

    #[test]
    fn partitioned_lowering_removes_whole_program_odr() {
        let ordinary = lower(SOURCE, 1, SymbolIdentity::Artifact);
        let partitioned = lower(SOURCE, 1, SymbolIdentity::Query);
        assert!(
            ordinary.functions.iter().any(|f| f.weak_odr),
            "test must reach a generic instance"
        );
        assert!(
            partitioned.functions.iter().all(|f| !f.weak_odr),
            "whole-program instances have one partition owner"
        );
        assert!(partitioned.data.iter().all(|d| !d.weak_odr));
    }

    #[test]
    fn external_builtin_object_names_and_odr_contracts_remain_unchanged() {
        let ordinary = lower_with_builtin_object(SOURCE, 1, SymbolIdentity::Artifact, true);
        let partitioned = lower_with_builtin_object(SOURCE, 1, SymbolIdentity::Query, true);
        let builtins = |module: &Module| {
            module
                .functions
                .iter()
                .filter(|f| !f.name.contains("partition_test"))
                .map(|f| f.name.clone())
                .collect::<BTreeSet<_>>()
        };
        assert!(!builtins(&ordinary).is_empty());
        assert_eq!(
            builtins(&ordinary),
            builtins(&partitioned),
            "external built-in object names must retain artifact identity"
        );
        assert!(
            partitioned.functions.iter().any(|f| f.weak_odr),
            "external object instances must retain ODR linkage"
        );
    }

    #[test]
    fn imported_object_symbols_keep_artifact_arguments_and_odr_linkage() {
        let mut db = flux_hir::SourceDb::new();
        let dependency = db.add(
            "/partition-identity/dep.rs",
            "pub fn imported<T>(x: T) -> T { x } pub fn value() -> u8 { 5 }",
        );
        let root = db.add("/partition-identity/main.rs", "struct Own(u8); fn main() { let x = dep::imported(Own(dep::value())); println!(\"{}\", x.0); }");
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                flux_hir::SourceCrate {
                    root: dependency,
                    name: flux_intern::Symbol::intern("dep"),
                    dependencies: Vec::new(),
                },
                flux_hir::SourceCrate {
                    root,
                    name: flux_intern::Symbol::intern("app"),
                    dependencies: vec![(flux_intern::Symbol::intern("dep"), 0)],
                },
            ],
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let hashes: Vec<_> = hir
            .items
            .crates
            .iter()
            .enumerate()
            .map(|(index, _)| (flux_hir::CrateId(index as u32), [index as u8; 32]))
            .collect();
        let dep = flux_hir::CrateId(
            hir.items
                .crates
                .iter()
                .position(|c| c.name.as_str() == "dep")
                .unwrap() as u32,
        );
        let compilation = Compilation {
            kind: OutputKind::Executable,
            crate_hashes: &hashes,
            imported: &[dep],
            builtin_object: false,
            required_functions: &[],
            required_instances: &[],
        };
        let ordinary = lower_crate_with_requirements(&hir, &typed, &mir, compilation)
            .unwrap()
            .module;
        let partitioned = lower_partitioned_crate(&hir, &typed, &mir, compilation)
            .unwrap()
            .module;
        let names = |module: &Module| {
            module
                .functions
                .iter()
                .map(|f| &f.name)
                .chain(module.decls.iter().map(|f| &f.name))
                .filter(|name| name.starts_with("_ZN3dep"))
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        assert!(!names(&ordinary).is_empty());
        assert_eq!(names(&ordinary), names(&partitioned));
        assert!(
            partitioned
                .functions
                .iter()
                .filter(|f| f.name.starts_with("_ZN3dep"))
                .any(|f| f.weak_odr)
        );
        keel_ir::verify_module(&partitioned).unwrap();
    }
}
