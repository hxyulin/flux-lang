//! Constant evaluation: running a constant's MIR at compile time.
//!
//! A constant's value is computed by interpreting the MIR of its
//! initializer, and of any `const fn` it calls, so every expression form
//! works the same at compile time as at run time. Unlike run time,
//! arithmetic that overflows and division by zero are errors (E0080),
//! with rustc's messages: "attempt to compute `i32::MAX + 1_i32`, which
//! would overflow". An error inside a call is reported at the call in the
//! constant.

use std::collections::HashMap;

use flux_arena::ArenaMap;
use flux_hir::{ConstId, FunctionId};
use flux_syntax::TextRange;
use flux_ty::IntTy;

use crate::{
    AssertMessage, BinOp, Body, Constant, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind, UnOp,
};

/// A value computed at compile time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Two's complement bits, truncated to the type's width.
    Int {
        bits: u128,
        ty: IntTy,
    },
    Bool(bool),
    /// A tuple or struct, fields in order; `()` has none.
    Aggregate(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    pub range: TextRange,
    pub message: String,
    /// The error is another constant's, which reports it itself.
    pub nested: bool,
}

/// How many statements a constant may run before evaluation gives up;
/// rustc allows loops of millions of iterations.
const STEP_LIMIT: u64 = 50_000_000;
/// How deep calls may nest: rustc allows 128 frames, the constant's own
/// included.
const DEPTH_LIMIT: u32 = 127;

/// Evaluates constants on demand, each once.
pub struct Evaluator<'a> {
    pub functions: &'a ArenaMap<flux_hir::Function, Body>,
    pub consts: &'a ArenaMap<flux_hir::Const, Body>,
    /// Each constant's name, for messages.
    pub names: &'a dyn Fn(ConstId) -> String,
    values: HashMap<ConstId, Result<Value, EvalError>>,
    in_progress: Vec<ConstId>,
    steps: u64,
}

impl<'a> Evaluator<'a> {
    pub fn new(
        functions: &'a ArenaMap<flux_hir::Function, Body>,
        consts: &'a ArenaMap<flux_hir::Const, Body>,
        names: &'a dyn Fn(ConstId) -> String,
    ) -> Evaluator<'a> {
        Evaluator {
            functions,
            consts,
            names,
            values: HashMap::new(),
            in_progress: Vec::new(),
            steps: 0,
        }
    }

    /// The value of constant `id`, evaluated if it was not already.
    pub fn eval_const(&mut self, id: ConstId) -> Result<Value, EvalError> {
        if let Some(result) = self.values.get(&id) {
            return result.clone();
        }
        let Some(body) = self.consts.get(id) else {
            return Err(error(
                TextRange::default(),
                "constant without a value".to_owned(),
            ));
        };
        if self.in_progress.contains(&id) {
            let message = format!(
                "cycle detected when const-evaluating `{}`",
                (self.names)(id)
            );
            return Err(error(TextRange::default(), message));
        }
        self.in_progress.push(id);
        self.steps = 0;
        // A constant's own arithmetic is always checked.
        let result = self.run(body, Vec::new(), 0, true);
        self.in_progress.pop();
        self.values.insert(id, result.clone());
        result
    }

    /// Runs `body`; `checked` if overflow there is an error.
    fn run(
        &mut self,
        body: &Body,
        args: Vec<Value>,
        depth: u32,
        checked: bool,
    ) -> Result<Value, EvalError> {
        let mut locals: Vec<Option<Value>> = vec![None; body.locals.len()];
        for (param, value) in body.params().zip(args) {
            locals[param.index()] = Some(value);
        }
        let mut block = Body::entry();
        loop {
            let data = &body.blocks[block];
            for statement in &data.statements {
                self.steps += 1;
                if self.steps > STEP_LIMIT {
                    return Err(error(
                        statement.range,
                        "constant evaluation is taking a long time".to_owned(),
                    ));
                }
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    let value = self.rvalue(&locals, rvalue, statement.range, !checked)?;
                    write(&mut locals, dest, value, statement.range)?;
                }
            }
            let range = data.terminator.range;
            block = match &data.terminator.kind {
                // What a constant drops runs no code: anything else is
                // E0493, reported before evaluation.
                TerminatorKind::Goto(target) | TerminatorKind::Drop { target, .. } => *target,
                TerminatorKind::SwitchInt {
                    discr,
                    targets,
                    otherwise,
                } => {
                    let value = match self.operand(&locals, discr, range)? {
                        Value::Int { bits, .. } => bits,
                        Value::Bool(b) => u128::from(b),
                        Value::Aggregate(_) => unreachable!("switches on scalars"),
                    };
                    targets
                        .iter()
                        .find(|&&(v, _)| v == value)
                        .map_or(*otherwise, |&(_, target)| target)
                }
                TerminatorKind::Call {
                    func,
                    args,
                    dest,
                    target,
                    ..
                } => {
                    let args = args
                        .iter()
                        .map(|a| self.operand(&locals, a, range))
                        .collect::<Result<Vec<_>, _>>()?;
                    let value = self.call(*func, args, depth, range, checked)?;
                    write(&mut locals, dest, value, range)?;
                    match target {
                        Some(target) => *target,
                        None => return Err(error(range, "entering unreachable code".to_owned())),
                    }
                }
                TerminatorKind::Assert {
                    cond,
                    expected,
                    msg,
                    target,
                } => {
                    let Value::Bool(value) = self.operand(&locals, cond, range)? else {
                        unreachable!("assertions test booleans")
                    };
                    if value != *expected {
                        let message = self.assert_message(&locals, msg, range)?;
                        return Err(error(range, message));
                    }
                    *target
                }
                TerminatorKind::Return => {
                    return locals[Body::RETURN.index()]
                        .clone()
                        .ok_or_else(|| error(range, "constant has no value".to_owned()));
                }
                TerminatorKind::Unreachable => {
                    return Err(error(range, "entering unreachable code".to_owned()));
                }
            };
        }
    }

    /// Calls `func`, a `const fn`; errors inside it are reported at the
    /// call.
    fn call(
        &mut self,
        func: FunctionId,
        args: Vec<Value>,
        depth: u32,
        range: TextRange,
        checked: bool,
    ) -> Result<Value, EvalError> {
        let Some(callee) = self.functions.get(func) else {
            return Err(error(
                range,
                "calling this function in constants is not supported yet".to_owned(),
            ));
        };
        if depth >= DEPTH_LIMIT {
            return Err(error(
                range,
                "reached the configured maximum number of stack frames".to_owned(),
            ));
        }
        // A function's own arithmetic follows `-C overflow-checks`, off in
        // flux, unless it inherits its caller's.
        let checked = checked && callee.inherits_overflow_checks;
        self.run(callee, args, depth + 1, checked)
            .map_err(|e| EvalError { range, ..e })
    }

    fn assert_message(
        &mut self,
        locals: &[Option<Value>],
        msg: &AssertMessage,
        range: TextRange,
    ) -> Result<String, EvalError> {
        let mut show = |op: &Operand| -> Result<String, EvalError> {
            Ok(show(&self.operand(locals, op, range)?))
        };
        Ok(match msg {
            AssertMessage::DivisionByZero(l) => format!("attempt to divide `{}` by zero", show(l)?),
            AssertMessage::RemainderByZero(l) => format!(
                "attempt to calculate the remainder of `{}` with a divisor of zero",
                show(l)?
            ),
            AssertMessage::DivisionOverflow(l, r) => {
                format!(
                    "attempt to compute `{} / {}`, which would overflow",
                    show(l)?,
                    show(r)?
                )
            }
            AssertMessage::RemainderOverflow(l, r) => {
                format!(
                    "attempt to compute `{} % {}`, which would overflow",
                    show(l)?,
                    show(r)?
                )
            }
        })
    }

    fn operand(
        &mut self,
        locals: &[Option<Value>],
        operand: &Operand,
        range: TextRange,
    ) -> Result<Value, EvalError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => read(locals, place, range),
            Operand::Const(constant) => match *constant {
                Constant::Int { bits, ty } => Ok(Value::Int { bits, ty }),
                Constant::Bool(b) => Ok(Value::Bool(b)),
                Constant::Unit => Ok(Value::Aggregate(Vec::new())),
                Constant::CStr(_) => Err(error(
                    range,
                    "C strings in constants are not supported yet".to_owned(),
                )),
                Constant::Item { id, .. } => {
                    let steps = self.steps;
                    // A cycle is reported at every constant in it, as rustc does.
                    let result = self.eval_const(id).map_err(|e| EvalError {
                        range,
                        nested: !e.message.starts_with("cycle detected"),
                        ..e
                    });
                    self.steps = steps;
                    result
                }
            },
        }
    }

    fn rvalue(
        &mut self,
        locals: &[Option<Value>],
        rvalue: &Rvalue,
        range: TextRange,
        wrapping: bool,
    ) -> Result<Value, EvalError> {
        match rvalue {
            Rvalue::Use(op) => self.operand(locals, op, range),
            Rvalue::Aggregate(_, ops) => Ok(Value::Aggregate(
                ops.iter()
                    .map(|op| self.operand(locals, op, range))
                    .collect::<Result<_, _>>()?,
            )),
            Rvalue::Ref { .. } => Err(error(
                range,
                "references in constants are not supported yet".to_owned(),
            )),
            Rvalue::Cast(op, to) => {
                let value = self.operand(locals, op, range)?;
                let bits = match value {
                    Value::Int { bits, ty } => sign_extend(bits, ty) as u128,
                    Value::Bool(b) => u128::from(b),
                    Value::Aggregate(_) => unreachable!("casts are of scalars"),
                };
                let to = to.as_int().expect("casts are to integers");
                Ok(Value::Int {
                    bits: truncate(bits, to),
                    ty: to,
                })
            }
            Rvalue::UnaryOp(op, operand) => match (op, self.operand(locals, operand, range)?) {
                (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                (UnOp::Not, Value::Int { bits, ty }) => Ok(Value::Int {
                    bits: truncate(!bits, ty),
                    ty,
                }),
                (UnOp::Neg, value @ Value::Int { bits, ty }) => {
                    if ty.is_signed() && bits == min_bits(ty) && !wrapping {
                        return Err(error(
                            range,
                            format!("attempt to negate `{}`, which would overflow", show(&value)),
                        ));
                    }
                    Ok(Value::Int {
                        bits: truncate(bits.wrapping_neg(), ty),
                        ty,
                    })
                }
                _ => unreachable!("type checking allows no other unary operations"),
            },
            Rvalue::BinaryOp(op, l, r) => {
                let l = self.operand(locals, l, range)?;
                let r = self.operand(locals, r, range)?;
                let result = if wrapping {
                    wrapping_binary(*op, &l, &r)
                } else {
                    binary(*op, &l, &r)
                };
                result.map_err(|message| error(range, message))
            }
        }
    }
}

fn error(range: TextRange, message: String) -> EvalError {
    EvalError {
        range,
        message,
        nested: false,
    }
}

fn read(locals: &[Option<Value>], place: &Place, range: TextRange) -> Result<Value, EvalError> {
    let mut value = locals[place.local.index()]
        .as_ref()
        .ok_or_else(|| error(range, "use of an uninitialized value".to_owned()))?;
    for elem in &place.projection {
        value = match (elem, value) {
            (ProjectionElem::Field(i), Value::Aggregate(fields)) => &fields[*i as usize],
            _ => {
                return Err(error(
                    range,
                    "references in constants are not supported yet".to_owned(),
                ));
            }
        };
    }
    Ok(value.clone())
}

fn write(
    locals: &mut [Option<Value>],
    place: &Place,
    new: Value,
    range: TextRange,
) -> Result<(), EvalError> {
    let slot = &mut locals[place.local.index()];
    if place.projection.is_empty() {
        *slot = Some(new);
        return Ok(());
    }
    let mut value = slot
        .as_mut()
        .ok_or_else(|| error(range, "use of an uninitialized value".to_owned()))?;
    for elem in &place.projection {
        value = match (elem, value) {
            (ProjectionElem::Field(i), Value::Aggregate(fields)) => &mut fields[*i as usize],
            _ => {
                return Err(error(
                    range,
                    "references in constants are not supported yet".to_owned(),
                ));
            }
        };
    }
    *value = new;
    Ok(())
}

/// `l op r`; an error message if it would overflow or divide by zero.
pub fn binary(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    let (lb, rb, ty) = match (l, r) {
        (Value::Bool(a), Value::Bool(b)) => {
            let value = match op {
                BinOp::BitAnd => *a & *b,
                BinOp::BitOr => *a | *b,
                BinOp::BitXor => *a ^ *b,
                BinOp::Eq => a == b,
                BinOp::Ne => a != b,
                BinOp::Lt => !*a & *b,
                BinOp::Le => a <= b,
                BinOp::Gt => *a & !*b,
                BinOp::Ge => a >= b,
                _ => unreachable!("type checking allows no other `bool` operations"),
            };
            return Ok(Value::Bool(value));
        }
        (Value::Aggregate(a), Value::Aggregate(b)) if a.is_empty() && b.is_empty() => {
            let equal = matches!(op, BinOp::Eq | BinOp::Le | BinOp::Ge);
            return Ok(Value::Bool(equal));
        }
        (Value::Int { bits: a, ty }, Value::Int { bits: b, .. }) => (*a, *b, *ty),
        _ => unreachable!("operands of the same type"),
    };
    let signed = ty.is_signed();
    let (a, b) = (sign_extend(lb, ty), sign_extend(rb, ty));
    let int = |value: i128| Value::Int {
        bits: truncate(value as u128, ty),
        ty,
    };
    let overflow = |sym: &str| {
        format!(
            "attempt to compute `{} {sym} {}`, which would overflow",
            show(l),
            show(r)
        )
    };
    // The exact result, to compare against the type's range.
    let fits = |exact: Option<i128>, exact_unsigned: Option<u128>| {
        if signed {
            exact.filter(|&v| v >= min_value(ty) && v <= max_value(ty))
        } else {
            exact_unsigned.filter(|&v| v <= ty.max()).map(|v| v as i128)
        }
    };
    let (ua, ub) = (lb, truncate(rb, ty));
    let value = match op {
        BinOp::Add => fits(a.checked_add(b), ua.checked_add(ub)).ok_or_else(|| overflow("+"))?,
        BinOp::Sub => fits(a.checked_sub(b), ua.checked_sub(ub)).ok_or_else(|| overflow("-"))?,
        BinOp::Mul => fits(a.checked_mul(b), ua.checked_mul(ub)).ok_or_else(|| overflow("*"))?,
        BinOp::Div | BinOp::Rem => {
            if b == 0 {
                return Err(match op {
                    BinOp::Div => format!("attempt to divide `{}` by zero", show(l)),
                    _ => format!(
                        "attempt to calculate the remainder of `{}` with a divisor of zero",
                        show(l)
                    ),
                });
            }
            if signed && a == min_value(ty) && b == -1 {
                return Err(overflow(if op == BinOp::Div { "/" } else { "%" }));
            }
            match (op, signed) {
                (BinOp::Div, true) => a / b,
                (BinOp::Div, false) => (ua / ub) as i128,
                (_, true) => a % b,
                (_, false) => (ua % ub) as i128,
            }
        }
        BinOp::Shl | BinOp::Shr => {
            // The amount may have any integer type.
            let amount = match r {
                Value::Int { bits, ty } => sign_extend(*bits, *ty),
                _ => unreachable!("shift amounts are integers"),
            };
            if amount < 0 || amount >= i128::from(ty.bits()) {
                let way = if op == BinOp::Shl { "left" } else { "right" };
                return Err(format!(
                    "attempt to shift {way} by `{}`, which would overflow",
                    show(r)
                ));
            }
            match (op, signed) {
                (BinOp::Shl, _) => (ua << amount) as i128,
                (_, true) => a >> amount,
                (_, false) => (ua >> amount) as i128,
            }
        }
        BinOp::BitAnd => (ua & ub) as i128,
        BinOp::BitOr => (ua | ub) as i128,
        BinOp::BitXor => (ua ^ ub) as i128,
        BinOp::Eq => return Ok(Value::Bool(a == b)),
        BinOp::Ne => return Ok(Value::Bool(a != b)),
        BinOp::Lt => return Ok(Value::Bool(if signed { a < b } else { ua < ub })),
        BinOp::Le => return Ok(Value::Bool(if signed { a <= b } else { ua <= ub })),
        BinOp::Gt => return Ok(Value::Bool(if signed { a > b } else { ua > ub })),
        BinOp::Ge => return Ok(Value::Bool(if signed { a >= b } else { ua >= ub })),
    };
    Ok(int(value))
}

/// `l op r` with flux's run-time semantics: arithmetic wraps and shift
/// amounts are masked; only division by zero is an error.
pub fn wrapping_binary(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    let (Value::Int { bits: a, ty }, Value::Int { bits: b, ty: rty }) = (l, r) else {
        return binary(op, l, r);
    };
    let (a, b, ty) = (*a, *b, *ty);
    let int = |bits: u128| Value::Int {
        bits: truncate(bits, ty),
        ty,
    };
    Ok(match op {
        BinOp::Add => int(a.wrapping_add(b)),
        BinOp::Sub => int(a.wrapping_sub(b)),
        BinOp::Mul => int(a.wrapping_mul(b)),
        BinOp::Shl | BinOp::Shr => {
            let amount = (truncate(b, *rty) % u128::from(ty.bits())) as u32;
            let amount_value = Value::Int {
                bits: u128::from(amount),
                ty: IntTy::U32,
            };
            return binary(op, l, &amount_value);
        }
        BinOp::Div | BinOp::Rem
            if ty.is_signed() && a == min_bits(ty) && sign_extend(b, ty) == -1 =>
        {
            // `MIN / -1` wraps to `MIN`, and `MIN % -1` is 0.
            if op == BinOp::Div { int(a) } else { int(0) }
        }
        _ => return binary(op, l, r),
    })
}

/// A value as rustc writes it in messages: `5_i32`, `-1_i8`, `i32::MAX`,
/// `u8::MAX`.
pub fn show(value: &Value) -> String {
    match *value {
        Value::Int { bits, ty } => {
            let name = ty.name();
            if bits == truncate(ty.max(), ty) {
                format!("{name}::MAX")
            } else if ty.is_signed() && bits == min_bits(ty) {
                format!("{name}::MIN")
            } else if ty.is_signed() {
                format!("{}_{name}", sign_extend(bits, ty))
            } else {
                format!("{bits}_{name}")
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Aggregate(_) => "{aggregate}".to_owned(),
    }
}

fn mask(ty: IntTy) -> u128 {
    if ty.bits() == 128 {
        u128::MAX
    } else {
        (1u128 << ty.bits()) - 1
    }
}

pub fn truncate(bits: u128, ty: IntTy) -> u128 {
    bits & mask(ty)
}

pub fn sign_extend(bits: u128, ty: IntTy) -> i128 {
    let width = ty.bits();
    if !ty.is_signed() || width == 128 {
        return bits as i128;
    }
    let shift = 128 - width;
    ((bits << shift) as i128) >> shift
}

fn min_bits(ty: IntTy) -> u128 {
    1u128 << (ty.bits() - 1)
}

fn min_value(ty: IntTy) -> i128 {
    if ty.bits() == 128 {
        i128::MIN
    } else {
        -(1i128 << (ty.bits() - 1))
    }
}

fn max_value(ty: IntTy) -> i128 {
    if ty.bits() == 128 {
        i128::MAX
    } else {
        (1i128 << (ty.bits() - 1)) - 1
    }
}
