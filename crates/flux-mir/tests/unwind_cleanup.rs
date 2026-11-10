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
struct Outer { id:i32, a:Token, b:Token }
impl Drop for Outer {
    fn drop(&mut self) { println!("{}",self.id); if self.id==7 {panic!("outer");} }
}
fn outer_panic() { let _a=Token(1);let _x=Outer{id:7,a:Token(2),b:Token(3)}; }
fn outer_field_panic() { let _a=Token(1);let _x=Outer{id:4,a:Token(7),b:Token(3)}; }
fn outer_double() { let _a=Token(1);let _x=Outer{id:7,a:Token(7),b:Token(3)}; }
fn tuple_whole() { let _a=Token(1);let _x=(Token(7),Token(2),Token(3)); }
fn array_whole() { let _a=Token(1);let _x=[Token(2),Token(7),Token(3)]; }
fn nested_arrays() { let _a=Token(1);let _x=[[Token(2),Token(7)],[Token(3),Token(4)]]; }
fn array_fields() { let _a=Token(1);let _x=[Outer{id:4,a:Token(7),b:Token(3)},Outer{id:5,a:Token(2),b:Token(6)}]; }
fn array_outer() { let _a=Token(1);let _x=[Outer{id:7,a:Token(2),b:Token(3)},Outer{id:5,a:Token(4),b:Token(6)}]; }
fn array_double() { let _a=Token(1);let _x=[Token(7),Token(7),Token(3)]; }
enum Tagged { Two(Token,Token), Empty }
impl Drop for Tagged { fn drop(&mut self){println!("40");panic!("enum");} }
fn enum_outer() {let _a=Token(1);let _x=Tagged::Two(Token(2),Token(3));}
enum SafeTagged { Two(Token,Token), Empty }
impl Drop for SafeTagged { fn drop(&mut self){println!("41");} }
fn enum_field() {let _a=Token(1);let _x=SafeTagged::Two(Token(7),Token(3));}
fn empty_enum() {let _a=Token(1);let _x=SafeTagged::Empty;}
#[repr(packed)] struct Packed(Token,Token);
#[repr(packed)] struct PackedOuter(Outer,Token);
fn packed() {let _a=Token(1);let _x=Packed(Token(7),Token(3));}
fn nested_packed() {let _a=Token(1);let _x=PackedOuter(Outer{id:7,a:Token(2),b:Token(3)},Token(4));}
fn packed_double() {let _a=Token(1);let _x=Packed(Token(7),Token(7));}
fn cleanup_whole() {let _a=Token(1);let _x=Outer{id:4,a:Token(2),b:Token(3)};trip();}
fn cleanup_array() {let _a=Token(1);let _x=[Token(2),Token(7),Token(3)];trip();}
fn zero_array() {let _a=Token(1);let _x:[Token;0]=[];trip();}

fn cleanup_outer_panic() {let _a=Token(1);let _x=Outer{id:7,a:Token(2),b:Token(3)};trip();}
fn cleanup_nested_value() {let _a=Token(1);let _x=Outer{id:4,a:Token(7),b:Token(3)};trip();}
struct Container{id:i32,first:Outer,last:Token}
impl Drop for Container {fn drop(&mut self){println!("{}",self.id);if self.id==7{panic!("container");}}}
fn nested_double() {let _a=Token(1);let _x=Container{id:7,first:Outer{id:7,a:Token(2),b:Token(3)},last:Token(4)};}
fn cleanup_packed() {let _a=Token(1);let _x=PackedOuter(Outer{id:7,a:Token(2),b:Token(3)},Token(4));trip();}
fn slice_drop(p:&mut[Token]) {unsafe{core::ptr::drop_in_place(p);}}
fn slice_empty(p:&mut[Token]) {unsafe{core::ptr::drop_in_place(p);}}
fn slice_outer(p:&mut[Outer]) {unsafe{core::ptr::drop_in_place(p);}}

fn generic_no_drop<T>(_value:T) {}
fn generic_outer<T>(_value:T) {}
fn outer_identity(_value:Outer) {}

"#;

#[derive(Debug, Clone)]
enum Value {
    Scalar(u128),
    Fields(Vec<Option<Value>>),
    Variant(u32, Vec<Option<Value>>),
    Function(FunctionId),
    Pointer(Place),
}
fn resolve(locals: &mut [Option<Value>], place: &Place) -> Place {
    let mut result = Place::from(place.local);
    for &projection in place.projection.iter() {
        match projection {
            ProjectionElem::Deref => {
                let Some(Value::Pointer(pointer)) = slot(locals, &result) else {
                    panic!("valid pointer")
                };
                result = pointer.clone();
            }
            ProjectionElem::Index(index) => {
                let Some(Value::Scalar(index)) = &locals[index.index()] else {
                    panic!("array index")
                };
                result = result.project(ProjectionElem::ConstantIndex {
                    offset: *index as u32,
                    min_length: 0,
                    from_end: false,
                });
            }
            other => result = result.project(other),
        }
    }
    result
}
fn slot<'a>(locals: &'a mut [Option<Value>], place: &Place) -> &'a mut Option<Value> {
    let resolved = resolve(locals, place);
    let place = &resolved;
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
    if let Some(owner) = body.owner {
        let name = hir.items.functions[owner].name.to_string();
        if name == "generic_outer" {
            locals[1] = Some(Value::Fields(vec![
                Some(Value::Scalar(7)),
                Some(Value::Fields(vec![Some(Value::Scalar(2))])),
                Some(Value::Fields(vec![Some(Value::Scalar(3))])),
            ]));
        }
        if name.starts_with("slice_") {
            let token = |id| Some(Value::Fields(vec![Some(Value::Scalar(id))]));
            let fields = match name.as_str() {
                "slice_empty" => Vec::new(),
                "slice_drop" => vec![token(2), token(7), token(3)],
                "slice_outer" => vec![
                    Some(Value::Fields(vec![
                        Some(Value::Scalar(4)),
                        token(7),
                        token(3),
                    ])),
                    Some(Value::Fields(vec![
                        Some(Value::Scalar(5)),
                        token(2),
                        token(6),
                    ])),
                ],
                _ => unreachable!(),
            };
            locals[1] = Some(Value::Pointer(Place::from(flux_mir::Local::from_raw(
                locals.len() as u32,
            ))));
            locals.push(Some(Value::Fields(fields)));
        }
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
                        Rvalue::Ref { place, .. } => Value::Pointer(resolve(&mut locals, place)),
                        Rvalue::Len { place, .. } => {
                            let Some(Value::Fields(fields)) = slot(&mut locals, place) else {
                                panic!("array/slice length")
                            };
                            Value::Scalar(fields.len() as u128)
                        }
                        Rvalue::BinaryOp(op, lhs, rhs) => {
                            let Value::Scalar(lhs) = operand(&mut locals, lhs) else {
                                panic!("scalar lhs")
                            };
                            let Value::Scalar(rhs) = operand(&mut locals, rhs) else {
                                panic!("scalar rhs")
                            };
                            Value::Scalar(match op {
                                flux_mir::BinOp::Eq => u128::from(lhs == rhs),
                                flux_mir::BinOp::Ne => u128::from(lhs != rhs),
                                flux_mir::BinOp::Lt => u128::from(lhs < rhs),
                                flux_mir::BinOp::Add => lhs + rhs,
                                other => panic!("unsupported op {other:?}"),
                            })
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
                generic_args,
            } => {
                let args: Vec<_> = args.iter().map(|op| operand(&mut locals, op)).collect();
                let name = hir.items.functions[*func].name.to_string();
                match name.as_str() {
                    "trip" | "make" => panicked = true,
                    "drop" => {
                        let [Value::Pointer(place)] = args.as_slice() else {
                            panic!("drop receiver")
                        };
                        let TyKind::Adt(adt, _) = generic_args[0].kind() else {
                            panic!("user destructor type")
                        };
                        let name = hir.items.structs[adt.strukt].name.to_string();
                        let id = match name.as_str() {
                            "Tagged" => 40,
                            "SafeTagged" => 41,
                            "Token" | "Outer" | "Container" => {
                                let Some(Value::Fields(fields)) = slot(&mut locals, place) else {
                                    panic!("destructor receiver")
                                };
                                let Some(Value::Scalar(id)) = fields[0] else {
                                    panic!("destructor event id")
                                };
                                id
                            }
                            _ => panic!("unsupported destructor {name}"),
                        };
                        events.push(id);
                        panicked = id == 7 || name == "Tagged";
                    }
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
            TerminatorKind::UnwindTerminate => return (events, "terminated"),
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

fn compare_with_rustc(expanded: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-unwind-cleanup-{}-{expanded}",
        std::process::id()
    ));
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
        "indirect"=>indirect(), "double_panic"=>double_panic(),
        "outer_panic"=>outer_panic(), "outer_field_panic"=>outer_field_panic(), "outer_double"=>outer_double(), "tuple_whole"=>tuple_whole(), "array_whole"=>array_whole(), "nested_arrays"=>nested_arrays(), "array_fields"=>array_fields(), "array_outer"=>array_outer(), "array_double"=>array_double(), "enum_outer"=>enum_outer(), "enum_field"=>enum_field(), "empty_enum"=>empty_enum(), "packed"=>packed(), "nested_packed"=>nested_packed(), "packed_double"=>packed_double(), "cleanup_whole"=>cleanup_whole(), "cleanup_array"=>cleanup_array(), "zero_array"=>zero_array(), "cleanup_outer_panic"=>cleanup_outer_panic(), "cleanup_nested_value"=>cleanup_nested_value(), "nested_double"=>nested_double(), "cleanup_packed"=>cleanup_packed(),
        "generic_no_drop"=>generic_no_drop(3i32), "generic_outer"=>generic_outer(Outer{id:7,a:Token(2),b:Token(3)}),
        "slice_drop"=>{let mut values=std::mem::ManuallyDrop::new([Token(2),Token(7),Token(3)]);slice_drop(&mut values[..]);},
        "slice_empty"=>{let mut values=std::mem::ManuallyDrop::new([]);slice_empty(&mut values[..]);},
        "slice_outer"=>{let mut values=std::mem::ManuallyDrop::new([Outer{id:4,a:Token(7),b:Token(3)},Outer{id:5,a:Token(2),b:Token(6)}]);slice_outer(&mut values[..]);}, _=>panic!("unknown mode")
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
    let mut modes = vec![
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
    ];
    if expanded {
        modes.extend([
            "generic_no_drop",
            "generic_outer",
            "cleanup_outer_panic",
            "cleanup_nested_value",
            "nested_double",
            "cleanup_packed",
            "slice_drop",
            "slice_empty",
            "slice_outer",
            "outer_panic",
            "outer_field_panic",
            "outer_double",
            "tuple_whole",
            "array_whole",
            "nested_arrays",
            "array_fields",
            "array_outer",
            "array_double",
            "enum_outer",
            "enum_field",
            "empty_enum",
            "packed",
            "nested_packed",
            "packed_double",
            "cleanup_whole",
            "cleanup_array",
            "zero_array",
        ]);
    }
    for mode in modes {
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
        let mut body = flux_mir::build_with_panic_strategy(&hir, &typed, id, PanicStrategy::Unwind);
        if expanded {
            if mode.starts_with("generic_") {
                let ty = if mode == "generic_no_drop" {
                    Ty::int(flux_ty::IntTy::I32)
                } else {
                    let id = hir
                        .items
                        .functions
                        .iter()
                        .find(|(_, f)| f.name.as_str() == "outer_identity")
                        .unwrap()
                        .0;
                    typed.sigs[id].params[0]
                };
                body = flux_mir::mono::subst_body(&hir, &typed.sigs, &body, Ty::list(&[ty]));
            }
            if mode.starts_with("slice_") {
                // Isolate intrinsic glue from the public ptr wrapper. General
                // function instantiation with unwinding is not enabled yet.
                let intrinsic = hir
                    .items
                    .functions
                    .iter()
                    .find(|(_, f)| f.intrinsic.is_some_and(|i| i.name() == "drop_in_place"))
                    .unwrap()
                    .0;
                for block in body.blocks.values_mut() {
                    if let TerminatorKind::Call { func, .. } = &mut block.terminator.kind
                        && hir.items.functions[*func].name.as_str() == "drop_in_place"
                    {
                        *func = intrinsic;
                    }
                }
            }
            flux_mir::mono::expand_drops(&hir.items, &typed.sigs, &mut body);
            assert!(
                !body
                    .blocks
                    .values()
                    .any(|block| matches!(block.terminator.kind, TerminatorKind::Drop { .. })),
                "{mode}: all drops expanded"
            );
        }
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

#[test]
fn cleanup_ladders_match_rustc_drop_order() {
    compare_with_rustc(false);
}

#[test]
fn expanded_drop_glue_matches_rustc_drop_order() {
    compare_with_rustc(true);
}
