//! Execute cleanup MIR and compare its observable drop order with native Rust.
use flux_hir::flib::{Decode, Decoder, Encode, Encoder};
use flux_hir::{FunctionId, Hir};
use flux_mir::{
    AggregateKind, Body, Constant, Operand, PanicStrategy, Place, ProjectionElem, Rvalue,
    StatementKind, TerminatorKind, UnwindAction,
};
use flux_ty::{Signatures, Ty, TyKind};
use std::process::Command;

const SOURCE: &str = r#"
struct Token(i32);
impl Drop for Token {
    fn drop(&mut self) {
        println!("{}", self.0);
        if self.0 == 7 { panic!("drop"); }
    }
}
fn trip() { panic!("trip"); }
fn make() -> Token { panic!("no result"); }
fn take(_value: Token) {}
fn consume(_value: Token) { trip(); }
fn normal() { let _a = Token(1); { let _b = Token(2); } let _c = Token(3); }
fn construction() { let _a = Token(2); let _p = (Token(1), make()); }
fn loop_exit() { let _a = Token(1); loop { let _b = Token(2); break; } trip(); }
fn division(d: i32) { let _a = Token(1); let _x = 1 / d; }
fn conditional_partial(flag: bool) {
    let p; if flag { p = (Token(7), Token(2), Token(3)); take(p.2); }
}
enum Pair { Values(Token,Token,Token), Empty }
fn enum_partial(flag: bool) {
    let p; if flag { p=Pair::Values(Token(7),Token(2),Token(3)); if let Pair::Values(_,_,x)=p { take(x); } }
}
fn array_partial(flag: bool) {
    let p; if flag { p = [Token(7),Token(2),Token(3)]; let [_,_,x]=p; take(x); }
}
fn partial_double_panic() { let _a=Token(1);let p=(Token(7),Token(7),Token(3));take(p.2); }
fn overwrite_panic() { let _a=Token(1);let mut b=Token(7);b=Token(2); }
fn receives(_:Token,_:Token) {}
fn arguments() { receives(Token(1),make()); }
fn direct() { let _a = Token(1); let _b = Token(2); trip(); }
fn moved() { let _a = Token(1); let b = Token(2); consume(b); }
fn no_result() { let _a = Token(1); let _b = make(); }
fn conditional(flag: bool) { let _a = Token(1); let _b; if flag { _b = Token(2); } trip(); }
fn partial() { let _a = Token(1); let p = (Token(2), Token(3)); take(p.0); trip(); }
fn overwritten() { let mut a = Token(1); a = Token(2); trip(); }
fn drop_panic() { let _a = Token(1); let _b = Token(7); }
fn partial_drop_panic() {
    let _a = Token(1); let p = (Token(7), Token(2), Token(3)); take(p.2);
}
fn return_owned() -> Token { let _a = Token(1); let _b = Token(7); Token(3) }
fn indirect() { let _a = Token(1); let call: fn() = trip; call(); }
fn double_panic() { let _a = Token(7); let _b = Token(7); }
"#;

#[derive(Debug, Clone)]
enum Value {
    Scalar(u128),
    Fields(Vec<Option<Value>>),
    Variant(u32, Vec<Option<Value>>),
    Function(FunctionId),
}
fn slot<'a>(locals: &'a mut [Option<Value>], place: &Place) -> &'a mut Option<Value> {
    let mut result = &mut locals[place.local.index()];
    for projection in place.projection.iter() {
        if let ProjectionElem::Downcast(variant) = projection {
            assert!(matches!(result,Some(Value::Variant(actual,_)) if actual==variant));
            continue;
        }
        let field = match projection {
            ProjectionElem::Field(field) => *field,
            ProjectionElem::ConstantIndex {
                offset,
                from_end: false,
                ..
            } => *offset,
            _ => panic!("unsupported projection {projection:?}"),
        };
        let Some(Value::Fields(fields) | Value::Variant(_, fields)) = result else {
            panic!("field of an uninitialized aggregate: {place:?}");
        };
        result = &mut fields[field as usize];
    }
    result
}
fn operand(locals: &mut [Option<Value>], op: &Operand) -> Value {
    match op {
        Operand::Copy(place) => slot(locals, place).clone().expect("copy initialized value"),
        Operand::Move(place) => slot(locals, place).take().expect("move initialized value"),
        Operand::Const(Constant::Int { bits, .. }) => Value::Scalar(*bits),
        Operand::Const(Constant::Bool(value)) => Value::Scalar(u128::from(*value)),
        Operand::Const(Constant::Unit) => Value::Fields(Vec::new()),
        Operand::Const(Constant::Fn { function, .. }) => Value::Function(*function),
        Operand::Const(Constant::FnItem(ty)) => {
            let TyKind::FnDef(instance) = ty.kind() else {
                panic!("function item type");
            };
            Value::Function(instance.function)
        }
        other => panic!("unsupported operand {other:?}"),
    }
}
fn drop_value(value: Value, ty: Ty, sigs: &Signatures, events: &mut Vec<u128>) -> bool {
    match ty.kind() {
        TyKind::Adt(_, _) if matches!(value, Value::Variant(..)) => {
            let Value::Variant(variant, fields) = value else {
                unreachable!()
            };
            let types = flux_solve::variants(sigs, ty).unwrap();
            for (field, &ty) in fields.into_iter().zip(types[variant as usize]) {
                if let Some(field) = field {
                    assert!(
                        !drop_value(field, ty, sigs, events),
                        "panicking enum fields must be separate MIR drops"
                    );
                }
            }
            false
        }
        TyKind::Adt(_, _) => {
            let Value::Fields(mut fields) = value else {
                panic!("token representation");
            };
            let Some(Value::Scalar(id)) = fields[0].take() else {
                panic!("token id");
            };
            events.push(id);
            id == 7
        }
        TyKind::Array(_, _) => {
            let TyKind::Array(elem, _) = ty.kind() else {
                unreachable!()
            };
            let Value::Fields(fields) = value else {
                panic!("array representation")
            };
            for field in fields.into_iter().flatten() {
                assert!(!drop_value(field, elem, sigs, events));
            }
            false
        }
        TyKind::Tuple(types) => {
            let Value::Fields(fields) = value else {
                panic!("tuple representation");
            };
            for (field, &ty) in fields.into_iter().zip(types) {
                if let Some(field) = field {
                    assert!(
                        !drop_value(field, ty, sigs, events),
                        "test must expose panicking fields as individual MIR drops"
                    );
                }
            }
            false
        }
        _ => false,
    }
}
fn execute(body: &Body, hir: &Hir, sigs: &Signatures, flag: bool) -> (Vec<u128>, &'static str) {
    let mut locals = vec![None; body.locals.len()];
    if body.param_count == 1 {
        locals[1] = Some(Value::Scalar(u128::from(flag)));
    }
    let mut events = Vec::new();
    let mut block = Body::entry();
    for _ in 0..10000 {
        let data = &body.blocks[block];
        for statement in &data.statements {
            match &statement.kind {
                StatementKind::Assign(place, value) => {
                    let value = match value {
                        Rvalue::Use(value) | Rvalue::Cast(value, _) => operand(&mut locals, value),
                        Rvalue::Aggregate(
                            AggregateKind::Struct(_)
                            | AggregateKind::Tuple
                            | AggregateKind::Array(_),
                            fields,
                        ) => Value::Fields(
                            fields
                                .iter()
                                .map(|op| Some(operand(&mut locals, op)))
                                .collect(),
                        ),
                        Rvalue::Aggregate(AggregateKind::Variant(_, variant), fields) => {
                            Value::Variant(
                                *variant,
                                fields
                                    .iter()
                                    .map(|op| Some(operand(&mut locals, op)))
                                    .collect(),
                            )
                        }
                        Rvalue::Discriminant(place) | Rvalue::DiscriminantForDrop(place) => {
                            let Some(Value::Variant(tag, _)) = slot(&mut locals, place) else {
                                panic!("initialized enum tag");
                            };
                            Value::Scalar(u128::from(*tag))
                        }
                        Rvalue::BinaryOp(op, lhs, rhs) => {
                            let Value::Scalar(lhs) = operand(&mut locals, lhs) else {
                                panic!("scalar lhs")
                            };
                            let Value::Scalar(rhs) = operand(&mut locals, rhs) else {
                                panic!("scalar rhs")
                            };
                            Value::Scalar(u128::from(match op {
                                flux_mir::BinOp::Eq => lhs == rhs,
                                flux_mir::BinOp::Ne => lhs != rhs,
                                other => panic!("unsupported op {other:?}"),
                            }))
                        }
                        other => panic!("unsupported rvalue {other:?}"),
                    };
                    *slot(&mut locals, place) = Some(value);
                }
                StatementKind::StorageDead(local) => {
                    locals[local.index()] = None;
                }
                StatementKind::PlaceMention(_) => {}
            }
        }
        let mut panicked = false;
        let next = match &data.terminator.kind {
            TerminatorKind::Goto(target) => Some(*target),
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
                ..
            } => {
                let Value::Scalar(value) = operand(&mut locals, discr) else {
                    panic!("switch scalar");
                };
                Some(
                    targets
                        .iter()
                        .find(|(bits, _)| *bits == value)
                        .map(|(_, target)| *target)
                        .unwrap_or(*otherwise),
                )
            }
            TerminatorKind::Call {
                func,
                args,
                dest,
                target,
                ..
            } => {
                let args: Vec<_> = args.iter().map(|op| operand(&mut locals, op)).collect();
                let name = hir.items.functions[*func].name.to_string();
                match name.as_str() {
                    "trip" | "make" => panicked = true,
                    "take" | "consume" => {
                        let arg_ty = sigs[*func].params[0];
                        for arg in args {
                            panicked |= drop_value(arg, arg_ty, sigs, &mut events);
                        }
                        panicked |= name == "consume";
                    }
                    _ => panic!("unsupported call {name}"),
                }
                if !panicked {
                    *slot(&mut locals, dest) = Some(Value::Fields(Vec::new()));
                }
                *target
            }
            TerminatorKind::CallIndirect {
                callee,
                args,
                target,
                ..
            } => {
                assert!(args.is_empty());
                let Value::Function(function) = operand(&mut locals, callee) else {
                    panic!("function pointer");
                };
                assert_eq!(hir.items.functions[function].name.to_string(), "trip");
                panicked = true;
                *target
            }
            TerminatorKind::Drop { place, target } => {
                let value = slot(&mut locals, place)
                    .take()
                    .expect("drop initialized value exactly once");
                panicked = drop_value(value, body.place_ty(place, sigs), sigs, &mut events);
                Some(*target)
            }
            TerminatorKind::Assert {
                cond,
                expected,
                target,
                ..
            } => {
                let Value::Scalar(value) = operand(&mut locals, cond) else {
                    panic!("assert bool")
                };
                panicked = (value != 0) != *expected;
                Some(*target)
            }
            TerminatorKind::Return => return (events, "returned"),
            TerminatorKind::UnwindResume => return (events, "caught"),
            other => panic!("unsupported terminator {other:?}"),
        };
        block = if panicked {
            match data.terminator.unwind {
                UnwindAction::Cleanup(target) => target,
                UnwindAction::Continue => return (events, "caught"),
                UnwindAction::Terminate => return (events, "terminated"),
                UnwindAction::Unreachable => panic!("panic lost its exceptional successor"),
            }
        } else {
            next.expect("non-divergent operation returns")
        };
    }
    panic!("cleanup graph failed to terminate")
}

#[test]
fn cleanup_ladders_match_rustc_drop_order() {
    let root = std::env::temp_dir().join(format!("flux-unwind-cleanup-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let driver = r#"
fn main() {
    let mode=std::env::args().nth(1).unwrap();
    let result=std::panic::catch_unwind(||match mode.as_str() {
        "normal"=>normal(), "construction"=>construction(), "loop_exit"=>loop_exit(), "division"=>division(0),
        "array_partial_true"=>array_partial(true), "array_partial_false"=>array_partial(false),
        "partial_double_panic"=>partial_double_panic(), "overwrite_panic"=>overwrite_panic(), "arguments"=>arguments(),
        "direct"=>direct(), "moved"=>moved(), "no_result"=>no_result(),
        "conditional_true"=>conditional(true), "conditional_false"=>conditional(false),
        "conditional_partial_true"=>conditional_partial(true), "conditional_partial_false"=>conditional_partial(false),
        "enum_partial_true"=>enum_partial(true), "enum_partial_false"=>enum_partial(false),
        "partial"=>partial(), "overwritten"=>overwritten(), "drop_panic"=>drop_panic(),
        "partial_drop_panic"=>partial_drop_panic(), "return_owned"=>drop(return_owned()),
        "indirect"=>indirect(), "double_panic"=>double_panic(), _=>panic!("unknown mode")
    });
    println!("{}",if result.is_err(){"caught"}else{"returned"});
}
"#;
    let path = root.join("oracle.rs");
    let binary = root.join("oracle");
    std::fs::write(&path, format!("{SOURCE}\n{driver}")).unwrap();
    let compiled = Command::new("rustc")
        .args(["--edition=2024", "-Cpanic=unwind", "-o"])
        .arg(&binary)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text("unwind_cleanup.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    for mode in [
        "normal",
        "construction",
        "loop_exit",
        "division",
        "array_partial_true",
        "array_partial_false",
        "partial_double_panic",
        "overwrite_panic",
        "arguments",
        "direct",
        "moved",
        "no_result",
        "conditional_true",
        "conditional_false",
        "conditional_partial_true",
        "conditional_partial_false",
        "enum_partial_true",
        "enum_partial_false",
        "partial",
        "overwritten",
        "drop_panic",
        "partial_drop_panic",
        "return_owned",
        "indirect",
        "double_panic",
    ] {
        let name = mode
            .strip_suffix("_true")
            .or_else(|| mode.strip_suffix("_false"))
            .unwrap_or(mode);
        let (id, _) = hir
            .items
            .functions
            .iter()
            .find(|(id, f)| hir.is_program(*id) && f.name.to_string() == name)
            .unwrap();
        let body = flux_mir::build_with_panic_strategy(&hir, &typed, id, PanicStrategy::Unwind);
        assert!(
            body.init_errors.is_empty(),
            "{mode}: {:?}",
            body.init_errors
        );
        let errors = flux_mir::validate::validate(&hir.items, &typed.sigs, &[], &body);
        assert!(errors.is_empty(), "{mode}: {errors:?}");
        let (events, outcome) = execute(&body, &hir, &typed.sigs, mode.ends_with("_true"));
        let mut e = Encoder::new();
        body.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        let round_trip = Body::decode(&mut d).unwrap();
        d.finish().unwrap();
        assert_eq!(
            execute(&round_trip, &hir, &typed.sigs, mode.ends_with("_true")),
            (events.clone(), outcome),
            "{mode}: metadata preserves cleanup"
        );
        let expected = Command::new("/bin/sh")
            .args(["-c", "ulimit -c 0; exec \"$@\"", "unwind-oracle"])
            .arg(&binary)
            .arg(mode)
            .output()
            .unwrap();
        let expected_text = String::from_utf8(expected.stdout).unwrap();
        let mut actual = events.iter().map(|v| format!("{v}\n")).collect::<String>();
        if outcome != "terminated" {
            actual.push_str(outcome);
            actual.push('\n');
        }
        assert_eq!(
            actual,
            expected_text,
            "{mode}: stderr {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        assert_eq!(expected.status.success(), outcome != "terminated", "{mode}");
        let abort = flux_mir::build(&hir, &typed, id);
        let explicit_abort =
            flux_mir::build_with_panic_strategy(&hir, &typed, id, PanicStrategy::Abort);
        assert!(!abort.has_unwind());
        assert_eq!(
            abort.blocks.values().collect::<Vec<_>>(),
            explicit_abort.blocks.values().collect::<Vec<_>>()
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
