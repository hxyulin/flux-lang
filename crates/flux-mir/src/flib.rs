//! Checked MIR templates stored in Fluxlib metadata.
use crate::eval::{StaticProjection, Value};
use crate::*;
use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error, FlibItem, IdKind};
#[path = "flib_allocations.rs"]
mod allocations;
#[path = "flib_validate.rs"]
mod boundary;

impl FlibItem for LocalDecl {
    const KIND: IdKind = IdKind::Function;
    const REMAP: bool = false;
}
impl FlibItem for BasicBlockData {
    const KIND: IdKind = IdKind::Function;
    const REMAP: bool = false;
}

macro_rules! record {
 ($ty:ty { $($field:ident),* $(,)? }) => {
  impl Encode for $ty { fn encode(&self,e:&mut Encoder)->Result<(),Error> { e.nested(|e| { $(self.$field.encode(e)?;)* Ok(()) }) } }
  impl Decode for $ty { fn decode(d:&mut Decoder<'_>)->Result<Self,Error> { d.nested(|d| Ok(Self { $($field: Decode::decode(d)?,)* })) } }
 };
}
record!(CaptureUse {
    local,
    range,
    assigned,
    unique
});
record!(Upvars {
    kind,
    names,
    by_ref,
    mutable,
    annotated_refs,
    fixed_args
});
record!(Place { local, projection });
record!(LocalDecl { ty, kind });
record!(BasicBlockData {
    statements,
    terminator
});
record!(Statement { kind, range });

impl Encode for ProjectionElem {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Field(v0) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Deref => {
                1u8.encode(e)?;
                Ok(())
            }
            Self::BoxDeref => {
                2u8.encode(e)?;
                Ok(())
            }
            Self::Downcast(v0) => {
                3u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Index(v0) => {
                4u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::ConstantIndex {
                offset,
                min_length,
                from_end,
            } => {
                5u8.encode(e)?;
                offset.encode(e)?;
                min_length.encode(e)?;
                from_end.encode(e)?;
                Ok(())
            }
            Self::Subslice { from, to, from_end } => {
                6u8.encode(e)?;
                from.encode(e)?;
                to.encode(e)?;
                from_end.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for ProjectionElem {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Field(Decode::decode(d)?)),
            1 => Ok(Self::Deref),
            2 => Ok(Self::BoxDeref),
            3 => Ok(Self::Downcast(Decode::decode(d)?)),
            4 => Ok(Self::Index(Decode::decode(d)?)),
            5 => Ok(Self::ConstantIndex {
                offset: Decode::decode(d)?,
                min_length: Decode::decode(d)?,
                from_end: Decode::decode(d)?,
            }),
            6 => Ok(Self::Subslice {
                from: Decode::decode(d)?,
                to: Decode::decode(d)?,
                from_end: Decode::decode(d)?,
            }),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for LocalKind {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Return => {
                0u8.encode(e)?;
                Ok(())
            }
            Self::Param => {
                1u8.encode(e)?;
                Ok(())
            }
            Self::User { name, mutable } => {
                2u8.encode(e)?;
                name.encode(e)?;
                mutable.encode(e)?;
                Ok(())
            }
            Self::Temp => {
                3u8.encode(e)?;
                Ok(())
            }
            Self::Static(v0) => {
                4u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Overloaded { index, ty } => {
                5u8.encode(e)?;
                index.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for LocalKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Return),
            1 => Ok(Self::Param),
            2 => Ok(Self::User {
                name: Decode::decode(d)?,
                mutable: Decode::decode(d)?,
            }),
            3 => Ok(Self::Temp),
            4 => Ok(Self::Static(Decode::decode(d)?)),
            5 => Ok(Self::Overloaded {
                index: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for StatementKind {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Assign(v0, v1) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::StorageDead(v0) => {
                1u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::PlaceMention(v0) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for StatementKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Assign(Decode::decode(d)?, Decode::decode(d)?)),
            1 => Ok(Self::StorageDead(Decode::decode(d)?)),
            2 => Ok(Self::PlaceMention(Decode::decode(d)?)),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for Rvalue {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::WithAddr(pointer, address) => {
                14u8.encode(e)?;
                pointer.encode(e)?;
                address.encode(e)
            }
            Self::OffsetOf(ty, fields) => {
                13u8.encode(e)?;
                ty.encode(e)?;
                fields.encode(e)
            }
            Self::Use(v0) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::InlineAsm {
                template,
                registers,
                options,
                inputs,
            } => {
                1u8.encode(e)?;
                template.encode(e)?;
                registers.encode(e)?;
                options.encode(e)?;
                inputs.encode(e)?;
                Ok(())
            }
            Self::BinaryOp(v0, v1, v2) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                v2.encode(e)?;
                Ok(())
            }
            Self::UnaryOp(v0, v1) => {
                3u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Cast(v0, v1) => {
                4u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Aggregate(v0, v1) => {
                5u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Discriminant(v0) => {
                6u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::DiscriminantForDrop(v0) => {
                7u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Ref {
                raw,
                mutability,
                place,
                two_phase,
            } => {
                8u8.encode(e)?;
                raw.encode(e)?;
                mutability.encode(e)?;
                place.encode(e)?;
                two_phase.encode(e)?;
                Ok(())
            }
            Self::Len { place, borrows } => {
                9u8.encode(e)?;
                place.encode(e)?;
                borrows.encode(e)?;
                Ok(())
            }
            Self::Repeat(v0, v1) => {
                10u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Unsize(v0, v1) => {
                11u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Subslice { slice, start, end } => {
                12u8.encode(e)?;
                slice.encode(e)?;
                start.encode(e)?;
                end.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for Rvalue {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            14 => Ok(Self::WithAddr(Decode::decode(d)?, Decode::decode(d)?)),
            13 => Ok(Self::OffsetOf(Decode::decode(d)?, Decode::decode(d)?)),
            0 => Ok(Self::Use(Decode::decode(d)?)),
            1 => Ok(Self::InlineAsm {
                template: Decode::decode(d)?,
                registers: Decode::decode(d)?,
                options: Decode::decode(d)?,
                inputs: Decode::decode(d)?,
            }),
            2 => Ok(Self::BinaryOp(
                Decode::decode(d)?,
                Decode::decode(d)?,
                Decode::decode(d)?,
            )),
            3 => Ok(Self::UnaryOp(Decode::decode(d)?, Decode::decode(d)?)),
            4 => Ok(Self::Cast(Decode::decode(d)?, Decode::decode(d)?)),
            5 => Ok(Self::Aggregate(Decode::decode(d)?, Decode::decode(d)?)),
            6 => Ok(Self::Discriminant(Decode::decode(d)?)),
            7 => Ok(Self::DiscriminantForDrop(Decode::decode(d)?)),
            8 => Ok(Self::Ref {
                raw: Decode::decode(d)?,
                mutability: Decode::decode(d)?,
                place: Decode::decode(d)?,
                two_phase: Decode::decode(d)?,
            }),
            9 => Ok(Self::Len {
                place: Decode::decode(d)?,
                borrows: Decode::decode(d)?,
            }),
            10 => Ok(Self::Repeat(Decode::decode(d)?, Decode::decode(d)?)),
            11 => Ok(Self::Unsize(Decode::decode(d)?, Decode::decode(d)?)),
            12 => Ok(Self::Subslice {
                slice: Decode::decode(d)?,
                start: Decode::decode(d)?,
                end: Decode::decode(d)?,
            }),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for AggregateKind {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Tuple => {
                0u8.encode(e)?;
                Ok(())
            }
            Self::Array(v0) => {
                1u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Struct(v0) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Union(v0, v1) => {
                3u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Variant(v0, v1) => {
                4u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::Closure => {
                5u8.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for AggregateKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Tuple),
            1 => Ok(Self::Array(Decode::decode(d)?)),
            2 => Ok(Self::Struct(Decode::decode(d)?)),
            3 => Ok(Self::Union(Decode::decode(d)?, Decode::decode(d)?)),
            4 => Ok(Self::Variant(Decode::decode(d)?, Decode::decode(d)?)),
            5 => Ok(Self::Closure),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for Operand {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Copy(v0) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Move(v0) => {
                1u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Const(v0) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for Operand {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Copy(Decode::decode(d)?)),
            1 => Ok(Self::Move(Decode::decode(d)?)),
            2 => Ok(Self::Const(Decode::decode(d)?)),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for Constant {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Float { bits, ty } => {
                0u8.encode(e)?;
                bits.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Int { bits, ty } => {
                1u8.encode(e)?;
                bits.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Bool(v0) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Char(v0) => {
                3u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Unit => {
                4u8.encode(e)?;
                Ok(())
            }
            Self::FnItem(v0) => {
                5u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::CStr(v0) => {
                6u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Item { id, args, ty } => {
                7u8.encode(e)?;
                id.encode(e)?;
                args.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Fn { function, args, ty } => {
                8u8.encode(e)?;
                function.encode(e)?;
                args.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Static { id, ty } => {
                9u8.encode(e)?;
                id.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Inline { index, ty } => {
                13u8.encode(e)?;
                index.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Promoted { index, ty } => {
                10u8.encode(e)?;
                index.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::TyConst { value, ty } => {
                11u8.encode(e)?;
                value.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::ItemRef { id, args, ty } => {
                12u8.encode(e)?;
                id.encode(e)?;
                args.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for Constant {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Float {
                bits: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            1 => Ok(Self::Int {
                bits: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            2 => Ok(Self::Bool(Decode::decode(d)?)),
            3 => Ok(Self::Char(Decode::decode(d)?)),
            4 => Ok(Self::Unit),
            5 => Ok(Self::FnItem(Decode::decode(d)?)),
            6 => Ok(Self::CStr(Decode::decode(d)?)),
            7 => Ok(Self::Item {
                id: Decode::decode(d)?,
                args: Ty::list(&Vec::<Ty>::decode(d)?),
                ty: Decode::decode(d)?,
            }),
            8 => Ok(Self::Fn {
                function: Decode::decode(d)?,
                args: Ty::list(&Vec::<Ty>::decode(d)?),
                ty: Decode::decode(d)?,
            }),
            9 => Ok(Self::Static {
                id: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            10 => Ok(Self::Promoted {
                index: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            11 => Ok(Self::TyConst {
                value: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            12 => Ok(Self::ItemRef {
                id: Decode::decode(d)?,
                args: Ty::list(&Vec::<Ty>::decode(d)?),
                ty: Decode::decode(d)?,
            }),
            13 => Ok(Self::Inline {
                index: Decode::decode(d)?,
                ty: Decode::decode(d)?,
            }),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for BinOp {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Add => {
                0u8.encode(e)?;
                Ok(())
            }
            Self::Sub => {
                1u8.encode(e)?;
                Ok(())
            }
            Self::Mul => {
                2u8.encode(e)?;
                Ok(())
            }
            Self::Div => {
                3u8.encode(e)?;
                Ok(())
            }
            Self::Rem => {
                4u8.encode(e)?;
                Ok(())
            }
            Self::Shl => {
                5u8.encode(e)?;
                Ok(())
            }
            Self::Shr => {
                6u8.encode(e)?;
                Ok(())
            }
            Self::BitAnd => {
                7u8.encode(e)?;
                Ok(())
            }
            Self::BitOr => {
                8u8.encode(e)?;
                Ok(())
            }
            Self::BitXor => {
                9u8.encode(e)?;
                Ok(())
            }
            Self::Eq => {
                10u8.encode(e)?;
                Ok(())
            }
            Self::Ne => {
                11u8.encode(e)?;
                Ok(())
            }
            Self::Lt => {
                12u8.encode(e)?;
                Ok(())
            }
            Self::Le => {
                13u8.encode(e)?;
                Ok(())
            }
            Self::Gt => {
                14u8.encode(e)?;
                Ok(())
            }
            Self::Ge => {
                15u8.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for BinOp {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Add),
            1 => Ok(Self::Sub),
            2 => Ok(Self::Mul),
            3 => Ok(Self::Div),
            4 => Ok(Self::Rem),
            5 => Ok(Self::Shl),
            6 => Ok(Self::Shr),
            7 => Ok(Self::BitAnd),
            8 => Ok(Self::BitOr),
            9 => Ok(Self::BitXor),
            10 => Ok(Self::Eq),
            11 => Ok(Self::Ne),
            12 => Ok(Self::Lt),
            13 => Ok(Self::Le),
            14 => Ok(Self::Gt),
            15 => Ok(Self::Ge),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for UnOp {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Neg => {
                0u8.encode(e)?;
                Ok(())
            }
            Self::Not => {
                1u8.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for UnOp {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Neg),
            1 => Ok(Self::Not),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
// Tag 255 extends a terminator without changing abort-mode metadata bytes.
const UNWIND_TERMINATOR: u8 = 255;

impl Encode for Terminator {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| {
            if self.unwind != UnwindAction::Unreachable {
                UNWIND_TERMINATOR.encode(e)?;
                match self.unwind {
                    UnwindAction::Unreachable => unreachable!("handled above"),
                    UnwindAction::Continue => 0u8.encode(e)?,
                    UnwindAction::Cleanup(block) => {
                        1u8.encode(e)?;
                        block.encode(e)?;
                    }
                    UnwindAction::Terminate => 2u8.encode(e)?,
                }
            }
            self.kind.encode(e)?;
            self.range.encode(e)
        })
    }
}
impl Decode for Terminator {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| {
            let tag = u8::decode(d)?;
            let (unwind, kind) = if tag == UNWIND_TERMINATOR {
                let unwind = match u8::decode(d)? {
                    0 => UnwindAction::Continue,
                    1 => UnwindAction::Cleanup(Decode::decode(d)?),
                    2 => UnwindAction::Terminate,
                    _ => return Err(Error::new("invalid MIR unwind action")),
                };
                (unwind, TerminatorKind::decode(d)?)
            } else {
                (
                    UnwindAction::Unreachable,
                    d.nested(|d| TerminatorKind::decode_tag(tag, d))?,
                )
            };
            Ok(Self {
                kind,
                unwind,
                range: Decode::decode(d)?,
            })
        })
    }
}

impl Encode for TerminatorKind {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Goto(v0) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::SwitchInt {
                discr,
                targets,
                otherwise,
                variants,
            } => {
                1u8.encode(e)?;
                discr.encode(e)?;
                targets.encode(e)?;
                otherwise.encode(e)?;
                variants.encode(e)?;
                Ok(())
            }
            Self::Call {
                func,
                generic_args,
                args,
                dest,
                target,
            } => {
                2u8.encode(e)?;
                func.encode(e)?;
                generic_args.encode(e)?;
                args.encode(e)?;
                dest.encode(e)?;
                target.encode(e)?;
                Ok(())
            }
            Self::CallIndirect {
                callee,
                args,
                dest,
                target,
            } => {
                3u8.encode(e)?;
                callee.encode(e)?;
                args.encode(e)?;
                dest.encode(e)?;
                target.encode(e)?;
                Ok(())
            }
            Self::Drop { place, target } => {
                4u8.encode(e)?;
                place.encode(e)?;
                target.encode(e)?;
                Ok(())
            }
            Self::Assert {
                cond,
                expected,
                msg,
                target,
            } => {
                5u8.encode(e)?;
                cond.encode(e)?;
                expected.encode(e)?;
                msg.encode(e)?;
                target.encode(e)?;
                Ok(())
            }
            Self::Return => {
                6u8.encode(e)?;
                Ok(())
            }
            Self::Unreachable => {
                7u8.encode(e)?;
                Ok(())
            }
            Self::UnwindResume => {
                8u8.encode(e)?;
                Ok(())
            }
            Self::UnwindTerminate => {
                9u8.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for TerminatorKind {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| {
            let tag = u8::decode(d)?;
            Self::decode_tag(tag, d)
        })
    }
}
impl TerminatorKind {
    fn decode_tag(tag: u8, d: &mut Decoder<'_>) -> Result<Self, Error> {
        match tag {
            0 => Ok(Self::Goto(Decode::decode(d)?)),
            1 => Ok(Self::SwitchInt {
                discr: Decode::decode(d)?,
                targets: Decode::decode(d)?,
                otherwise: Decode::decode(d)?,
                variants: Decode::decode(d)?,
            }),
            2 => Ok(Self::Call {
                func: Decode::decode(d)?,
                generic_args: Ty::list(&Vec::<Ty>::decode(d)?),
                args: Decode::decode(d)?,
                dest: Decode::decode(d)?,
                target: Decode::decode(d)?,
            }),
            3 => Ok(Self::CallIndirect {
                callee: Decode::decode(d)?,
                args: Decode::decode(d)?,
                dest: Decode::decode(d)?,
                target: Decode::decode(d)?,
            }),
            4 => Ok(Self::Drop {
                place: Decode::decode(d)?,
                target: Decode::decode(d)?,
            }),
            5 => Ok(Self::Assert {
                cond: Decode::decode(d)?,
                expected: Decode::decode(d)?,
                msg: Decode::decode(d)?,
                target: Decode::decode(d)?,
            }),
            6 => Ok(Self::Return),
            7 => Ok(Self::Unreachable),
            8 => Ok(Self::UnwindResume),
            9 => Ok(Self::UnwindTerminate),
            _ => Err(Error::new("invalid MIR metadata tag")),
        }
    }
}
impl Encode for AssertMessage {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::DivisionByZero(v0) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::RemainderByZero(v0) => {
                1u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::DivisionOverflow(v0, v1) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::RemainderOverflow(v0, v1) => {
                3u8.encode(e)?;
                v0.encode(e)?;
                v1.encode(e)?;
                Ok(())
            }
            Self::BoundsCheck { len, index } => {
                4u8.encode(e)?;
                len.encode(e)?;
                index.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for AssertMessage {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::DivisionByZero(Decode::decode(d)?)),
            1 => Ok(Self::RemainderByZero(Decode::decode(d)?)),
            2 => Ok(Self::DivisionOverflow(
                Decode::decode(d)?,
                Decode::decode(d)?,
            )),
            3 => Ok(Self::RemainderOverflow(
                Decode::decode(d)?,
                Decode::decode(d)?,
            )),
            4 => Ok(Self::BoundsCheck {
                len: Decode::decode(d)?,
                index: Decode::decode(d)?,
            }),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}
impl Encode for Value {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        allocations::value_height(self, e, 0)?;
        e.nested(|e| match self {
            Self::Union { field, value } => {
                15u8.encode(e)?;
                field.encode(e)?;
                allocations::encode_value(value, e)
            }
            Self::Float { bits, ty } => {
                0u8.encode(e)?;
                bits.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Int { bits, ty } => {
                1u8.encode(e)?;
                bits.encode(e)?;
                ty.encode(e)?;
                Ok(())
            }
            Self::Bool(v0) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Function(v0) => {
                3u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::RawAddr(v0) => {
                4u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::DynPointer {
                pointer,
                concrete,
                object,
                mutable,
            } => {
                5u8.encode(e)?;
                allocations::encode_value(pointer, e)?;
                concrete.encode(e)?;
                object.encode(e)?;
                mutable.encode(e)?;
                Ok(())
            }
            Self::Aggregate(v0) => {
                6u8.encode(e)?;
                allocations::encode_fields(v0, e)?;
                Ok(())
            }
            Self::Variant {
                strukt,
                index,
                fields,
            } => {
                7u8.encode(e)?;
                strukt.encode(e)?;
                index.encode(e)?;
                allocations::encode_fields(fields, e)?;
                Ok(())
            }
            Self::Static(v0) => {
                8u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::StaticPart { id, path } => {
                9u8.encode(e)?;
                id.encode(e)?;
                path.as_ref().encode(e)?;
                Ok(())
            }
            Self::Ref(v0) => {
                10u8.encode(e)?;
                allocations::encode_value(v0, e)?;
                Ok(())
            }
            Self::InteriorRef(v0) => {
                11u8.encode(e)?;
                allocations::encode_value(v0, e)?;
                Ok(())
            }
            Self::PointerView {
                pointer,
                source,
                target,
            } => {
                12u8.encode(e)?;
                allocations::encode_value(pointer, e)?;
                source.encode(e)?;
                target.encode(e)?;
                Ok(())
            }
            Self::PointerOffset {
                pointer,
                source,
                offset,
                target,
                len,
            } => {
                13u8.encode(e)?;
                allocations::encode_value(pointer, e)?;
                source.encode(e)?;
                offset.encode(e)?;
                target.encode(e)?;
                len.encode(e)?;
                Ok(())
            }
            Self::Uninit(ty) => {
                14u8.encode(e)?;
                ty.encode(e)
            }
            Self::FramePointer { .. } | Self::LocalRef { .. } => {
                Err(Error::new("frame-local value cannot be serialized"))
            }
        })
    }
}
impl Decode for Value {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| {
            let value = match u8::decode(d)? {
                15 => Ok(Self::Union {
                    field: Decode::decode(d)?,
                    value: allocations::decode_value(d)?,
                }),
                0 => Ok(Self::Float {
                    bits: Decode::decode(d)?,
                    ty: Decode::decode(d)?,
                }),
                1 => Ok(Self::Int {
                    bits: Decode::decode(d)?,
                    ty: Decode::decode(d)?,
                }),
                2 => Ok(Self::Bool(Decode::decode(d)?)),
                3 => Ok(Self::Function(Decode::decode(d)?)),
                4 => Ok(Self::RawAddr(Decode::decode(d)?)),
                5 => Ok(Self::DynPointer {
                    pointer: allocations::decode_value(d)?,
                    concrete: Decode::decode(d)?,
                    object: Decode::decode(d)?,
                    mutable: Decode::decode(d)?,
                }),
                6 => Ok(Self::Aggregate(allocations::decode_fields(d)?)),
                7 => Ok(Self::Variant {
                    strukt: Decode::decode(d)?,
                    index: Decode::decode(d)?,
                    fields: allocations::decode_fields(d)?,
                }),
                8 => Ok(Self::Static(Decode::decode(d)?)),
                9 => Ok(Self::StaticPart {
                    id: Decode::decode(d)?,
                    path: Vec::<StaticProjection>::decode(d)?.into(),
                }),
                10 => Ok(Self::Ref(allocations::decode_value(d)?)),
                11 => Ok(Self::InteriorRef(allocations::decode_value(d)?)),
                12 => Ok(Self::PointerView {
                    pointer: allocations::decode_value(d)?,
                    source: Decode::decode(d)?,
                    target: Decode::decode(d)?,
                }),
                13 => Ok(Self::PointerOffset {
                    pointer: allocations::decode_value(d)?,
                    source: Decode::decode(d)?,
                    offset: Decode::decode(d)?,
                    target: Decode::decode(d)?,
                    len: Decode::decode(d)?,
                }),
                14 => Ok(Self::Uninit(Decode::decode(d)?)),
                _ => Err(Error::new("invalid MIR metadata tag")),
            }?;
            allocations::decoded_height(&value, d)?;
            Ok(value)
        })
    }
}
impl Encode for StaticProjection {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| match self {
            Self::Field(v0) => {
                0u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Index(v0) => {
                1u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Downcast(v0) => {
                2u8.encode(e)?;
                v0.encode(e)?;
                Ok(())
            }
            Self::Subslice { from, len } => {
                3u8.encode(e)?;
                from.encode(e)?;
                len.encode(e)?;
                Ok(())
            }
        })
    }
}
impl Decode for StaticProjection {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| match u8::decode(d)? {
            0 => Ok(Self::Field(Decode::decode(d)?)),
            1 => Ok(Self::Index(Decode::decode(d)?)),
            2 => Ok(Self::Downcast(Decode::decode(d)?)),
            3 => Ok(Self::Subslice {
                from: Decode::decode(d)?,
                len: Decode::decode(d)?,
            }),
            _ => Err(Error::new("invalid MIR metadata tag")),
        })
    }
}

impl Encode for Projection {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.as_ref().encode(e)
    }
}
impl Decode for Projection {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Vec::<ProjectionElem>::decode(d)?.into())
    }
}

fn encode_arena<T: Encode>(arena: &Arena<T>, e: &mut Encoder) -> Result<(), Error> {
    e.nested(|e| {
        arena.len().encode(e)?;
        for value in arena.values() {
            value.encode(e)?;
        }
        Ok(())
    })
}
fn decode_arena<T: Decode>(d: &mut Decoder<'_>) -> Result<Arena<T>, Error> {
    let mut arena = Arena::new();
    for value in Vec::<T>::decode(d)? {
        arena.alloc(value);
    }
    Ok(arena)
}
fn encode_map<K: FlibItem, V: Encode>(map: &ArenaMap<K, V>, e: &mut Encoder) -> Result<(), Error> {
    e.nested(|e| {
        let entries: Vec<_> = map.iter().collect();
        entries.len().encode(e)?;
        for (key, value) in entries {
            key.encode(e)?;
            value.encode(e)?;
        }
        Ok(())
    })
}
fn decode_map<K: FlibItem, V: Decode>(d: &mut Decoder<'_>) -> Result<ArenaMap<K, V>, Error> {
    let n = d.length()?;
    decode_map_entries(d, n)
}
fn decode_map_entries<K: FlibItem, V: Decode>(
    d: &mut Decoder<'_>,
    n: usize,
) -> Result<ArenaMap<K, V>, Error> {
    let mut map = ArenaMap::new();
    for _ in 0..n {
        let key = Idx::<K>::decode(d)?;
        if K::REMAP && d.ids.is_strict() && !d.ids.contains_owned(K::KIND, key.into_raw()) {
            return Err(Error::new("MIR entry belongs to another crate"));
        }
        if map.get(key).is_some() {
            return Err(Error::new("duplicate MIR entry"));
        }
        map.insert(key, V::decode(d)?);
    }
    Ok(map)
}

impl Encode for Body {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| {
            if !self.bugs.is_empty() || !self.init_errors.is_empty() {
                return Err(Error::new("cannot serialize unchecked MIR"));
            }
            encode_arena(&self.locals, e)?;
            self.region_annotations.encode(e)?;
            self.region_relations.encode(e)?;
            encode_map(&self.call_regions, e)?;
            encode_map(&self.reification_regions, e)?;
            encode_arena(&self.blocks, e)?;
            self.param_count.encode(e)?;
            self.c_strings.encode(e)?;
            self.in_core.encode(e)?;
            self.inherits_overflow_checks.encode(e)?;
            self.live_drops.encode(e)?;
            self.upvars.encode(e)?;
            self.capture_uses.encode(e)?;
            self.owner.encode(e)?;
            self.constant_owner.encode(e)?;
            self.promoted.encode(e)?;
            self.inline_constants.encode(e)?;
            let mut spans: Vec<_> = self
                .fn_spans
                .iter()
                .map(|(&block, &offset)| (block, TextRange::empty(offset)))
                .collect();
            spans.sort_by_key(|(key, _)| key.into_raw());
            spans.encode(e)
        })
    }
}
impl Decode for Body {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| {
            let locals = decode_arena(d)?;
            let region_annotations = Decode::decode(d)?;
            let region_relations = Decode::decode(d)?;
            let call_rows = Vec::<(BasicBlock, flux_ty::regions::TypeArgRegions)>::decode(d)?;
            let reification_rows = Vec::<(Local, flux_ty::regions::TypeArgRegions)>::decode(d)?;
            let mut body = Self {
                locals,
                region_annotations,
                region_relations,
                call_regions: ArenaMap::new(),
                reification_regions: ArenaMap::new(),
                blocks: decode_arena(d)?,
                param_count: Decode::decode(d)?,
                c_strings: Decode::decode(d)?,
                in_core: Decode::decode(d)?,
                inherits_overflow_checks: Decode::decode(d)?,
                live_drops: Decode::decode(d)?,
                upvars: Decode::decode(d)?,
                capture_uses: Decode::decode(d)?,
                owner: Decode::decode(d)?,
                constant_owner: Decode::decode(d)?,
                promoted: Decode::decode(d)?,
                inline_constants: Decode::decode(d)?,
                fn_spans: Default::default(),
                init_errors: Vec::new(),
                bugs: Vec::new(),
            };
            for (block, regions) in call_rows {
                if block.index() >= body.blocks.len() {
                    return Err(Error::new("invalid MIR call region block"));
                }
                if body.call_regions.insert(block, regions).is_some() {
                    return Err(Error::new("duplicate MIR call region block"));
                }
            }
            for (local, rows) in reification_rows {
                if local.index() >= body.locals.len() {
                    return Err(Error::new("invalid MIR reification local"));
                }
                if body.reification_regions.insert(local, rows).is_some() {
                    return Err(Error::new("duplicate MIR reification local"));
                }
            }
            for (block, range) in Vec::<(BasicBlock, TextRange)>::decode(d)? {
                if !range.is_empty() {
                    return Err(Error::new("invalid MIR call position"));
                }
                let span = range.start();
                if block.index() >= body.blocks.len() {
                    return Err(Error::new("invalid MIR span block"));
                }
                if body.fn_spans.insert(block, span).is_some() {
                    return Err(Error::new("duplicate MIR span block"));
                }
            }
            validate_body_indices(&body)?;
            Ok(body)
        })
    }
}

fn validate_body_indices(body: &Body) -> Result<(), Error> {
    if body.has_unwind() && !crate::validate::unwind_errors(body).is_empty() {
        return Err(Error::new("invalid MIR unwind structure"));
    }
    if body.locals.is_empty() || body.blocks.is_empty() || body.param_count >= body.locals.len() {
        return Err(Error::new("invalid MIR body shape"));
    }
    let local = |l: Local| -> Result<(), Error> {
        if l.index() >= body.locals.len() {
            Err(Error::new("invalid MIR local reference"))
        } else {
            Ok(())
        }
    };
    let place = |p: &Place| -> Result<(), Error> {
        local(p.local)?;
        for index in p.index_locals() {
            local(index)?;
        }
        Ok(())
    };
    let operand = |o: &Operand| -> Result<(), Error> {
        match o {
            Operand::Copy(p) | Operand::Move(p) => place(p),
            Operand::Const(Constant::CStr(i)) if *i as usize >= body.c_strings.len() => {
                Err(Error::new("invalid MIR string reference"))
            }
            Operand::Const(Constant::Inline { index, .. })
                if *index as usize >= body.inline_constants.len() =>
            {
                Err(Error::new("invalid MIR inline constant reference"))
            }
            Operand::Const(Constant::Promoted { index, .. })
                if *index as usize >= body.promoted.len() =>
            {
                Err(Error::new("invalid MIR promotion reference"))
            }
            _ => Ok(()),
        }
    };
    for (l, _, _) in &body.region_annotations {
        local(*l)?;
    }
    for (a, b, _, _) in &body.region_relations {
        place(a)?;
        place(b)?;
    }
    for capture in &body.capture_uses {
        local(capture.local)?;
    }
    for block in body.blocks.values() {
        if block
            .terminator
            .successors()
            .any(|b| b.index() >= body.blocks.len())
        {
            return Err(Error::new("invalid MIR block reference"));
        }
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign(p, r) => {
                    place(p)?;
                    match r {
                        Rvalue::OffsetOf(..) => {}
                        Rvalue::Use(o)
                        | Rvalue::UnaryOp(_, o)
                        | Rvalue::Cast(o, _)
                        | Rvalue::Repeat(o, _)
                        | Rvalue::Unsize(o, _) => operand(o)?,
                        Rvalue::WithAddr(a, b) | Rvalue::BinaryOp(_, a, b) => {
                            operand(a)?;
                            operand(b)?;
                        }
                        Rvalue::InlineAsm { inputs, .. } | Rvalue::Aggregate(_, inputs) => {
                            for o in inputs {
                                operand(o)?;
                            }
                        }
                        Rvalue::Discriminant(p)
                        | Rvalue::DiscriminantForDrop(p)
                        | Rvalue::Ref { place: p, .. }
                        | Rvalue::Len { place: p, .. } => place(p)?,
                        Rvalue::Subslice { slice, start, end } => {
                            operand(slice)?;
                            operand(start)?;
                            operand(end)?;
                        }
                    }
                }
                StatementKind::PlaceMention(p) => place(p)?,
                StatementKind::StorageDead(l) => local(*l)?,
            }
        }
        match &block.terminator.kind {
            TerminatorKind::SwitchInt { discr, .. } => operand(discr)?,
            TerminatorKind::Call { args, dest, .. } => {
                for o in args {
                    operand(o)?;
                }
                place(dest)?;
            }
            TerminatorKind::CallIndirect {
                callee, args, dest, ..
            } => {
                operand(callee)?;
                for o in args {
                    operand(o)?;
                }
                place(dest)?;
            }
            TerminatorKind::Drop { place: p, .. } => place(p)?,
            TerminatorKind::Assert { cond, msg, .. } => {
                operand(cond)?;
                match msg {
                    AssertMessage::DivisionByZero(a) | AssertMessage::RemainderByZero(a) => {
                        operand(a)?
                    }
                    AssertMessage::DivisionOverflow(a, b)
                    | AssertMessage::RemainderOverflow(a, b)
                    | AssertMessage::BoundsCheck { len: a, index: b } => {
                        operand(a)?;
                        operand(b)?;
                    }
                }
            }
            _ => {}
        }
    }
    if body.fn_spans.keys().any(|b| b.index() >= body.blocks.len())
        || body
            .call_regions
            .iter()
            .any(|(b, _)| b.index() >= body.blocks.len())
    {
        return Err(Error::new("invalid MIR block reference"));
    }
    if body
        .reification_regions
        .iter()
        .any(|(local, _)| local.index() >= body.locals.len())
    {
        return Err(Error::new("invalid MIR reification local"));
    }
    Ok(())
}

impl FileMir {
    /// Stores checked bodies and constants owned by the artifact's crate.
    pub fn encode_flib(&self, e: &mut Encoder) -> Result<(), Error> {
        if !self.diagnostics.is_empty() || !self.bugs.is_empty() {
            return Err(Error::new("cannot serialize erroneous MIR"));
        }
        let bodies: Vec<_> = self
            .bodies
            .iter()
            .filter(|(id, _)| e.ids.contains_owned(IdKind::Function, id.into_raw()))
            .collect();
        if self.panic_strategy == PanicStrategy::Unwind {
            // A reserved count introduces strategy-tagged MIR. Abort keeps its
            // original byte stream; legacy decoders reject the reserved count.
            u32::MAX.encode(e)?;
            self.panic_strategy.encode(e)?;
        }
        bodies.len().encode(e)?;
        for (id, body) in bodies {
            id.encode(e)?;
            body.encode(e)?;
        }
        let mut closures: Vec<_> = self
            .closures
            .iter()
            .filter(|((id, _), _)| match id.owner {
                flux_ty::ClosureOwner::Function(f) => {
                    e.ids.contains_owned(IdKind::Function, f.into_raw())
                }
                flux_ty::ClosureOwner::Const(c) => {
                    e.ids.contains_owned(IdKind::Const, c.into_raw())
                }
            })
            .collect();
        closures.sort_by_key(|((id, kind), _)| {
            (
                match id.owner {
                    flux_ty::ClosureOwner::Function(f) => (0, f.into_raw()),
                    flux_ty::ClosureOwner::Const(c) => (1, c.into_raw()),
                },
                id.expr.into_raw(),
                match kind {
                    ClosureKind::Fn => 0,
                    ClosureKind::FnMut => 1,
                    ClosureKind::FnOnce => 2,
                },
            )
        });
        closures.len().encode(e)?;
        for (key, body) in closures {
            key.encode(e)?;
            body.encode(e)?;
        }
        let consts: Vec<_> = self
            .consts
            .iter()
            .filter(|(id, _)| e.ids.contains_owned(IdKind::Const, id.into_raw()))
            .collect();
        consts.len().encode(e)?;
        for (id, body) in consts {
            id.encode(e)?;
            body.encode(e)?;
        }
        let values: Vec<_> = self
            .const_values
            .items
            .iter()
            .filter(|(id, _)| e.ids.contains_owned(IdKind::Const, id.into_raw()))
            .collect();
        values.len().encode(e)?;
        for (id, value) in values {
            id.encode(e)?;
            value.encode(e)?;
        }
        // Instance caches are recomputed from serialized constant MIR, not persisted.
        let discriminants: Vec<_> = self
            .discriminants
            .iter()
            .filter(|(id, _)| e.ids.contains_owned(IdKind::Struct, id.into_raw()))
            .collect();
        discriminants.len().encode(e)?;
        for (id, values) in discriminants {
            id.encode(e)?;
            values.encode(e)?;
        }
        Ok(())
    }

    /// Adds decoded MIR to a staged compilation without lowering dependency source.
    pub fn decode_flib_into(
        &mut self,
        d: &mut Decoder<'_>,
        items: &flux_hir::ItemTree,
        sigs: &Signatures,
    ) -> Result<(), Error> {
        let check_types = !d.mir_typing_staged();
        let prefix = u32::decode(d)?;
        let (strategy, count) = if prefix == u32::MAX {
            let strategy = PanicStrategy::decode(d)?;
            if strategy != PanicStrategy::Unwind {
                return Err(Error::new("noncanonical MIR strategy header"));
            }
            (strategy, d.length()?)
        } else {
            let count = prefix as usize;
            if count > flux_hir::flib::MAX_RECORD_LENGTH || count > d.remaining() {
                return Err(Error::new("metadata allocation limit exceeded"));
            }
            (PanicStrategy::Abort, count)
        };
        if strategy != self.panic_strategy {
            return Err(Error::new("panic strategy mismatch in MIR metadata"));
        }
        let bodies = decode_map_entries::<flux_hir::Function, Body>(d, count)?;
        for (id, body) in bodies.iter() {
            if body.owner != Some(id) || body.constant_owner.is_some() {
                return Err(Error::new("MIR function owner mismatch"));
            }
            if sigs
                .fns
                .get(id)
                .is_none_or(|sig| sig.params.len() != body.param_count)
            {
                return Err(Error::new("MIR function parameter count mismatch"));
            }
            boundary::body(body, items, sigs, check_types)?;
            if self.bodies.get(id).is_some() {
                return Err(Error::new("duplicate imported MIR body"));
            }
            self.bodies.insert(id, body.clone());
        }
        let n = d.length()?;
        for _ in 0..n {
            let key = <(ClosureId, ClosureKind)>::decode(d)?;
            let body = Body::decode(d)?;
            let (kind, raw, owner, constant) = match key.0.owner {
                flux_ty::ClosureOwner::Function(f) => {
                    (IdKind::Function, f.into_raw(), Some(f), None)
                }
                flux_ty::ClosureOwner::Const(c) => (IdKind::Const, c.into_raw(), None, Some(c)),
            };
            if d.ids.is_strict() && !d.ids.contains_owned(kind, raw) {
                return Err(Error::new("closure belongs to another crate"));
            }
            if (body.owner.is_some() && body.owner != owner)
                || (body.constant_owner.is_some() && body.constant_owner != constant)
                || body.param_count != 2
            {
                return Err(Error::new("MIR closure owner mismatch"));
            }
            // Generated Fn-family shims deliberately have no body owner. Their
            // self parameter still records the exact enclosing closure identity.
            let receiver = body
                .locals
                .get(Local::from_raw(1))
                .ok_or_else(|| Error::new("missing closure receiver"))?
                .ty;
            let closure = match (key.1, receiver.kind()) {
                (ClosureKind::Fn, TyKind::Ref(flux_ty::Mutability::Not, ty))
                | (ClosureKind::FnMut, TyKind::Ref(flux_ty::Mutability::Mut, ty)) => ty,
                (ClosureKind::FnOnce, TyKind::Closure(..)) => receiver,
                _ => return Err(Error::new("invalid closure receiver kind")),
            };
            if !matches!(closure.kind(),TyKind::Closure(id,_) if id==key.0) {
                return Err(Error::new("MIR closure receiver identity mismatch"));
            }
            boundary::body_for_owner(&body, items, sigs, key.0.owner, check_types)?;
            if self.closures.insert(key, body).is_some() {
                return Err(Error::new("duplicate imported closure"));
            }
        }
        let consts = decode_map::<flux_hir::Const, Body>(d)?;
        for (id, body) in consts.iter() {
            if body.constant_owner != Some(id) || body.owner.is_some() {
                return Err(Error::new("MIR constant owner mismatch"));
            }
            if body.param_count != 0 {
                return Err(Error::new("constant MIR has parameters"));
            }
            boundary::body(body, items, sigs, check_types)?;
            if self.consts.get(id).is_some() {
                return Err(Error::new("duplicate imported constant body"));
            }
            self.consts.insert(id, body.clone());
        }
        let values = decode_map::<flux_hir::Const, Value>(d)?;
        for (id, value) in values.iter() {
            boundary::value_for(
                value,
                *sigs
                    .consts
                    .get(id)
                    .ok_or_else(|| Error::new("missing constant signature"))?,
                items,
                sigs,
                0,
            )?;
            if self.const_values.items.get(id).is_some() {
                return Err(Error::new("duplicate imported constant value"));
            }
            self.const_values.items.insert(id, value.clone());
        }
        let discriminants = decode_map::<flux_hir::Struct, Box<[u128]>>(d)?;
        for (id, values) in discriminants.iter() {
            let count = items
                .structs
                .get(id)
                .and_then(|s| s.variants.as_ref())
                .ok_or(Error::new("discriminants require an enum"))?
                .len();
            if count != values.len() {
                return Err(Error::new("invalid discriminant count"));
            }
            if self.discriminants.get(id).is_some() {
                return Err(Error::new("duplicate imported discriminants"));
            }
            self.discriminants.insert(id, values.clone());
        }
        self.validate_imported_closures(&d.ids, sigs)?;
        Ok(())
    }

    fn validate_imported_closures(
        &self,
        ids: &flux_hir::flib::IdMap,
        sigs: &Signatures,
    ) -> Result<(), Error> {
        let owns = |id: ClosureId| match id.owner {
            flux_ty::ClosureOwner::Function(f) => {
                ids.contains_owned(IdKind::Function, f.into_raw())
            }
            flux_ty::ClosureOwner::Const(c) => ids.contains_owned(IdKind::Const, c.into_raw()),
        };
        let mut pending = Vec::new();
        for (id, body) in self.bodies.iter() {
            if ids.contains_owned(IdKind::Function, id.into_raw()) {
                pending.extend(body.locals.values().map(|l| l.ty));
            }
        }
        for (id, body) in self.consts.iter() {
            if ids.contains_owned(IdKind::Const, id.into_raw()) {
                pending.extend(body.locals.values().map(|l| l.ty));
            }
        }
        for ((id, _), body) in &self.closures {
            if owns(*id) {
                pending.extend(body.locals.values().map(|l| l.ty));
            }
        }
        for (&(function, _), &hidden) in &sigs.hidden {
            if ids.contains_owned(IdKind::Function, function.into_raw()) {
                pending.push(hidden);
            }
        }
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if let TyKind::Closure(id, args) = ty.kind()
                && owns(id)
            {
                if args.len() < 3 {
                    return Err(Error::new("invalid imported closure arguments"));
                }
                let kind = flux_ty::ClosureArgs::split(args)
                    .closure_kind()
                    .ok_or_else(|| Error::new("invalid imported closure kind"))?;
                for mode in [ClosureKind::Fn, ClosureKind::FnMut, ClosureKind::FnOnce] {
                    let required = match kind {
                        ClosureKind::Fn => true,
                        ClosureKind::FnMut => mode != ClosureKind::Fn,
                        ClosureKind::FnOnce => mode == ClosureKind::FnOnce,
                    };
                    if required && !self.closures.contains_key(&(id, mode)) {
                        return Err(Error::new("missing imported closure body or shim"));
                    }
                }
            }
            pending.extend(ty.children());
        }
        Ok(())
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    fn minimal_body() -> Body {
        let mut body = Body::empty(None);
        body.locals.alloc(LocalDecl {
            ty: Ty::UNIT,
            kind: LocalKind::Return,
        });
        body.blocks.alloc(BasicBlockData {
            statements: Vec::new(),
            terminator: Terminator {
                unwind: crate::UnwindAction::Unreachable,
                kind: TerminatorKind::Return,
                range: TextRange::new(0, 0),
            },
        });
        body
    }

    #[test]
    fn sparse_call_region_index_is_rejected_before_allocation() {
        let body = minimal_body();
        let mut e = Encoder::new();
        body.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut prefix = Encoder::new();
        encode_arena(&body.locals, &mut prefix).unwrap();
        body.region_annotations.encode(&mut prefix).unwrap();
        body.region_relations.encode(&mut prefix).unwrap();
        let start = prefix.into_bytes().len();
        let mut corrupted = bytes[..start].to_vec();
        let mut row = Encoder::new();
        1u32.encode(&mut row).unwrap();
        u32::MAX.encode(&mut row).unwrap();
        0u32.encode(&mut row).unwrap();
        corrupted.extend_from_slice(&row.into_bytes());
        corrupted.extend_from_slice(&bytes[start + 4..]);
        assert!(Body::decode(&mut Decoder::new(&corrupted)).is_err());
    }

    #[test]
    fn sparse_reification_index_is_rejected_before_allocation() {
        let body = minimal_body();
        let mut e = Encoder::new();
        body.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut prefix = Encoder::new();
        encode_arena(&body.locals, &mut prefix).unwrap();
        body.region_annotations.encode(&mut prefix).unwrap();
        body.region_relations.encode(&mut prefix).unwrap();
        encode_map(&body.call_regions, &mut prefix).unwrap();
        let start = prefix.into_bytes().len();
        let mut corrupted = bytes[..start].to_vec();
        let mut row = Encoder::new();
        1u32.encode(&mut row).unwrap();
        u32::MAX.encode(&mut row).unwrap();
        0u32.encode(&mut row).unwrap();
        corrupted.extend_from_slice(&row.into_bytes());
        corrupted.extend_from_slice(&bytes[start + 4..]);
        assert!(Body::decode(&mut Decoder::new(&corrupted)).is_err());
    }

    #[test]
    fn reification_rows_round_trip_and_reject_unrelated_assignments() {
        let (_, _, hir) = flux_hir::lower_text(
            "reification.rs",
            "fn require<T:'static>(){} fn probe(){let p:fn()=require::<&'static u32>;p();}",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = crate::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let id = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "probe")
            .unwrap()
            .0;
        let body = &mir.bodies[id];
        let (local, rows) = body.reification_regions.iter().next().unwrap();
        assert_eq!(rows[0].1.as_ref(), &[flux_ty::regions::Region::Static]);
        let mut e = Encoder::new();
        body.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        let decoded = Body::decode(&mut d).unwrap();
        d.finish().unwrap();
        boundary::body(&decoded, &hir.items, &typed.sigs, true).unwrap();
        assert_eq!(decoded.reification_regions.get(local), Some(rows));
        for row in [
            vec![(u32::MAX, rows[0].1.clone())],
            vec![(0, Box::new([]))],
            vec![(0, Box::new([flux_ty::regions::Region::Param(u32::MAX)]))],
            vec![(0, Box::new([flux_ty::regions::Region::Late(0)]))],
            vec![rows[0].clone(), rows[0].clone()],
        ] {
            let mut bad = decoded.clone();
            bad.reification_regions.insert(local, row);
            assert!(boundary::body(&bad, &hir.items, &typed.sigs, true).is_err());
        }
        let mut bad = decoded.clone();
        bad.locals[local].kind = LocalKind::Param;
        assert!(boundary::body(&bad, &hir.items, &typed.sigs, true).is_err());
        let (block, statement) = decoded
            .blocks
            .iter()
            .find_map(|(block, data)| {
                data.statements
                    .iter()
                    .position(|s| {
                        matches!(&s.kind,
                StatementKind::Assign(dest, _) if dest.local == local)
                    })
                    .map(|s| (block, s))
            })
            .unwrap();
        let mut bad = decoded.clone();
        let duplicate = bad.blocks[block].statements[statement].clone();
        bad.blocks[block].statements.push(duplicate);
        assert!(boundary::body(&bad, &hir.items, &typed.sigs, true).is_err());
        let mut bad = decoded;
        bad.blocks[block].statements.remove(statement);
        assert!(boundary::body(&bad, &hir.items, &typed.sigs, true).is_err());
    }

    #[test]
    fn duplicate_span_records_are_rejected() {
        let mut body = minimal_body();
        body.fn_spans.insert(BasicBlock::from_raw(0), 2);
        let mut e = Encoder::new();
        body.encode(&mut e).unwrap();
        let mut bytes = e.into_bytes();
        let mut without_spans = body.clone();
        without_spans.fn_spans.clear();
        let mut prefix = Encoder::new();
        without_spans.encode(&mut prefix).unwrap();
        let start = prefix.into_bytes().len() - 4;
        bytes[start..start + 4].copy_from_slice(&2u32.to_le_bytes());
        let duplicate = bytes[start + 4..].to_vec();
        bytes.extend_from_slice(&duplicate);
        assert!(Body::decode(&mut Decoder::new(&bytes)).is_err());
    }

    fn checked_context() -> (Body, flux_hir::ItemTree, Signatures) {
        let (_, _, hir) = flux_hir::lower_text(
            "bounds.rs",
            "fn probe() {}
",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = crate::build_file(&hir, &typed);
        let id = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "probe")
            .unwrap()
            .0;
        let body = mir.bodies[id].clone();
        boundary::body(&body, &hir.items, &typed.sigs, true).unwrap();
        (body, hir.items, typed.sigs)
    }

    #[test]
    fn address_mapping_round_trips_and_rejects_invalid_operand_shapes() {
        let (_, _, hir) = flux_hir::lower_text(
            "mapping.rs",
            "fn probe(pointer:*const [u8],address:usize)->*const [u8]{pointer.with_addr(address)}",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let owner = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "probe")
            .unwrap()
            .0;
        let (items, sigs) = (hir.items, typed.sigs);
        let mut body = minimal_body();
        body.owner = Some(owner);
        let pointer_ty = Ty::raw_ptr(
            flux_ty::Mutability::Not,
            Ty::slice(Ty::int(flux_ty::IntTy::U8)),
        );
        body.locals[Body::RETURN].ty = pointer_ty;
        let pointer = body.locals.alloc(LocalDecl {
            ty: pointer_ty,
            kind: LocalKind::Param,
        });
        let address = body.locals.alloc(LocalDecl {
            ty: Ty::int(flux_ty::IntTy::Usize),
            kind: LocalKind::Param,
        });
        body.param_count = 2;
        body.blocks[Body::entry()].statements.push(Statement {
            kind: StatementKind::Assign(
                Body::RETURN.into(),
                Rvalue::WithAddr(Operand::Copy(pointer.into()), Operand::Copy(address.into())),
            ),
            range: TextRange::default(),
        });
        boundary::body(&body, &items, &sigs, true).unwrap();
        assert!(crate::validate::validate(&items, &sigs, &[], &body).is_empty());
        let mut encoded = Encoder::new();
        body.encode(&mut encoded).unwrap();
        let bytes = encoded.into_bytes();
        let mut decoder = Decoder::new(&bytes);
        let decoded = Body::decode(&mut decoder).unwrap();
        decoder.finish().unwrap();
        boundary::body(&decoded, &items, &sigs, true).unwrap();
        assert_eq!(body.blocks[Body::entry()], decoded.blocks[Body::entry()]);
        let mut bad = decoded.clone();
        bad.locals[address].ty = Ty::BOOL;
        assert!(boundary::body(&bad, &items, &sigs, true).is_err());
        let mut bad = decoded.clone();
        bad.locals[pointer].ty = Ty::int(flux_ty::IntTy::Usize);
        assert!(boundary::body(&bad, &items, &sigs, true).is_err());
        let mut bad = decoded;
        bad.locals[Body::RETURN].ty = Ty::raw_ptr(
            flux_ty::Mutability::Mut,
            Ty::slice(Ty::int(flux_ty::IntTy::U8)),
        );
        assert!(boundary::body(&bad, &items, &sigs, true).is_err());
    }

    #[test]
    fn out_of_range_place_fields_fail_before_type_validator() {
        let (mut body, items, sigs) = checked_context();
        body.blocks[BasicBlock::from_raw(0)]
            .statements
            .push(Statement {
                kind: StatementKind::Assign(
                    Place::from(Body::RETURN).field(u32::MAX),
                    Rvalue::Use(Operand::Const(Constant::Unit)),
                ),
                range: TextRange::new(0, 0),
            });
        assert!(boundary::body(&body, &items, &sigs, true).is_err());
    }

    #[test]
    fn staged_suites_skip_only_type_checks() {
        let (mut body, items, sigs) = checked_context();
        let mut assign = |value| {
            body.blocks[BasicBlock::from_raw(0)]
                .statements
                .push(Statement {
                    kind: StatementKind::Assign(
                        Place::from(Body::RETURN),
                        Rvalue::Use(Operand::Const(value)),
                    ),
                    range: TextRange::new(0, 0),
                });
        };
        // A `bool` stored into a `()` return place is well formed but ill-typed.
        assign(Constant::Bool(true));
        assert!(boundary::body(&body, &items, &sigs, true).is_err());
        boundary::body(&body, &items, &sigs, false).unwrap();
        body.in_core = true;
        assert!(boundary::body(&body, &items, &sigs, false).is_err());
    }

    #[test]
    fn imported_privileges_and_inconsistent_capture_rows_fail() {
        let (mut body, items, sigs) = checked_context();
        body.in_core = true;
        assert!(boundary::body(&body, &items, &sigs, true).is_err());
        body.in_core = false;
        body.upvars = Some(Upvars {
            kind: ClosureKind::Fn,
            names: vec!["captured".into()],
            by_ref: vec![],
            mutable: vec![],
            annotated_refs: false,
            fixed_args: vec![],
        });
        assert!(boundary::body(&body, &items, &sigs, true).is_err());
    }
}

#[cfg(test)]
mod closure_presence_tests {
    use super::*;

    #[test]
    fn imported_closure_types_require_every_callable_shim() {
        let (_, _, hir) = flux_hir::lower_text(
            "closure-presence.rs",
            "fn main() { let plus = |x: u32| x + 1; let _ = plus(1); }",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mut mir = crate::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let own = hir.items.main_crate();
        let mut ids = flux_hir::flib::IdMap::default();
        for (id, f) in hir.items.functions.iter() {
            if hir.items.modules[f.loc.module].krate == own {
                ids.mark_owned(IdKind::Function, id.into_raw());
            }
        }
        mir.validate_imported_closures(&ids, &typed.sigs).unwrap();
        let key = mir
            .closures
            .keys()
            .find(|(id, kind)| {
                *kind == ClosureKind::FnOnce
                    && match id.owner {
                        flux_ty::ClosureOwner::Function(f) => {
                            ids.contains_owned(IdKind::Function, f.into_raw())
                        }
                        _ => false,
                    }
            })
            .copied()
            .expect("source closure has an FnOnce shim");
        mir.closures.remove(&key);
        assert_eq!(
            mir.validate_imported_closures(&ids, &typed.sigs),
            Err(Error::new("missing imported closure body or shim"))
        );
    }
}
