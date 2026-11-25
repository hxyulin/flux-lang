//! Use a private result-pointer ABI for tail calls between aggregate-return bodies.

use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{AbiParam, CallConv, Function, InstData, Module, Purpose, Signature, Type};

pub(super) fn run(module: &mut Module, seeds: &[usize]) -> Result<(), keel_opt::OptError> {
    let eligible = module.functions.iter().map(eligible).collect::<Vec<_>>();
    let definitions: FxHashMap<_, _> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, _)| eligible[*id])
        .map(|(id, function)| (function.name.as_str(), id))
        .collect();
    let mut edges = vec![Vec::new(); eligible.len()];
    for (id, source) in module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, _)| eligible[*id])
    {
        let result = source.block_params(source.entry_block().expect("eligible body has entry"))[0];
        for (_, inst) in source.layout_insts() {
            if let InstData::Call { func, args } = source.inst_data(inst)
                && !args.has_explicit_abi()
                && args
                    .first()
                    .is_some_and(|&value| source.resolve(value) == result)
            {
                let callee = source.callee(*func);
                if callee.sig == source.sig
                    && let Some(&other) = definitions.get(callee.name.as_str())
                    && module.functions[other].sig == callee.sig
                {
                    edges[id].push(other);
                    edges[other].push(id);
                }
            }
        }
    }
    let mut pending = seeds.to_vec();
    let mut selected = vec![false; eligible.len()];
    while let Some(id) = pending.pop() {
        if eligible[id] && !std::mem::replace(&mut selected[id], true) {
            pending.extend(&edges[id]);
        }
    }
    let existing: FxHashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            std::iter::once(f.name.as_str()).chain(f.callees().map(|(_, decl)| decl.name.as_str()))
        })
        .chain(module.decls.iter().map(|decl| decl.name.as_str()))
        .chain(module.data.iter().map(|data| data.name.as_str()))
        .collect();
    let mut replacements = FxHashMap::default();
    for (id, function) in module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, _)| selected[*id])
    {
        let name = format!("__flux_private_result_{id}");
        if existing.contains(name.as_str()) {
            return Ok(());
        }
        let private = PrivateCall {
            name,
            original: function.sig.clone(),
            signature: private_signature(&function.sig),
        };
        replacements.insert(function.name.clone(), private);
    }
    if replacements.is_empty() {
        return Ok(());
    }
    for (id, function) in module.functions.iter_mut().enumerate() {
        // Leave the original signatures and address-taking uses available.
        // Only direct calls with the original signature can use a private entry.
        rewrite_calls(function, &replacements, selected[id]);
    }
    let mut entries = Vec::new();
    for function in &mut module.functions {
        if let Some(private) = replacements.get(&function.name) {
            let mut public = Function::new(function.name.clone(), function.sig.clone());
            public.attrs = function.attrs.clone();
            public.local = function.local;
            public.hidden = function.hidden;
            public.weak_odr = function.weak_odr;
            let entry = public.create_block();
            let args = private
                .signature
                .params
                .iter()
                .map(|param| public.append_block_param(entry, param.ty))
                .collect::<Vec<_>>();
            let callee = public.import_function(&private.name, private.signature.clone());
            public.append_inst(
                entry,
                InstData::Call {
                    func: callee,
                    args: args.into(),
                },
            );
            public.append_inst(entry, InstData::Return { args: Vec::new() });
            let mut body = std::mem::replace(function, public);
            body.name = private.name.clone();
            body.sig = private.signature.clone();
            body.local = true;
            body.hidden = false;
            body.weak_odr = false;
            entries.push(body);
        }
    }
    module.functions.extend(entries);
    module.sync_function_attrs();
    keel_opt::run_passes(module, &["compact-unused-functions"])
}

struct PrivateCall {
    name: String,
    original: Signature,
    signature: Signature,
}

fn private_signature(original: &Signature) -> Signature {
    let mut signature = original.clone();
    signature.params[0] = AbiParam::new(Type::Ptr);
    signature.cc = CallConv::Fast;
    signature
}

pub(super) fn eligible(f: &Function) -> bool {
    let Some((first, rest)) = f.sig.params.split_first() else {
        return false;
    };
    f.entry_block().is_some()
        && (f.local || f.attrs.noninterposable)
        && !f.attrs.weak
        && !f.weak_odr
        && f.attrs.nounwind
        && !f.attrs.noreturn
        && !f.attrs.optnone
        && !f.attrs.naked
        && !f.attrs.interrupt
        && !f.attrs.frame_pointer
        && !f.attrs.constructor
        && !f.attrs.destructor
        && f.attrs.section.is_none()
        && !f.attrs.used
        && !f.attrs.retain
        && f.attrs.contract.is_unknown()
        && f.restrict_params.is_empty()
        && f.personality.is_none()
        && f.debug_loc.is_none()
        && f.debug_return.is_none()
        && f.debug_vars.is_empty()
        && !f.sig.variadic
        && !f.sig.returns_twice
        && f.sig.returns.is_empty()
        && f.sig.cc == CallConv::C
        && f.sig.x86_vector_abi.is_none()
        && first.ty == Type::Ptr
        && matches!(first.purpose, Purpose::Sret(_))
        && rest.len() < 6
        && rest.iter().all(|param| {
            param.purpose == Purpose::Normal && matches!(param.ty, Type::Ptr | Type::I64)
        })
        && !f.layout_insts().any(|(_, inst)| {
            matches!(
                f.inst_data(inst),
                InstData::InlineAsm { .. }
                    | InstData::X86RflagsRegion { .. }
                    | InstData::Invoke { .. }
                    | InstData::LandingPad { .. }
                    | InstData::ReturnCall { .. }
                    | InstData::ReturnCallIndirect { .. }
            )
        })
}

fn frame_unobservable(function: &Function) -> bool {
    // A sibling transfer can retire the stack frame only when its address
    // cannot have been observed, including through an earlier call.
    !function.attrs.no_sibling_calls
        && function.layout_insts().all(|(_, inst)| {
            matches!(
                function.inst_data(inst),
                InstData::Iconst { .. }
                    | InstData::Fconst { .. }
                    | InstData::Unary { .. }
                    | InstData::Binary { .. }
                    | InstData::Select { .. }
                    | InstData::Convert { .. }
                    | InstData::Icmp { .. }
                    | InstData::Fcmp { .. }
                    | InstData::Load { .. }
                    | InstData::Store { .. }
                    | InstData::Jump { .. }
                    | InstData::Brif { .. }
                    | InstData::Switch { .. }
                    | InstData::Trap
                    | InstData::Return { .. }
                    | InstData::ReturnCall { .. }
                    | InstData::Call { .. }
                    | InstData::SymbolAddr { .. }
            )
        })
}

fn rewrite_calls(
    function: &mut Function,
    replacements: &FxHashMap<String, PrivateCall>,
    selected_for_tail: bool,
) {
    let expected = if selected_for_tail && frame_unobservable(function) {
        let entry = function
            .entry_block()
            .expect("private tail source has an entry");
        Some(function.block_params(entry)[0])
    } else {
        None
    };
    let mut sites = Vec::new();
    for (block, inst) in function.layout_insts() {
        let InstData::Call { func, args } = function.inst_data(inst) else {
            continue;
        };
        let decl = function.callee(*func);
        let Some(private) = replacements.get(&decl.name) else {
            continue;
        };
        if decl.sig != private.original
            || args.has_explicit_abi()
            || function.call_contract(inst).is_some()
        {
            continue;
        }
        let tail = if let Some(return_pointer) = expected
            && args
                .first()
                .is_some_and(|&value| function.resolve(value) == return_pointer)
            && let [.., call, ret] = function.block_insts(block)
            && *call == inst
            && matches!(function.inst_data(*ret), InstData::Return { args } if args.is_empty())
        {
            Some(*ret)
        } else {
            None
        };
        sites.push((inst, args.clone(), private, tail));
    }
    for (inst, args, private, tail) in sites {
        let callee = function.import_function(&private.name, private.signature.clone());
        if let Some(ret) = tail {
            function.replace_inst(ret, InstData::ReturnCall { func: callee, args });
            function.remove_inst(inst);
        } else {
            function.replace_inst(inst, InstData::Call { func: callee, args });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_tail_keeps_public_abi_and_borrows_no_caller_storage() {
        let mut module = keel_ir::parse(
            "decl @observe(ptr)
             func @result(ptr sret {i64,i64,i64},i64) nounwind local {
             entry(v0:ptr,v1:i64):
              store v1,v0
              return
             }
             func @tail(ptr sret {i64,i64,i64},i64) nounwind noninterposable {
             entry(v0:ptr,v1:i64):
              call @result(v0,v1)
              return
             }
             func @address(ptr sret {i64,i64,i64},i64) nounwind noninterposable {
              ss0 = stack_slot 8, align 8
             entry(v0:ptr,v1:i64):
              v2 = stack_addr ss0
              call @observe(v2)
              call @result(v0,v1)
              return
             }",
        )
        .unwrap();
        let original = module.functions[1].sig.clone();

        run(&mut module, &[0]).unwrap();
        keel_ir::verify_module(&module).unwrap();

        let body = |name| module.functions.iter().find(|f| f.name == name).unwrap();
        for (name, private) in [
            ("tail", "__flux_private_result_1"),
            ("address", "__flux_private_result_2"),
        ] {
            assert_eq!(body(name).sig, original);
            assert!(body(private).local);
            assert_eq!(body(private).sig.cc, CallConv::Fast);
            assert_eq!(body(private).sig.params[0].purpose, Purpose::Normal);
            assert_eq!(
                body(private).layout_insts().any(|(_, inst)| {
                    matches!(body(private).inst_data(inst), InstData::ReturnCall { .. })
                }),
                name == "tail"
            );
        }
    }
}
