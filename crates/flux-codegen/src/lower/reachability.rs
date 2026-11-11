//! Native definitions retained by exported interfaces and their MIR templates.
use flux_hir::def_map::{Def, Ns, Resolved};
use flux_hir::{
    ConstId, CrateId, FunctionId, Hir, ItemTree, ModuleId, Owner, StructId, TraitId, TypeRef,
    Visibility,
};
use flux_mir::{Body, Constant, FileMir, Operand, Rvalue, StatementKind, TerminatorKind};
use flux_ty::{ClosureOwner, Instance, Signatures, Ty, TyKind};
use rustc_hash::{FxHashMap, FxHashSet};

pub(super) struct Roots {
    pub functions: Vec<FunctionId>,
    pub statics: Vec<ConstId>,
}

pub(super) fn library(hir: &Hir, sigs: &Signatures, mir: &FileMir, krate: CrateId) -> Roots {
    let mut reach = Reach::new(hir, sigs, mir, krate);
    for (id, function) in hir.items.functions.iter() {
        if !function.is_extern
            && function.link_name.is_some()
            && hir.items.modules[function.loc.module].krate == krate
        {
            reach.function(id, true);
        }
    }
    let mut modules = vec![hir.items.module_of(krate)];
    let mut seen = FxHashSet::default();
    while let Some(module) = modules.pop() {
        if !seen.insert(module) {
            continue;
        }
        let scope = &hir.items.def_map.scopes[module.index()];
        for binding in scope
            .types
            .values()
            .chain(scope.values.values())
            .chain(scope.anonymous_traits.iter())
        {
            if binding.vis != Visibility::Public {
                continue;
            }
            if let Def::Module(module) = binding.def {
                if hir.items.modules[module].krate == krate {
                    modules.push(module);
                }
            } else {
                reach.definition(binding.def);
            }
        }
    }
    reach.finish()
}

pub(super) fn executable(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    required: &[FunctionId],
) -> Roots {
    let krate = hir.items.main_crate();
    let mut reach = Reach::new(hir, sigs, mir, krate);
    for (id, f) in hir.items.functions.iter() {
        if (f.name.as_str() == hir.items.entry_function_name(hir.items.main_crate())
            && f.loc.module == hir.items.module_of(krate)
            && f.owner.is_none())
            || (!f.is_extern
                && f.link_name.is_some()
                && hir.items.modules[f.loc.module].krate == krate)
        {
            reach.function(id, true);
        }
    }
    for &id in required {
        reach.function(id, true);
    }
    reach.finish()
}

type Context = (ClosureOwner, flux_ty::GenericArgs);

struct Template {
    context: Context,
    runtime: bool,
    ancestors: Vec<Context>,
}

struct Reach<'a> {
    hir: &'a Hir,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    mir: &'a FileMir,
    krate: CrateId,
    functions: FxHashSet<FunctionId>,
    const_functions: FxHashSet<(FunctionId, flux_ty::GenericArgs)>,
    runtime_functions: FxHashSet<(FunctionId, flux_ty::GenericArgs)>,
    work: Vec<Template>,
    drop_parameters: FxHashMap<FunctionId, DropParameters>,
    current: Vec<Context>,
    constants: FxHashSet<(ConstId, flux_ty::GenericArgs)>,
    statics: FxHashSet<ConstId>,
    types: FxHashSet<(Ty, bool)>,
    exposed: FxHashSet<StructId>,
    opaque: FxHashSet<StructId>,
    opaque_traits: FxHashMap<TraitId, Vec<Ty>>,
    callback_bounds: FxHashMap<TraitId, Vec<flux_ty::Predicate>>,
    callback_contexts: FxHashMap<TraitId, Vec<flux_ty::GenericArgs>>,
    growing_unknowns: FxHashMap<Ty, Ty>,
    growing_embeddings: FxHashMap<(Ty, Ty), bool>,
    dropped: FxHashSet<StructId>,
    traits: FxHashSet<TraitId>,
    impls: FxHashSet<flux_hir::ImplId>,
    drop_impls: FxHashSet<flux_hir::ImplId>,
    aliases: FxHashSet<flux_hir::TypeAliasId>,
    // Inputs are immutable for this traversal. Keep top-level normalization
    // separate from recursive erasure, which deliberately treats aliases as
    // unknown even when an independent closed occurrence can normalize.
    normalized_templates: std::cell::RefCell<FxHashMap<Ty, Ty>>,
    erased_templates: std::cell::RefCell<FxHashMap<Ty, Ty>>,
    symbolic_calls: FxHashMap<(FunctionId, flux_ty::GenericArgs, bool), Vec<Context>>,
    symbolic_constants: FxHashMap<flux_ty::ConstInstance, Vec<Context>>,
    closures: FxHashMap<ClosureOwner, Vec<&'a Body>>,
}

impl<'a> Reach<'a> {
    fn new(hir: &'a Hir, sigs: &'a Signatures, mir: &'a FileMir, krate: CrateId) -> Self {
        let mut closures: FxHashMap<_, Vec<_>> = FxHashMap::default();
        for ((id, _), body) in &mir.closures {
            closures.entry(id.owner).or_default().push(body);
        }
        Self {
            hir,
            items: &hir.items,
            sigs,
            mir,
            krate,
            closures,
            functions: FxHashSet::default(),
            const_functions: FxHashSet::default(),
            runtime_functions: FxHashSet::default(),
            work: Vec::new(),
            drop_parameters: drop_parameters(&hir.items, sigs, mir),
            current: Vec::new(),
            constants: FxHashSet::default(),
            statics: FxHashSet::default(),
            types: FxHashSet::default(),
            exposed: FxHashSet::default(),
            opaque: FxHashSet::default(),
            opaque_traits: FxHashMap::default(),
            callback_bounds: FxHashMap::default(),
            callback_contexts: FxHashMap::default(),
            growing_unknowns: FxHashMap::default(),
            growing_embeddings: FxHashMap::default(),
            dropped: FxHashSet::default(),
            traits: FxHashSet::default(),
            impls: FxHashSet::default(),
            drop_impls: FxHashSet::default(),
            aliases: FxHashSet::default(),
            normalized_templates: Default::default(),
            erased_templates: Default::default(),
            symbolic_calls: FxHashMap::default(),
            symbolic_constants: FxHashMap::default(),
        }
    }

    fn own(&self, module: ModuleId) -> bool {
        self.items.modules[module].krate == self.krate
    }

    fn definition(&mut self, def: Def) {
        match def {
            Def::Function(f) => self.interface_function(f),
            Def::Const(c) | Def::Static(c) => {
                if let Some(&ty) = self.sigs.consts.get(c) {
                    self.expose(ty, true);
                }
                self.konst(c);
            }
            Def::Struct(s) | Def::Variant(s, _) => self.expose_struct(s, true),
            Def::Trait(t) => self.expose_trait(t),
            Def::TypeAlias(a) if self.aliases.insert(a) => {
                let alias = &self.items.type_aliases[a];
                self.written(alias.loc.module, &alias.ty);
                for param in &alias.generics.params {
                    if let Some((ty, _)) = &param.default {
                        self.written(alias.loc.module, ty);
                    }
                }
                for p in &alias.generics.predicates {
                    self.written(alias.loc.module, &p.ty);
                    self.path(alias.loc.module, &p.bound.path);
                }
            }
            _ => {}
        }
    }

    fn path(&mut self, module: ModuleId, path: &flux_hir::Path) {
        if let Some(q) = &path.qself {
            self.written(module, &q.ty);
            if let Some(trait_) = &q.trait_ {
                self.path(module, trait_);
            }
        }
        if let Ok(Resolved::Def(def)) = self.items.resolve(module, path, Ns::Types) {
            self.definition(def);
        }
        for args in path.args.iter().flatten() {
            self.arguments(module, args);
        }
    }

    fn arguments(&mut self, module: ModuleId, args: &flux_hir::GenericArgs) {
        for ty in &args.types {
            self.written(module, ty);
        }
        for binding in &args.bindings {
            self.written(module, &binding.ty);
            if let Some(args) = &binding.args {
                self.arguments(module, args);
            }
        }
    }

    fn written(&mut self, module: ModuleId, ty: &TypeRef) {
        match ty {
            TypeRef::Path(path) => self.path(module, path),
            TypeRef::Tuple(types) => {
                for ty in types {
                    self.written(module, ty);
                }
            }
            TypeRef::Ptr { pointee, .. }
            | TypeRef::Ref { pointee, .. }
            | TypeRef::Slice(pointee) => self.written(module, pointee),
            TypeRef::Array { elem, .. } => self.written(module, elem),
            TypeRef::FnPtr { params, ret, .. } => {
                for ty in params {
                    self.written(module, ty);
                }
                self.written(module, ret);
            }
            TypeRef::Dyn(object) => {
                for bound in &object.bounds {
                    self.path(module, &bound.path);
                }
            }
            TypeRef::ImplTrait(opaque) => {
                for bound in &opaque.bounds {
                    self.path(module, &bound.path);
                }
            }
            _ => {}
        }
    }

    fn trait_ref(&mut self, trait_ref: flux_ty::TraitRef) {
        self.expose_trait(trait_ref.trait_);
        for &ty in trait_ref.args {
            self.expose(ty, true);
        }
    }

    fn predicate(&mut self, bound: &flux_ty::Predicate) {
        self.expose(bound.self_ty, true);
        self.trait_ref(bound.trait_ref);
    }

    fn projection(&mut self, projection: &flux_ty::Projection) {
        self.expose(Ty::alias(projection.alias), true);
        self.expose(projection.ty, true);
    }

    fn interface_function(&mut self, f: FunctionId) {
        if let Some(sig) = self.sigs.fns.get(f) {
            for &ty in sig.params.iter().chain([&sig.ret]) {
                self.expose(ty, true);
            }
        }
        if let Some(bounds) = self.sigs.fn_predicates.get(f) {
            for bound in bounds {
                self.predicate(bound);
            }
        }
        if let Some(projections) = self.sigs.fn_projections.get(f) {
            for projection in projections {
                self.projection(projection);
            }
        }
        self.function(f, true);
    }

    fn expose(&mut self, ty: Ty, full: bool) {
        if !self.types.insert((ty, full)) {
            return;
        }
        match ty.kind() {
            TyKind::Adt(adt, _) => self.expose_struct(adt.strukt, full),
            TyKind::Dynamic(object) => self.expose_trait(object.trait_),
            TyKind::Alias(alias) => self.expose_trait(alias.trait_),
            TyKind::Opaque(opaque) => {
                let hidden = self
                    .sigs
                    .hidden
                    .get(&(opaque.function, opaque.index))
                    .copied();
                if let Some(bounds) = self
                    .sigs
                    .opaques
                    .get(opaque.function)
                    .and_then(|b| b.get(opaque.index as usize))
                {
                    for bound in &bounds.predicates {
                        self.trait_ref(bound.trait_ref);
                        if let Some(hidden) = hidden
                            && matches!(bound.self_ty.kind(), TyKind::Param(p) if p.index as usize == opaque.args.len())
                        {
                            for t in flux_solve::objects::supertraits_of(
                                self.items,
                                bound.trait_ref.trait_,
                            ) {
                                self.opaque_traits.entry(t).or_default().push(hidden);
                            }
                        }
                    }
                    for projection in &bounds.projections {
                        self.projection(projection);
                    }
                }
                if let Some(hidden) = hidden {
                    self.expose(hidden, false);
                }
            }
            TyKind::FnDef(def) => self.function(def.function, true),
            _ => {}
        }
        for ty in ty.children() {
            self.expose(ty, full);
        }
    }

    fn expose_struct(&mut self, s: StructId, full: bool) {
        // A genuinely private nominal type cannot be used downstream, even
        // when a public signature leaks it. Opaque hidden types use the
        // separate trait-only path below.
        if full
            && self.own(self.items.structs[s].loc.module)
            && self.items.structs[s].loc.vis != Visibility::Public
        {
            return;
        }
        let changed = if full {
            self.exposed.insert(s)
        } else {
            self.opaque.insert(s)
        };
        if !changed {
            return;
        }
        self.drop_struct(s);
        if !full {
            return;
        }
        if let Some(bounds) = self.sigs.struct_predicates.get(s) {
            for bound in bounds {
                self.predicate(bound);
            }
        }
        if let Some(projections) = self.sigs.struct_projections.get(s) {
            for projection in projections {
                self.projection(projection);
            }
        }
        let strukt = &self.items.structs[s];
        for param in &strukt.generics.params {
            if let Some((ty, _)) = &param.default {
                self.written(strukt.loc.module, ty);
            }
        }
        for (&(strukt, _), &function) in &self.items.ctor_fns {
            let data = &self.items.structs[s];
            if strukt == s
                && (data.is_enum() || data.fields.iter().all(|f| f.vis == Visibility::Public))
            {
                self.interface_function(function);
            }
        }
        let strukt = &self.items.structs[s];
        let mut fields = strukt.fields.iter().collect::<Vec<_>>();
        if let Some(variants) = &strukt.variants {
            fields = variants.iter().flat_map(|v| v.fields.iter()).collect();
        }
        if let Some(types) = self.sigs.structs.get(s) {
            for (&ty, field) in types.iter().zip(fields) {
                if strukt.is_enum() || field.vis == Visibility::Public {
                    self.expose(ty, true);
                }
            }
        }
    }

    fn expose_trait(&mut self, t: TraitId) {
        if !self.traits.insert(t) {
            return;
        }
        for &f in &self.items.traits[t].functions {
            self.interface_function(f);
        }
        for &c in &self.items.traits[t].consts {
            self.konst(c);
        }
        if let Some(sig) = self.sigs.traits.get(t) {
            for &sup in &sig.supertraits {
                self.trait_ref(sup);
            }
            for &ty in sig.defaults.iter().flatten() {
                self.expose(ty, true);
            }
            for &(_, bound) in &sig.type_bounds {
                self.trait_ref(bound);
            }
            for projection in sig.implied.iter().chain(sig.header_projections.iter()) {
                self.projection(projection);
            }
        }
    }

    fn drop_struct(&mut self, s: StructId) {
        if !self.dropped.insert(s) {
            return;
        }
        if let Some(types) = self.sigs.structs.get(s) {
            for &ty in *types {
                self.drop_type(ty);
            }
        }
    }

    fn drop_type(&mut self, ty: Ty) {
        match ty.kind() {
            TyKind::Adt(adt, args) => {
                if flux_solve::is_lang_struct(self.items, adt.strukt, "manually_drop") {
                    return;
                }
                self.drop_struct(adt.strukt);
                if let Some(imp) = flux_solve::traits::drop_impl(self.items, self.sigs, adt.strukt)
                {
                    let args = Ty::list(&self.implementation_args(imp, Ty::list(&[ty])));
                    for &f in &self.items.impls[imp].functions {
                        self.function_args(f, args, true);
                    }
                }
                if flux_solve::traits::needs_drop(self.items, self.sigs, &[], ty) {
                    for &arg in args {
                        self.drop_type(arg);
                    }
                }
            }
            TyKind::Tuple(types) => {
                for &ty in types {
                    self.drop_type(ty);
                }
            }
            TyKind::Array(elem, _) => self.drop_type(elem),
            TyKind::Closure(_, args) => {
                for &ty in flux_ty::ClosureArgs::split(args).upvar_types() {
                    self.drop_type(ty);
                }
            }
            TyKind::Opaque(opaque) => {
                if let Some(&ty) = self.sigs.hidden.get(&(opaque.function, opaque.index)) {
                    self.drop_type(ty);
                }
            }
            _ => {}
        }
    }

    fn parameters(&self, owner: ClosureOwner) -> flux_ty::GenericArgs {
        let names = match owner {
            ClosureOwner::Function(f) => flux_ty::generic_names(
                self.items,
                self.items.functions[f].owner,
                &self.items.functions[f].generics,
            ),
            ClosureOwner::Const(c) => flux_ty::generic_names(
                self.items,
                self.items.consts[c].owner,
                &flux_hir::Generics::default(),
            ),
        };
        Ty::list(
            &names
                .into_iter()
                .enumerate()
                .map(|(i, name)| Ty::param(i as u32, name))
                .collect::<Vec<_>>(),
        )
    }

    fn enqueue(&mut self, owner: ClosureOwner, args: flux_ty::GenericArgs, runtime: bool) {
        // A symbolic invocation contributes its concrete callback contracts in
        // `call`, before this point. Inspect its parametric body once, rather
        // than constructing every producer/consumer adapter combination.
        // Closed invocations retain their precise instance selection.
        let args = if args.is_empty() || args.iter().any(|ty| ty.has_params()) {
            self.parameters(owner)
        } else {
            args
        };
        // Recursive generic templates are legal before instantiation. Stop a
        // growing symbolic cycle; concrete mono still diagnoses invalid uses.
        if self.current.iter().any(|&(old, previous)| {
            old == owner
                && growing_args_cached(
                    previous,
                    args,
                    &mut self.growing_unknowns,
                    &mut self.growing_embeddings,
                )
        }) {
            return;
        }
        // Equivalent parameter renamings describe the same symbolic body.
        // Keep original arguments in the work item for substitution and use
        // the canonical form only to deduplicate analysis contexts.
        let key = template_key(args);
        let changed = match owner {
            ClosureOwner::Function(f) if runtime => self.runtime_functions.insert((f, key)),
            ClosureOwner::Function(f) => self.const_functions.insert((f, key)),
            ClosureOwner::Const(c) => self.constants.insert((c, key)),
        };
        if changed {
            self.work.push(Template {
                context: (owner, args),
                runtime,
                ancestors: self.current.clone(),
            });
        }
    }

    fn function(&mut self, f: FunctionId, runtime: bool) {
        self.function_args(f, &[], runtime);
    }

    fn function_args(&mut self, f: FunctionId, args: flux_ty::GenericArgs, runtime: bool) {
        if runtime && self.own(self.items.functions[f].loc.module) {
            self.functions.insert(f);
        }
        self.enqueue(ClosureOwner::Function(f), args, runtime);
    }

    fn konst(&mut self, c: ConstId) {
        self.konst_args(c, &[]);
    }

    fn konst_args(&mut self, c: ConstId, args: flux_ty::GenericArgs) {
        if self.own(self.items.consts[c].loc.module) && self.items.consts[c].is_static.is_some() {
            self.statics.insert(c);
        }
        self.enqueue(ClosureOwner::Const(c), args, false);
    }

    fn implementation_args(&self, imp: flux_hir::ImplId, wanted: flux_ty::GenericArgs) -> Vec<Ty> {
        let normalized: Vec<_> = wanted
            .iter()
            .map(|&ty| {
                let result = flux_solve::traits::normalize(self.items, self.sigs, &[], ty);
                if result.has_error() { ty } else { result }
            })
            .collect();
        let wanted = Ty::list(&normalized);
        let owner = &self.items.impls[imp].generics;
        let mut bindings = vec![None; owner.len()];
        if let (Some(&pattern), Some(&actual)) = (self.sigs.impls.get(imp), wanted.first()) {
            let _ = flux_solve::traits::match_ty(pattern, actual, &mut bindings);
        }
        if let Some(header) = self.sigs.impl_traits.get(imp) {
            for (&pattern, &actual) in header.args.iter().zip(wanted.iter().skip(1)) {
                let _ = flux_solve::traits::match_ty(pattern, actual, &mut bindings);
            }
        }
        owner
            .params
            .iter()
            .enumerate()
            .map(|(i, param)| bindings[i].unwrap_or_else(|| Ty::param(i as u32, param.name)))
            .collect()
    }

    fn constant_reference(&mut self, c: flux_ty::ConstInstance) {
        self.callback_contracts(
            flux_solve::traits::owner_predicates(
                self.items,
                self.sigs,
                self.items.consts[c.konst].owner,
            ),
            c.args,
        );
        self.konst_args(c.konst, c.args);
        if !c.args.iter().any(|ty| ty.has_params()) {
            let resolved = flux_solve::traits::resolve_const(self.items, self.sigs, c);
            self.konst_args(resolved.konst, resolved.args);
        } else if matches!(self.items.consts[c.konst].owner, Some(Owner::Trait(_))) {
            self.symbolic_constants
                .entry(c)
                .or_insert(self.current.clone());
            self.symbolic_constant(c);
        }
    }

    fn symbolic_constant(&mut self, c: flux_ty::ConstInstance) {
        let Some(Owner::Trait(t)) = self.items.consts[c.konst].owner else {
            return;
        };
        for &imp in self.items.trait_impls.get(&t).into_iter().flatten() {
            if let (Some(&self_ty), Some(&wanted)) = (self.sigs.impls.get(imp), c.args.first()) {
                if matches!(
                    wanted.kind(),
                    TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_)
                ) {
                    // An unknown receiver can select this crate's native
                    // helpers. Foreign implementations are instantiated by
                    // the downstream concrete caller, rather than expanding
                    // every foreign adapter around an unknown parameter.
                    if !self.own(self.items.impls[imp].module) || !self.call_available(self_ty, t) {
                        continue;
                    }
                }
                if !self.template_matches(self_ty, wanted) {
                    continue;
                }
            }
            if !self.trait_arguments_match(imp, c.args) {
                continue;
            }
            for &target in &self.items.impls[imp].consts {
                if self.items.consts[target].name == self.items.consts[c.konst].name {
                    self.konst_args(target, Ty::list(&self.implementation_args(imp, c.args)));
                }
            }
        }
    }

    fn callback_contracts(&mut self, bounds: Vec<flux_ty::Predicate>, args: flux_ty::GenericArgs) {
        let bounds: Vec<_> = bounds.into_iter().map(|bound| bound.subst(args)).collect();
        let mut pending = flux_solve::traits::elaborate(self.sigs, &bounds);
        while let Some(bound) = pending.pop() {
            if !self.callback_subject(bound.self_ty)
                && !bound
                    .trait_ref
                    .args
                    .iter()
                    .any(|&ty| self.callback_subject(ty))
            {
                continue;
            }
            let mut wanted = vec![bound.self_ty];
            wanted.extend_from_slice(bound.trait_ref.args);
            let wanted = Ty::list(&wanted);
            let contexts = self
                .callback_contexts
                .entry(bound.trait_ref.trait_)
                .or_default();
            if contexts.iter().any(|&old| {
                old == wanted
                    || growing_args_cached(
                        old,
                        wanted,
                        &mut self.growing_unknowns,
                        &mut self.growing_embeddings,
                    )
            }) {
                continue;
            }
            contexts.push(wanted);
            self.expose_trait(bound.trait_ref.trait_);
            self.callback_bounds
                .entry(bound.trait_ref.trait_)
                .or_default()
                .push(bound);
            // A foreign blanket implementation can introduce callbacks absent
            // from the called function's written bounds. Follow its contract,
            // without instantiating its adapter body in this context.
            let implementations = self
                .items
                .trait_impls
                .get(&bound.trait_ref.trait_)
                .cloned()
                .unwrap_or_default();
            for imp in implementations {
                let Some(&self_ty) = self.sigs.impls.get(imp) else {
                    continue;
                };
                if !self.template_matches(self_ty, bound.self_ty)
                    || !self.trait_arguments_match(imp, wanted)
                {
                    continue;
                }
                if matches!(
                    bound.self_ty.kind(),
                    TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_)
                ) && !self.own(self.items.impls[imp].module)
                {
                    continue;
                }
                let args = Ty::list(&self.implementation_args(imp, wanted));
                let requirements: Vec<_> = self
                    .sigs
                    .impl_predicates
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .map(|bound| bound.subst(args))
                    .collect();
                pending.extend(flux_solve::traits::elaborate(self.sigs, &requirements));
                let outputs: Vec<_> = self
                    .sigs
                    .impl_types
                    .get(imp)
                    .into_iter()
                    .flatten()
                    .map(|&(_, ty)| ty.subst(args))
                    .collect();
                for output in outputs {
                    self.expose(output, true);
                }
            }
        }
    }

    fn call(&mut self, f: FunctionId, args: flux_ty::GenericArgs, runtime: bool) {
        self.callback_contracts(
            flux_solve::traits::predicates_of(self.items, self.sigs, f),
            args,
        );
        if runtime && args.iter().any(|ty| ty.has_params()) {
            let effects = self.drop_parameters.get(&f).cloned().unwrap_or_default();
            for index in effects.owned {
                if let Some(&ty) = args.get(index as usize) {
                    self.drop_type(ty);
                }
            }
            let mut pending: Vec<_> = effects
                .projected
                .into_iter()
                .filter_map(|index| args.get(index as usize).copied())
                .collect();
            let mut seen = FxHashSet::default();
            while let Some(ty) = pending.pop() {
                if seen.insert(ty) {
                    self.drop_type(ty);
                    pending.extend(ty.children());
                }
            }
        }
        // Symbolic templates do not provide the concrete bounds required for
        // instance selection. Retain matching implementations conservatively.
        if args.iter().any(|ty| ty.has_params()) {
            self.function_args(f, args, runtime);
            if matches!(self.items.functions[f].owner, Some(Owner::Trait(_))) {
                self.symbolic_calls
                    .entry((f, args, runtime))
                    .or_insert(self.current.clone());
                self.symbolic_call(f, args, runtime);
            }
            return;
        }
        let resolved = flux_solve::traits::resolve_instance(
            self.items,
            self.sigs,
            Instance { function: f, args },
        );
        self.function_args(resolved.function, resolved.args, runtime);
    }

    fn symbolic_call(&mut self, f: FunctionId, args: flux_ty::GenericArgs, runtime: bool) {
        let Some(Owner::Trait(t)) = self.items.functions[f].owner else {
            return;
        };
        for &imp in self.items.trait_impls.get(&t).into_iter().flatten() {
            if let (Some(&self_ty), Some(&wanted)) = (self.sigs.impls.get(imp), args.first()) {
                if matches!(
                    wanted.kind(),
                    TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_)
                ) {
                    // An unknown receiver can select this crate's native
                    // helpers. Foreign implementations are instantiated by
                    // the downstream concrete caller, rather than expanding
                    // every foreign adapter around an unknown parameter.
                    if !self.own(self.items.impls[imp].module) || !self.call_available(self_ty, t) {
                        continue;
                    }
                }
                if !self.template_matches(self_ty, wanted) {
                    continue;
                }
            }
            if !self.trait_arguments_match(imp, args) {
                continue;
            }
            for &target in &self.items.impls[imp].functions {
                if self.items.functions[target].name == self.items.functions[f].name {
                    let mut bindings = self.implementation_args(imp, args);
                    let header = self.sigs.impl_traits.get(imp).map_or(0, |t| t.args.len());
                    bindings.extend_from_slice(args.get(1 + header..).unwrap_or(&[]));
                    self.function_args(target, Ty::list(&bindings), runtime);
                }
            }
        }
    }

    fn trait_arguments_match(&self, imp: flux_hir::ImplId, wanted: flux_ty::GenericArgs) -> bool {
        self.sigs.impl_traits.get(imp).is_none_or(|header| {
            wanted.len() > header.args.len()
                && header
                    .args
                    .iter()
                    .zip(wanted.iter().skip(1))
                    .all(|(&left, &right)| self.template_matches(left, right))
        })
    }

    fn template_matches(&self, left: Ty, right: Ty) -> bool {
        fn erase_unresolved(ty: Ty, memo: &mut FxHashMap<Ty, Ty>) -> Ty {
            if let Some(&erased) = memo.get(&ty) {
                return erased;
            }
            let erased = match ty.kind() {
                TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Infer(_) => {
                    Ty::param(u32::MAX, flux_intern::Symbol::intern("_flib"))
                }
                _ => ty.map_children(|child| erase_unresolved(child, memo)),
            };
            memo.insert(ty, erased);
            erased
        }
        let canonical = |ty: Ty| {
            if let Some(&known) = self.normalized_templates.borrow().get(&ty) {
                return known;
            }
            let candidate = flux_solve::traits::normalize(self.items, self.sigs, &[], ty);
            let normalized = if candidate.has_error() { ty } else { candidate };
            let erased = erase_unresolved(normalized, &mut self.erased_templates.borrow_mut());
            self.normalized_templates.borrow_mut().insert(ty, erased);
            erased
        };
        flux_ty_ir::overlap(canonical(left), canonical(right))
    }

    fn callback_available(&self, imp: flux_hir::ImplId, self_ty: Ty, trait_: TraitId) -> bool {
        self.callback_bounds.get(&trait_).is_some_and(|bounds| {
            bounds.iter().any(|bound| {
                self.template_matches(self_ty, bound.self_ty)
                    && self.sigs.impl_traits.get(imp).is_some_and(|header| {
                        header.args.len() == bound.trait_ref.args.len()
                            && header
                                .args
                                .iter()
                                .zip(bound.trait_ref.args)
                                .all(|(&left, &right)| self.template_matches(left, right))
                    })
            })
        })
    }

    fn call_available(&self, ty: Ty, trait_: TraitId) -> bool {
        self.trait_available(ty, trait_)
    }

    fn trait_available(&self, ty: Ty, trait_: TraitId) -> bool {
        self.available(ty)
            || self
                .opaque_traits
                .get(&trait_)
                .is_some_and(|types| types.iter().any(|&hidden| flux_ty_ir::overlap(hidden, ty)))
    }

    fn callback_subject(&self, ty: Ty) -> bool {
        !self.available(ty)
            || matches!(ty.kind(), TyKind::Alias(_) | TyKind::Opaque(_))
            || ty.children().any(|child| self.callback_subject(child))
    }

    fn available(&self, ty: Ty) -> bool {
        match ty.kind() {
            TyKind::Adt(adt, _) => {
                !self.own(self.items.structs[adt.strukt].loc.module)
                    || self.exposed.contains(&adt.strukt)
            }
            TyKind::Tuple(types) => types.iter().all(|&ty| self.available(ty)),
            TyKind::Ref(_, ty)
            | TyKind::RawPtr(_, ty)
            | TyKind::Array(ty, _)
            | TyKind::Slice(ty) => self.available(ty),
            _ => true,
        }
    }

    fn body(&mut self, body: &Body, runtime: bool) {
        if runtime {
            let mut types: Vec<_> = body.locals.values().map(|local| local.ty).collect();
            let mut seen = FxHashSet::default();
            while let Some(ty) = types.pop() {
                if !seen.insert(ty) {
                    continue;
                }
                if let TyKind::FnDef(def) = ty.kind() {
                    self.call(def.function, def.args, true);
                }
                types.extend(ty.children());
            }
        }
        for c in body.const_items() {
            self.constant_reference(c);
        }
        for block in body.blocks.values() {
            if let TerminatorKind::Call {
                func, generic_args, ..
            } = &block.terminator.kind
            {
                self.call(*func, generic_args, runtime);
                // Polling is introduced only when specializing the saved async
                // body. Its local non-generic implementations must already be
                // retained in the library object consumed downstream.
                if self.items.functions[*func].intrinsic
                    == Some(flux_hir::Intrinsic::CoroutineAwait)
                    && let Some(&future) = generic_args.first()
                    && let Some(&trait_) = self
                        .items
                        .lang_traits
                        .get(&flux_intern::Symbol::intern("future_trait"))
                    && let Some(poll) = self.items.traits[trait_]
                        .function(self.items, flux_intern::Symbol::intern("poll"))
                {
                    self.call(poll, Ty::list(&[future]), runtime);
                }
            }
            if runtime && let TerminatorKind::Drop { place, .. } = &block.terminator.kind {
                self.drop_type(body.place_ty(place, self.sigs));
            }
            for statement in &block.statements {
                if runtime && let StatementKind::Assign(_, Rvalue::Unsize(op, to)) = &statement.kind
                {
                    let from = body.operand_ty(op, self.sigs);
                    let concrete = super::pointee_of(from)
                        .or_else(|| flux_ty::shared_pointee(self.items, from));
                    let object =
                        super::pointee_of(*to).or_else(|| flux_ty::shared_pointee(self.items, *to));
                    if let (Some(concrete), Some(object)) = (concrete, object) {
                        self.vtable(object, concrete);
                    }
                }
            }
            let operands = block
                .statements
                .iter()
                .filter_map(|s| match &s.kind {
                    StatementKind::Assign(_, r) => Some(r),
                    _ => None,
                })
                .flat_map(Rvalue::operands)
                .chain(block.terminator.kind.operands());
            for op in operands {
                if let Operand::Const(Constant::Fn { function, args, .. }) = op {
                    self.call(*function, args, true);
                }
            }
        }
        for value in &body.promoted {
            self.value(value);
        }
    }

    fn vtable(&mut self, object: Ty, concrete: Ty) {
        if !object.is_dyn() || concrete.is_dyn() {
            return;
        }
        self.drop_type(concrete);
        if object.has_params() || concrete.has_params() {
            if let TyKind::Dynamic(d) = object.kind() {
                for bound in flux_solve::traits::dyn_bounds(self.sigs, object, d) {
                    let mut args = vec![concrete];
                    args.extend_from_slice(bound.trait_ref.args);
                    let args = Ty::list(&args);
                    for &function in &self.items.traits[bound.trait_ref.trait_].functions {
                        self.call(function, args, true);
                    }
                }
            }
            return;
        }
        if let Some(instance) = flux_mir::vtable::drop_entry(self.items, self.sigs, concrete) {
            self.function_args(instance.function, instance.args, true);
        }
        let mut pending = vec![object];
        let mut seen = FxHashSet::default();
        while let Some(object) = pending.pop() {
            if !seen.insert(object) {
                continue;
            }
            for entry in flux_mir::vtable::entries(self.items, self.sigs, object, concrete) {
                match entry {
                    flux_mir::vtable::Entry::Method(instance) => {
                        self.function_args(instance.function, instance.args, true)
                    }
                    flux_mir::vtable::Entry::Super(object) => pending.push(object),
                    flux_mir::vtable::Entry::Null => {}
                }
            }
        }
    }

    fn value(&mut self, value: &flux_mir::eval::Value) {
        use flux_mir::eval::Value;
        let mut pending = vec![value];
        let mut allocations = FxHashSet::default();
        while let Some(value) = pending.pop() {
            match value {
                Value::Function(instance) => self.call(instance.function, instance.args, true),
                Value::Static(c) | Value::StaticPart { id: c, .. } => self.konst(*c),
                Value::DynPointer {
                    pointer,
                    concrete,
                    object,
                    ..
                } => {
                    self.vtable(*object, *concrete);
                    if allocations.insert((0, std::sync::Arc::as_ptr(pointer) as usize)) {
                        pending.push(pointer);
                    }
                }
                Value::Aggregate(fields) | Value::Variant { fields, .. } => {
                    if allocations
                        .insert((1, std::sync::Arc::as_ptr(fields) as *const Value as usize))
                    {
                        pending.extend(fields.iter());
                    }
                }
                Value::Ref(value)
                | Value::InteriorRef(value)
                | Value::PointerView { pointer: value, .. }
                | Value::PointerOffset { pointer: value, .. }
                | Value::LocalRef { value, .. }
                    if allocations.insert((0, std::sync::Arc::as_ptr(value) as usize)) =>
                {
                    pending.push(value);
                }
                _ => {}
            }
        }
    }

    fn finish(mut self) -> Roots {
        loop {
            while let Some(template) = self.work.pop() {
                let (owner, args) = template.context;
                self.current = template.ancestors;
                self.current.push(template.context);
                let mir = self.mir;
                let hir = self.hir;
                let sigs = self.sigs;
                let subst = |body: &Body| {
                    if args.iter().any(|ty| ty.has_params()) {
                        flux_mir::mono::subst_template(body, args)
                    } else {
                        flux_mir::mono::subst_body(hir, sigs, body, args)
                    }
                };
                match owner {
                    ClosureOwner::Function(f) => {
                        if let Some(body) = mir.bodies.get(f) {
                            let body = subst(body);
                            self.body(&body, template.runtime);
                        }
                    }
                    ClosureOwner::Const(c) => {
                        if let Some(value) = mir.const_values.get(c, args) {
                            self.value(value);
                        } else if let Some(body) = mir.consts.get(c) {
                            let body = subst(body);
                            self.body(&body, false);
                        }
                    }
                }
                if let Some(bodies) = self.closures.get(&owner).cloned() {
                    for body in bodies {
                        let body = subst(body);
                        self.body(&body, template.runtime);
                    }
                }
            }
            // Deferred dispatch keeps its original ancestry. Clearing it
            // would restart a growing generic cycle in every fixed-point round.
            let calls: Vec<_> = self
                .symbolic_calls
                .iter()
                .map(|(call, ancestors)| (*call, ancestors.clone()))
                .collect();
            for ((function, args, runtime), ancestors) in calls {
                self.current = ancestors;
                self.symbolic_call(function, args, runtime);
            }
            let constants: Vec<_> = self
                .symbolic_constants
                .iter()
                .map(|(constant, ancestors)| (*constant, ancestors.clone()))
                .collect();
            for (c, ancestors) in constants {
                self.current = ancestors;
                self.symbolic_constant(c);
            }
            self.current.clear();
            let before = (
                self.types.len(),
                self.exposed.len(),
                self.opaque.len(),
                self.dropped.len(),
                self.traits.len(),
            );
            for (id, imp) in self.items.impls.iter() {
                if !self.own(imp.module) {
                    continue;
                }
                let Some(&self_ty) = self.sigs.impls.get(id) else {
                    continue;
                };
                let drop = imp.trait_.is_some_and(|t| {
                    self.items.traits[t]
                        .lang
                        .is_some_and(|name| name.as_str() == "drop")
                });
                let drop_reachable = drop
                    && matches!(self_ty.kind(), TyKind::Adt(adt, _) if self.dropped.contains(&adt.strukt));
                if drop_reachable && self.drop_impls.insert(id) {
                    for &f in &imp.functions {
                        self.function(f, true);
                    }
                }
                let eligible = match imp.trait_ {
                    Some(t) => {
                        (self.trait_available(self_ty, t)
                            || self.callback_available(id, self_ty, t))
                            && (!self.own(self.items.traits[t].loc.module)
                                || self.traits.contains(&t))
                    }
                    None => {
                        matches!(self_ty.kind(), TyKind::Adt(adt, _) if self.exposed.contains(&adt.strukt))
                    }
                };
                if !eligible || !self.impls.insert(id) {
                    continue;
                }
                if let Some(bounds) = self.sigs.impl_predicates.get(id) {
                    for bound in bounds {
                        self.predicate(bound);
                    }
                }
                if let Some(projections) = self.sigs.impl_projections.get(id) {
                    for projection in projections {
                        self.projection(projection);
                    }
                }
                for &f in &imp.functions {
                    if imp.trait_.is_some() || self.items.functions[f].loc.vis == Visibility::Public
                    {
                        self.interface_function(f);
                    }
                }
                for &c in &imp.consts {
                    if imp.trait_.is_some() || self.items.consts[c].loc.vis == Visibility::Public {
                        if let Some(&ty) = self.sigs.consts.get(c) {
                            self.expose(ty, true);
                        }
                        self.konst(c);
                    }
                }
                if let Some(types) = self.sigs.impl_types.get(id) {
                    for &(_, ty) in types {
                        self.expose(ty, true);
                    }
                }
            }
            if self.work.is_empty()
                && before
                    == (
                        self.types.len(),
                        self.exposed.len(),
                        self.opaque.len(),
                        self.dropped.len(),
                        self.traits.len(),
                    )
            {
                break;
            }
        }
        let mut functions: Vec<_> = self.functions.into_iter().collect();
        functions.sort_unstable();
        let mut statics: Vec<_> = self.statics.into_iter().collect();
        statics.sort_unstable();
        Roots { functions, statics }
    }
}

/// Canonicalizes parameter names and indices without changing correlations,
/// concrete constructors, projections or callback lifetime binders.
fn template_key(args: flux_ty::GenericArgs) -> flux_ty::GenericArgs {
    fn visit(ty: Ty, memo: &mut FxHashMap<Ty, Ty>, next: &mut u32) -> Ty {
        if let Some(&known) = memo.get(&ty) {
            return known;
        }
        let result = if matches!(ty.kind(), TyKind::Param(_)) {
            let parameter = Ty::param(*next, flux_intern::Symbol::intern("_flib"));
            *next += 1;
            parameter
        } else {
            ty.map_children(|child| visit(child, memo, next))
        };
        memo.insert(ty, result);
        result
    }
    if !args.iter().any(|ty| ty.has_params()) {
        return args;
    }
    let mut memo = FxHashMap::default();
    let mut next = 0;
    Ty::list(
        &args
            .iter()
            .map(|&ty| visit(ty, &mut memo, &mut next))
            .collect::<Vec<_>>(),
    )
}

#[derive(Clone, Default)]
struct DropParameters {
    owned: FxHashSet<u32>,
    // A projection may return an owned value of a nested argument even when
    // the receiver itself is a reference or a non-owning wrapper.
    projected: FxHashSet<u32>,
}
impl DropParameters {
    fn extend(&mut self, other: Self) -> bool {
        let mut changed = false;
        for index in other.owned {
            changed |= self.owned.insert(index);
        }
        for index in other.projected {
            changed |= self.projected.insert(index);
        }
        changed
    }
}

// Native drop glue follows ownership effects, not every type mentioned by a
// generic call. The finite parameter sets propagate through call edges without
// constructing the corresponding nested adapter types.
fn drop_parameters(
    items: &ItemTree,
    sigs: &Signatures,
    mir: &FileMir,
) -> FxHashMap<FunctionId, DropParameters> {
    fn parameters(items: &ItemTree, ty: Ty, projected: bool) -> DropParameters {
        let mut result = DropParameters::default();
        let mut pending = vec![(ty, projected)];
        let mut seen = FxHashSet::default();
        while let Some((ty, mut projected)) = pending.pop() {
            if !ty.has_params() || !seen.insert((ty, projected)) {
                continue;
            }
            projected |= matches!(ty.kind(), TyKind::Alias(_) | TyKind::Opaque(_));
            match ty.kind() {
                TyKind::Param(p) => {
                    if projected {
                        result.projected.insert(p.index);
                    } else {
                        result.owned.insert(p.index);
                    }
                }
                _ if projected => pending.extend(ty.children().map(|ty| (ty, true))),
                TyKind::Ref(..) | TyKind::RawPtr(..) | TyKind::FnDef(_) | TyKind::FnPtr(_) => {}
                TyKind::Adt(adt, _)
                    if flux_solve::is_lang_struct(items, adt.strukt, "manually_drop")
                        || flux_solve::is_lang_struct(items, adt.strukt, "phantom_data") => {}
                TyKind::Closure(_, args) => pending.extend(
                    flux_ty::ClosureArgs::split(args)
                        .upvar_types()
                        .iter()
                        .map(|&ty| (ty, false)),
                ),
                _ => pending.extend(ty.children().map(|ty| (ty, false))),
            }
        }
        result
    }
    let mut drops: FxHashMap<FunctionId, DropParameters> = FxHashMap::default();
    let mut edges = Vec::new();
    for (f, function) in items.functions.iter() {
        if matches!(function.owner, Some(Owner::Trait(_))) && mir.bodies.get(f).is_none() {
            let count = flux_ty::generic_names(items, function.owner, &function.generics).len();
            drops
                .entry(f)
                .or_default()
                .projected
                .extend((0..count).map(|i| i as u32));
        }
    }
    let bodies = mir
        .bodies
        .iter()
        .chain(
            mir.closures
                .iter()
                .filter_map(|((closure, _), body)| match closure.owner {
                    ClosureOwner::Function(f) => Some((f, body)),
                    ClosureOwner::Const(_) => None,
                }),
        );
    for (f, body) in bodies {
        for block in body.blocks.values() {
            match &block.terminator.kind {
                TerminatorKind::Drop { place, .. } => {
                    drops.entry(f).or_default().extend(parameters(
                        items,
                        body.place_ty(place, sigs),
                        false,
                    ));
                }
                TerminatorKind::Call {
                    func, generic_args, ..
                } => edges.push((f, *func, *generic_args)),
                TerminatorKind::CallIndirect { args, .. } => {
                    for arg in args {
                        drops.entry(f).or_default().extend(parameters(
                            items,
                            body.operand_ty(arg, sigs),
                            false,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    loop {
        let mut changed = false;
        for &(owner, callee, args) in &edges {
            let mut inherited = DropParameters::default();
            if let Some(dropped) = drops.get(&callee) {
                for (indices, projected) in [(&dropped.owned, false), (&dropped.projected, true)] {
                    for &index in indices {
                        if let Some(&ty) = args.get(index as usize) {
                            inherited.extend(parameters(items, ty, projected));
                        }
                    }
                }
            }
            changed |= drops.entry(owner).or_default().extend(inherited);
        }
        if !changed {
            return drops;
        }
    }
}

#[cfg(test)]
fn growing_args(previous: flux_ty::GenericArgs, next: flux_ty::GenericArgs) -> bool {
    growing_args_cached(
        previous,
        next,
        &mut FxHashMap::default(),
        &mut FxHashMap::default(),
    )
}

fn growing_args_cached(
    previous: flux_ty::GenericArgs,
    next: flux_ty::GenericArgs,
    memo: &mut FxHashMap<Ty, Ty>,
    embeddings: &mut FxHashMap<(Ty, Ty), bool>,
) -> bool {
    fn unknowns(ty: Ty, memo: &mut FxHashMap<Ty, Ty>) -> Ty {
        if let Some(&known) = memo.get(&ty) {
            return known;
        }
        let result = if matches!(ty.kind(), TyKind::Param(_)) {
            Ty::param(u32::MAX, flux_intern::Symbol::intern("_flib"))
        } else {
            ty.map_children(|child| unknowns(child, memo))
        };
        memo.insert(ty, result);
        result
    }
    if previous.len() != next.len() {
        return false;
    }
    // Callback contexts repeatedly compare the same nested adapter types.
    // Canonicalization and embedding depend only on interned types. Reuse both
    // caches within this Reach, including comparisons between different calls.
    let previous: Vec<_> = previous.iter().map(|&ty| unknowns(ty, memo)).collect();
    let next: Vec<_> = next.iter().map(|&ty| unknowns(ty, memo)).collect();
    // Changing equality relationships between parameters is not growth. Such
    // contexts retain distinct canonical keys and must each be inspected.
    previous != next
        && previous
            .into_iter()
            .zip(next)
            .all(|(left, right)| embeds_cached(left, right, embeddings))
}

// Homeomorphic embedding detects growing template cycles without imposing a
// monomorphization limit on a library's uninstantiated generic interfaces.
#[cfg(test)]
fn embeds(previous: Ty, next: Ty) -> bool {
    embeds_cached(previous, next, &mut FxHashMap::default())
}
fn embeds_cached(previous: Ty, next: Ty, cache: &mut FxHashMap<(Ty, Ty), bool>) -> bool {
    fn visit(previous: Ty, next: Ty, cache: &mut FxHashMap<(Ty, Ty), bool>) -> bool {
        if previous == next {
            return true;
        }
        if let Some(&known) = cache.get(&(previous, next)) {
            return known;
        }
        let left: Vec<_> = previous.children().collect();
        let right: Vec<_> = next.children().collect();
        let same_head = match (previous.kind(), next.kind()) {
            (TyKind::Tuple(a), TyKind::Tuple(b)) => a.len() == b.len(),
            (TyKind::Adt(a, _), TyKind::Adt(b, _)) => a.strukt == b.strukt,
            (TyKind::Ref(a, _), TyKind::Ref(b, _))
            | (TyKind::RawPtr(a, _), TyKind::RawPtr(b, _)) => a == b,
            (TyKind::Array(_, _), TyKind::Array(_, _)) | (TyKind::Slice(_), TyKind::Slice(_)) => {
                true
            }
            (TyKind::FnDef(a), TyKind::FnDef(b)) => a.function == b.function,
            (TyKind::Closure(a, _), TyKind::Closure(b, _)) => a == b,
            (TyKind::Alias(a), TyKind::Alias(b)) => a.trait_ == b.trait_ && a.name == b.name,
            (TyKind::Opaque(a), TyKind::Opaque(b)) => {
                a.function == b.function && a.index == b.index
            }
            (TyKind::Dynamic(a), TyKind::Dynamic(b)) => {
                a.trait_ == b.trait_
                    && a.auto_traits == b.auto_traits
                    && a.assoc_names == b.assoc_names
            }
            (TyKind::FnPtr(a), TyKind::FnPtr(b)) => {
                a.abi == b.abi
                    && a.is_unsafe == b.is_unsafe
                    && a.inputs_output.len() == b.inputs_output.len()
                    // Captured types participate in the child embedding below.
                    // Comparing them here would hide growing callback cycles.
                    && a.regions.map(|r| (r.bound, r.free, r.inputs_output, r.types.len()))
                        == b.regions.map(|r| (r.bound, r.free, r.inputs_output, r.types.len()))
            }
            _ => false,
        };
        let result = right.iter().any(|&child| visit(previous, child, cache))
            || same_head
                && left.len() == right.len()
                && left.iter().zip(&right).all(|(&a, &b)| visit(a, b, cache));
        cache.insert((previous, next), result);
        result
    }
    visit(previous, next, cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_template_subtrees_are_compared_as_a_dag() {
        let parameter = Ty::param(0, flux_intern::Symbol::intern("T"));
        let mut repeated = parameter;
        for _ in 0..32 {
            repeated = Ty::tuple(&[repeated, repeated]);
        }
        assert!(growing_args(Ty::list(&[parameter]), Ty::list(&[repeated])));
    }
    #[test]
    fn template_keys_preserve_correlations_and_concrete_heads() {
        let parameter = |index, name| Ty::param(index, flux_intern::Symbol::intern(name));
        let (a, b) = (parameter(0, "Input"), parameter(1, "Output"));
        let (x, y) = (parameter(4, "I"), parameter(7, "O"));
        assert_eq!(
            template_key(Ty::list(&[a, b])),
            template_key(Ty::list(&[x, y]))
        );
        assert_ne!(
            template_key(Ty::list(&[a, a])),
            template_key(Ty::list(&[x, y]))
        );
        assert_ne!(
            template_key(Ty::list(&[a, b])),
            template_key(Ty::list(&[x, x]))
        );
        assert!(growing_args(
            Ty::list(&[a, b]),
            Ty::list(&[Ty::tuple(&[x]), y])
        ));
        assert!(!growing_args(Ty::list(&[a, a]), Ty::list(&[x, y])));
        assert!(!growing_args(
            Ty::list(&[Ty::int(flux_ty::IntTy::U32)]),
            Ty::list(&[Ty::int(flux_ty::IntTy::U64)])
        ));
        assert!(!growing_args(
            Ty::list(&[a]),
            Ty::list(&[Ty::int(flux_ty::IntTy::U32)])
        ));
    }
    #[test]
    fn growing_callback_captures_embed_without_erasing_their_binders() {
        let parameter = Ty::param(0, flux_intern::Symbol::intern("T"));
        let pointer = |input, bound| {
            let mut signature = flux_ty::FnSigTy::new(&[input], Ty::UNIT, false);
            signature.regions = Some(flux_ty::FnPtrRegions::intern(
                bound,
                0,
                &[vec![flux_ty::FnRegion::Type(0)], vec![]],
                &[input],
            ));
            Ty::new(TyKind::FnPtr(signature))
        };
        let original = pointer(parameter, 0);
        assert!(embeds(original, pointer(Ty::tuple(&[parameter]), 0)));
        assert!(!embeds(original, pointer(Ty::tuple(&[parameter]), 1)));
    }
}
