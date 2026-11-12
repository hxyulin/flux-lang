//! Structural type and signature metadata for Fluxlib artifacts.

use crate::regions::*;
use crate::traits::AssociatedTypeSig;
use crate::*;
use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error};
use flux_hir::{ConstId, FunctionId, StructId};

macro_rules! record {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl Encode for $ty {
            fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
                $(self.$field.encode(encoder)?;)*
                Ok(())
            }
        }
        impl Decode for $ty {
            fn decode(decoder: &mut Decoder<'_>) -> Result<Self, Error> {
                Ok(Self { $($field: Decode::decode(decoder)?,)* })
            }
        }
    };
}
macro_rules! unit_enum {
    ($ty:ty { $($variant:ident = $tag:literal),* $(,)? }) => {
        impl Encode for $ty {
            fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
                let tag: u8 = match self { $(Self::$variant => $tag,)* };
                tag.encode(encoder)
            }
        }
        impl Decode for $ty {
            fn decode(decoder: &mut Decoder<'_>) -> Result<Self, Error> {
                match u8::decode(decoder)? {
                    $($tag => Ok(Self::$variant),)*
                    _ => Err(Error::new("invalid type metadata tag")),
                }
            }
        }
    };
}
unit_enum!(Mutability { Not = 0, Mut = 1 });
unit_enum!(IntTy { I8 = 0, I16 = 1, I32 = 2, I64 = 3, I128 = 4, Isize = 5, U8 = 6, U16 = 7, U32 = 8, U64 = 9, U128 = 10, Usize = 11 });
unit_enum!(FloatTy { F32 = 0, F64 = 1 });
unit_enum!(ClosureKind { Fn = 0, FnMut = 1, FnOnce = 2 });
unit_enum!(Variance { Bivariant = 0, Covariant = 1, Contravariant = 2, Invariant = 3 });
unit_enum!(CallbackBinder { Unclassified = 0, Independent = 1, SignatureBound = 2 });
record!(AdtId { strukt, name });
record!(ParamTy { index, name });
impl Encode for ClosureId {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.owner.encode(e)?;
        self.expr.into_raw().encode(e)
    }
}
impl Decode for ClosureId {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            owner: Decode::decode(d)?,
            expr: flux_hir::ExprId::from_raw(u32::decode(d)?),
        })
    }
}
record!(FnSig { params, ret });

record!(Predicate { self_ty, trait_ref });
record!(Projection { alias, ty });
record!(AssociatedTypeSig {
    owner_params,
    lifetimes,
    params,
    predicates,
    projections
});
record!(TraitSig {
    families,
    supertraits,
    defaults,
    type_bounds,
    implied,
    header_projections
});
record!(OpaqueSig {
    bound_origins,
    predicates,
    projections
});
record!(Builtins { sized, auto_traits });
record!(LifetimeName { name, range });
record!(FnRegions {
    names,
    owner,
    params,
    ret,
    selected_inputs,
    outlives,
    has_bounds,
    callback_binder,
    bound_selfs,
    bound_traits,
    projections,
    opaques
});
record!(OpaqueRegions {
    bounds,
    captures,
    outlives,
    projections
});
record!(OpaqueBoundRegions {
    origin,
    predicate,
    regions
});
record!(StructRegions {
    selected_inputs,
    bound_selfs,
    bound_traits,
    projections,
    lifetimes,
    fields,
    variances,
    outlives
});
record!(TraitRefRegions {
    bound,
    lifetimes,
    args
});
record!(TraitRegions {
    supertraits,
    families,
    names,
    outlives
});
record!(FamilyProjection {
    projection,
    regions,
    bound
});
record!(FamilyRegions {
    names,
    owner,
    value,
    outlives,
    output_outlives,
    output_traits,
    output_projections,
    bound_selfs,
    bound_traits,
    projections,
    selections
});
record!(ImplRegions {
    families,
    names,
    self_ty,
    trait_lifetimes,
    trait_args,
    header_selections,
    types,
    type_selections,
    outlives,
    bounds,
    bound_selfs,
    bound_traits,
    projections
});
record!(ProjectionSelection {
    alias,
    qualified,
    lifetimes,
    args
});
record!(AliasRegions { ty, regions });

impl Encode for Region {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        match self {
            Self::Static => 0u8.encode(e),
            Self::Param(i) => {
                1u8.encode(e)?;
                i.encode(e)
            }
            Self::Erased => 2u8.encode(e),
            Self::Late(i) => {
                3u8.encode(e)?;
                i.encode(e)
            }
        }
    }
}
impl Decode for Region {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(Self::Static),
            1 => Ok(Self::Param(u32::decode(d)?)),
            2 => Ok(Self::Erased),
            3 => Ok(Self::Late(u32::decode(d)?)),
            _ => Err(Error::new("invalid region tag")),
        }
    }
}
impl<R: Encode> Encode for Outlives<R> {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        match self {
            Self::Region(a, b) => {
                0u8.encode(e)?;
                a.encode(e)?;
                b.encode(e)
            }
            Self::Type(a, b) => {
                1u8.encode(e)?;
                a.encode(e)?;
                b.encode(e)
            }
        }
    }
}
impl<R: Decode> Decode for Outlives<R> {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(Self::Region(Decode::decode(d)?, Decode::decode(d)?)),
            1 => Ok(Self::Type(Decode::decode(d)?, Decode::decode(d)?)),
            _ => Err(Error::new("invalid outlives tag")),
        }
    }
}
impl Encode for ClosureOwner {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        match self {
            Self::Function(f) => {
                0u8.encode(e)?;
                f.encode(e)
            }
            Self::Const(c) => {
                1u8.encode(e)?;
                c.encode(e)
            }
        }
    }
}
impl Decode for ClosureOwner {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(Self::Function(Decode::decode(d)?)),
            1 => Ok(Self::Const(Decode::decode(d)?)),
            _ => Err(Error::new("invalid closure owner")),
        }
    }
}
impl Encode for FnRegion {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        match self {
            Self::Static => 0u8.encode(e),
            Self::Bound(i) => {
                1u8.encode(e)?;
                i.encode(e)
            }
            Self::Free(i) => {
                2u8.encode(e)?;
                i.encode(e)
            }
            Self::Type(i) => {
                3u8.encode(e)?;
                i.encode(e)
            }
            Self::Projection(i) => {
                4u8.encode(e)?;
                i.encode(e)
            }
        }
    }
}
impl Decode for FnRegion {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        match u8::decode(d)? {
            0 => Ok(Self::Static),
            1 => Ok(Self::Bound(Decode::decode(d)?)),
            2 => Ok(Self::Free(Decode::decode(d)?)),
            3 => Ok(Self::Type(Decode::decode(d)?)),
            4 => Ok(Self::Projection(Decode::decode(d)?)),
            _ => Err(Error::new("invalid function region")),
        }
    }
}
impl Encode for TraitRef {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.trait_.encode(e)?;
        self.name.encode(e)?;
        self.args.encode(e)?;
        Ok(())
    }
}
impl Decode for TraitRef {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            trait_: Decode::decode(d)?,
            name: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
        })
    }
}
impl Encode for AliasTy {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.trait_.encode(e)?;
        self.trait_name.encode(e)?;
        self.name.encode(e)?;
        self.args.encode(e)?;
        self.owner_lifetimes.encode(e)?;
        self.family_lifetimes.encode(e)?;
        self.family_args.encode(e)?;
        Ok(())
    }
}
impl Decode for AliasTy {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            trait_: Decode::decode(d)?,
            trait_name: Decode::decode(d)?,
            name: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
            owner_lifetimes: Decode::decode(d)?,
            family_lifetimes: Decode::decode(d)?,
            family_args: Ty::list(&Vec::<Ty>::decode(d)?),
        })
    }
}
impl Encode for OpaqueTy {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.function.encode(e)?;
        self.index.encode(e)?;
        self.name.encode(e)?;
        self.args.encode(e)?;
        self.lifetimes.encode(e)?;
        Ok(())
    }
}
impl Decode for OpaqueTy {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            function: Decode::decode(d)?,
            index: Decode::decode(d)?,
            name: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
            lifetimes: Decode::decode(d)?,
        })
    }
}
impl Encode for FnDefTy {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.function.encode(e)?;
        self.name.encode(e)?;
        self.args.encode(e)?;
        self.sig.encode(e)?;
        Ok(())
    }
}
impl Decode for FnDefTy {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            function: Decode::decode(d)?,
            name: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
            sig: Decode::decode(d)?,
        })
    }
}
impl Encode for DynTy {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.callable.encode(e)?;
        self.quantified.encode(e)?;
        self.auto_traits.encode(e)?;
        self.trait_.encode(e)?;
        self.name.encode(e)?;
        self.args.encode(e)?;
        self.assoc_names.encode(e)?;
        self.assoc_tys.encode(e)?;
        Ok(())
    }
}
impl Decode for DynTy {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let callable = Option::<Ty>::decode(d)?;
        let quantified = Option::<Ty>::decode(d)?;
        if quantified
            .is_some_and(|ty| !matches!(ty.kind(), TyKind::FnPtr(sig) if sig.regions.is_some()))
            || callable.is_some() && quantified.is_some()
        {
            return Err(Error::new("invalid quantified object region recipe"));
        }
        if callable.is_some_and(|ty| !matches!(ty.kind(), TyKind::FnPtr(_))) {
            return Err(Error::new("invalid callable object region recipe"));
        }
        let object = Self {
            callable,
            quantified,
            auto_traits: crate::ty::object_auto_traits(&Vec::decode(d)?),
            trait_: Decode::decode(d)?,
            name: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
            assoc_names: crate::ty::symbol_list(&Vec::decode(d)?),
            assoc_tys: Ty::list(&Vec::<Ty>::decode(d)?),
        };
        if let Some(signature) = object.quantified_signature()
            && signature
                .params()
                .len()
                .checked_sub(object.args.len() + object.assoc_tys.len())
                .is_none_or(|owners| !object.valid_quantified(owners))
        {
            return Err(Error::new("invalid quantified object signature"));
        }
        Ok(object)
    }
}
impl Encode for Instance {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.function.encode(e)?;
        self.args.encode(e)?;
        Ok(())
    }
}
impl Decode for Instance {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            function: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
        })
    }
}
impl Encode for ConstInstance {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.konst.encode(e)?;
        self.args.encode(e)?;
        Ok(())
    }
}
impl Decode for ConstInstance {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        Ok(Self {
            konst: Decode::decode(d)?,
            args: Ty::list(&Vec::<Ty>::decode(d)?),
        })
    }
}

impl Encode for FnSigTy {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        self.inputs_output.encode(e)?;
        self.is_unsafe.encode(e)?;
        self.abi.encode(e)?;
        self.regions.is_some().encode(e)?;
        if let Some(r) = self.regions {
            r.bound.encode(e)?;
            r.free.encode(e)?;
            r.inputs_output.encode(e)?;
            r.types.encode(e)?;
            (r.projections.len() as u32).encode(e)?;
            for projection in r.projections {
                projection.alias.encode(e)?;
                projection.inputs.encode(e)?;
            }
        }
        Ok(())
    }
}
impl Decode for FnSigTy {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        let types = Vec::<Ty>::decode(d)?;
        if types.is_empty() {
            return Err(Error::new("empty function signature"));
        }
        let is_unsafe = bool::decode(d)?;
        let abi = flux_hir::Abi::decode(d)?;
        let regions = if bool::decode(d)? {
            let bound = u32::decode(d)?;
            let free = u32::decode(d)?;
            let rows = Vec::<Vec<FnRegion>>::decode(d)?;
            let captures = Vec::<Ty>::decode(d)?;
            let count = u32::decode(d)? as usize;
            let mut projections = Vec::new();
            let valid = |term: FnRegion, earlier: usize| match term {
                FnRegion::Bound(i) => i < bound,
                FnRegion::Free(i) => i < free,
                FnRegion::Type(i) => (i as usize) < captures.len(),
                FnRegion::Projection(i) => (i as usize) < earlier,
                FnRegion::Static => true,
            };
            for _ in 0..count {
                let alias = AliasTy::decode(d)?;
                let inputs = Vec::<FnRegion>::decode(d)?;
                if inputs.iter().any(|&term| !valid(term, projections.len())) {
                    return Err(Error::new("invalid function projection input index"));
                }
                projections.push((alias, inputs));
            }
            if rows.len() != types.len() {
                return Err(Error::new("invalid function region rows"));
            }
            if rows
                .iter()
                .flatten()
                .any(|&term| !valid(term, projections.len()))
            {
                return Err(Error::new("invalid function region index"));
            }
            Some(FnPtrRegions::intern_with_projections(
                bound,
                free,
                &rows,
                &captures,
                &projections,
            ))
        } else {
            None
        };
        Ok(Self {
            inputs_output: Ty::list(&types),
            is_unsafe,
            abi,
            regions,
        })
    }
}
impl Encode for Ty {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.nested(|e| {
            let raw = self.flib_cache_key();
            if let Some((wire, _)) = e.type_reference(raw)? {
                1u8.encode(e)?;
                return wire.encode(e);
            }
            let depth = encoding_type_depth(*self, e, 0)?;
            let wire = e.reserve_type(raw)?;
            0u8.encode(e)?;
            wire.encode(e)?;
            match self.kind() {
                TyKind::Int(a0) => {
                    0u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Float(a0) => {
                    1u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Bool => 2u8.encode(e),
                TyKind::Char => 3u8.encode(e),
                TyKind::Str => 4u8.encode(e),
                TyKind::Tuple(a0) => {
                    5u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Adt(a0, a1) => {
                    6u8.encode(e)?;
                    a0.encode(e)?;
                    a1.encode(e)?;
                    Ok(())
                }
                TyKind::Param(a0) => {
                    7u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Alias(a0) => {
                    8u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Opaque(a0) => {
                    9u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Never => 10u8.encode(e),
                TyKind::RawPtr(a0, a1) => {
                    11u8.encode(e)?;
                    a0.encode(e)?;
                    a1.encode(e)?;
                    Ok(())
                }
                TyKind::Ref(a0, a1) => {
                    12u8.encode(e)?;
                    a0.encode(e)?;
                    a1.encode(e)?;
                    Ok(())
                }
                TyKind::Array(a0, a1) => {
                    13u8.encode(e)?;
                    a0.encode(e)?;
                    a1.encode(e)?;
                    Ok(())
                }
                TyKind::Slice(a0) => {
                    14u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::CStr => 15u8.encode(e),
                TyKind::Dynamic(a0) => {
                    16u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::FnDef(a0) => {
                    17u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::FnPtr(a0) => {
                    18u8.encode(e)?;
                    a0.encode(e)?;
                    Ok(())
                }
                TyKind::Closure(a0, a1) => {
                    19u8.encode(e)?;
                    a0.encode(e)?;
                    a1.encode(e)?;
                    Ok(())
                }
                TyKind::Value(v) => {
                    20u8.encode(e)?;
                    v.ty.encode(e)?;
                    v.bits().encode(e)
                }
                TyKind::Infer(_) | TyKind::Error => Err(Error::new("unchecked type in metadata")),
            }?;
            e.complete_type(raw, depth)
        })
    }
}
impl Decode for Ty {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        d.nested(|d| {
            let entry=u8::decode(d)?;
            let wire=u32::decode(d)?;
            if entry==1 {
                let (raw,_)=d.type_reference(wire)?;
                return Ty::from_flib_cache_key(raw).ok_or_else(||Error::new("invalid cached type"));
            }
            if entry!=0{return Err(Error::new("invalid type table tag"));}
            d.reserve_type(wire)?;
            let kind=match u8::decode(d)?{
0=>TyKind::Int(IntTy::decode(d)?),
1=>TyKind::Float(FloatTy::decode(d)?),
2=>TyKind::Bool,
3=>TyKind::Char,
4=>TyKind::Str,
5=>TyKind::Tuple(Ty::list(&Vec::<Ty>::decode(d)?)),
6=>TyKind::Adt(AdtId::decode(d)?,Ty::list(&Vec::<Ty>::decode(d)?)),
7=>TyKind::Param(ParamTy::decode(d)?),
8=>TyKind::Alias(AliasTy::decode(d)?),
9=>TyKind::Opaque(OpaqueTy::decode(d)?),
10=>TyKind::Never,
11=>TyKind::RawPtr(Mutability::decode(d)?,Ty::decode(d)?),
12=>TyKind::Ref(Mutability::decode(d)?,Ty::decode(d)?),
13=>TyKind::Array(Ty::decode(d)?,Ty::decode(d)?),
14=>TyKind::Slice(Ty::decode(d)?),
15=>TyKind::CStr,
16=>TyKind::Dynamic(DynTy::decode(d)?),
17=>TyKind::FnDef(FnDefTy::decode(d)?),
18=>TyKind::FnPtr(FnSigTy::decode(d)?),
19=>TyKind::Closure(ClosureId::decode(d)?,Ty::list(&Vec::<Ty>::decode(d)?)),
20=>TyKind::Value(crate::ty::ConstVal::new(Ty::decode(d)?,u128::decode(d)?)),
_=>return Err(Error::new("invalid type tag")),
};
if let TyKind::Closure(_,args)=kind {
 if args.len()<3{return Err(Error::new("invalid closure arguments"));}
 let closure=ClosureArgs::split(args);
 if closure.closure_kind().is_none() || !matches!(closure.sig.kind(),TyKind::Tuple([params,_]) if matches!(params.kind(),TyKind::Tuple(_))) || !matches!(closure.upvars.kind(),TyKind::Tuple(_)) {return Err(Error::new("invalid closure argument shape"));}
}
if let TyKind::Value(value)=kind {
 let valid=match value.ty.kind(){TyKind::Int(int)=>int.bits()==128 || value.bits()>>int.bits()==0,TyKind::Bool=>value.bits()<=1,TyKind::Char=>u32::try_from(value.bits()).ok().and_then(char::from_u32).is_some(),_=>false};
 if !valid{return Err(Error::new("invalid scalar constant"));}
}
if let TyKind::Alias(alias)=kind && alias.args.is_empty(){return Err(Error::new("missing associated self type"));}
if let TyKind::Dynamic(object)=kind && object.assoc_names.len()!=object.assoc_tys.len(){return Err(Error::new("invalid object associated types"));}
let ty=Ty::new(kind);
let mut depth=1;
for child in encoded_type_children(ty) {
    let child_depth=d.type_depth(child.flib_cache_key()).ok_or_else(||Error::new("missing type semantic depth"))?;
    depth=depth.max(child_depth+1);
}
d.complete_type(wire,ty.flib_cache_key(),depth)?;
Ok(ty) })
    }
}

// Include the type-valued fields omitted by ordinary semantic traversals.
fn encoded_type_children(ty: Ty) -> Vec<Ty> {
    let mut children: Vec<_> = ty.children().collect();
    match ty.kind() {
        TyKind::Value(v) => children.push(v.ty),
        TyKind::Dynamic(d) => {
            for bound in d.auto_traits {
                children.extend_from_slice(bound.args);
            }
        }
        _ => {}
    }
    children
}

fn encoding_type_depth(ty: Ty, e: &mut Encoder, nesting: u32) -> Result<u32, Error> {
    use flux_hir::flib::MAX_NESTING_DEPTH;
    if nesting >= MAX_NESTING_DEPTH {
        return Err(Error::new("type semantic nesting limit exceeded"));
    }
    if let Some(depth) = e.type_depth(ty.flib_cache_key()) {
        return Ok(depth);
    }
    let mut depth = 1;
    for child in encoded_type_children(ty) {
        depth = depth.max(encoding_type_depth(child, e, nesting + 1)? + 1);
    }
    e.set_type_depth(ty.flib_cache_key(), depth)?;
    Ok(depth)
}

fn encode_map<T: flux_hir::flib::FlibItem, V: Encode>(
    map: &flux_arena::LayeredArenaMap<T, V>,
    e: &mut Encoder,
) -> Result<(), Error> {
    let entries: Vec<_> = map
        .iter()
        .filter(|(id, _)| e.ids.contains_owned(T::KIND, id.into_raw()))
        .collect();
    entries.len().encode(e)?;
    for (id, value) in entries {
        id.encode(e)?;
        value.encode(e)?;
    }
    Ok(())
}
fn decode_map<T: flux_hir::flib::FlibItem + Clone, V: Decode + Clone>(
    map: &mut flux_arena::LayeredArenaMap<T, V>,
    d: &mut Decoder<'_>,
) -> Result<(), Error> {
    let n = d.length()?;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..n {
        let id = flux_arena::Idx::<T>::decode(d)?;
        if d.ids.is_strict() && !d.ids.contains_owned(T::KIND, id.into_raw()) {
            return Err(Error::new("signature entry belongs to another crate"));
        }
        if !seen.insert(id.into_raw()) {
            return Err(Error::new("duplicate signature entry"));
        }
        map.insert(id, V::decode(d)?);
    }
    Ok(())
}

impl Signatures {
    /// Writes semantic records owned by the encoder's crate selection.
    /// Lookup indexes and process-local runtime type IDs are rebuilt on load.
    pub fn encode_flib(&self, e: &mut Encoder) -> Result<(), Error> {
        if self.cyclic {
            return Err(Error::new("cyclic signatures cannot be exported"));
        }
        encode_map(&self.fns, e)?;
        encode_map(&self.variant_starts, e)?;
        encode_map(&self.discriminants, e)?;
        encode_map(&self.impls, e)?;
        encode_map(&self.consts, e)?;
        encode_map(&self.traits, e)?;
        encode_map(&self.impl_traits, e)?;
        encode_map(&self.impl_predicates, e)?;
        encode_map(&self.struct_predicates, e)?;
        encode_map(&self.trait_predicates, e)?;
        encode_map(&self.fn_predicates, e)?;
        encode_map(&self.impl_projections, e)?;
        encode_map(&self.struct_projections, e)?;
        encode_map(&self.trait_projections, e)?;
        encode_map(&self.fn_projections, e)?;
        encode_map(&self.impl_types, e)?;
        encode_map(&self.impl_families, e)?;
        encode_map(&self.opaques, e)?;
        encode_map(&self.trait_defaults, e)?;
        let fields: Vec<_> = self
            .structs
            .iter()
            .filter(|(id, _)| {
                e.ids
                    .contains_owned(flux_hir::flib::IdKind::Struct, id.into_raw())
            })
            .collect();
        fields.len().encode(e)?;
        for (id, tys) in fields {
            id.encode(e)?;
            tys.encode(e)?;
        }
        encode_map(&self.regions.traits, e)?;
        encode_map(&self.regions.fns, e)?;
        encode_map(&self.regions.structs, e)?;
        encode_map(&self.regions.impls, e)?;
        encode_map(&self.regions.consts, e)?;
        let mut entries: Vec<_> = self
            .sized_dispatch
            .iter()
            .filter(|(id, _)| {
                e.ids
                    .contains_owned(flux_hir::flib::IdKind::Function, id.into_raw())
            })
            .collect();
        entries.sort_unstable_by_key(|(id, _)| **id);
        entries.len().encode(e)?;
        for (id, value) in entries {
            id.encode(e)?;
            value.encode(e)?;
        }
        let mut entries: Vec<_> = self
            .hidden
            .iter()
            .filter(|(id, _)| {
                e.ids
                    .contains_owned(flux_hir::flib::IdKind::Function, id.0.into_raw())
            })
            .collect();
        entries.sort_unstable_by_key(|(id, _)| **id);
        entries.len().encode(e)?;
        for (id, value) in entries {
            id.encode(e)?;
            value.encode(e)?;
        }
        let mut entries: Vec<_> = self
            .array_lens
            .iter()
            .filter(|(id, _)| {
                e.ids
                    .contains_owned(flux_hir::flib::IdKind::Const, id.into_raw())
            })
            .collect();
        entries.sort_unstable_by_key(|(id, _)| **id);
        entries.len().encode(e)?;
        for (id, value) in entries {
            id.encode(e)?;
            value.encode(e)?;
        }
        let mut entries: Vec<_> = self
            .arg_tys
            .iter()
            .filter(|(id, _)| {
                e.ids
                    .contains_owned(flux_hir::flib::IdKind::Const, id.into_raw())
            })
            .collect();
        entries.sort_unstable_by_key(|(id, _)| **id);
        entries.len().encode(e)?;
        for (id, value) in entries {
            id.encode(e)?;
            value.encode(e)?;
        }
        let mut unions: Vec<_> = self
            .unions
            .iter()
            .copied()
            .filter(|id| {
                e.ids
                    .contains_owned(flux_hir::flib::IdKind::Struct, id.into_raw())
            })
            .collect();
        unions.sort_unstable();
        unions.encode(e)?;
        Ok(())
    }

    /// Loads remapped semantic records into a signature snapshot.
    /// The caller publishes this snapshot only after every section validates.
    pub fn decode_flib_into(&mut self, d: &mut Decoder<'_>, items: &ItemTree) -> Result<(), Error> {
        decode_map(&mut self.fns, d)?;
        decode_map(&mut self.variant_starts, d)?;
        decode_map(&mut self.discriminants, d)?;
        decode_map(&mut self.impls, d)?;
        decode_map(&mut self.consts, d)?;
        decode_map(&mut self.traits, d)?;
        decode_map(&mut self.impl_traits, d)?;
        decode_map(&mut self.impl_predicates, d)?;
        decode_map(&mut self.struct_predicates, d)?;
        decode_map(&mut self.trait_predicates, d)?;
        decode_map(&mut self.fn_predicates, d)?;
        decode_map(&mut self.impl_projections, d)?;
        decode_map(&mut self.struct_projections, d)?;
        decode_map(&mut self.trait_projections, d)?;
        decode_map(&mut self.fn_projections, d)?;
        decode_map(&mut self.impl_types, d)?;
        decode_map(&mut self.impl_families, d)?;
        decode_map(&mut self.opaques, d)?;
        decode_map(&mut self.trait_defaults, d)?;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..d.length()? {
            let id = StructId::decode(d)?;
            if d.ids.is_strict()
                && !d
                    .ids
                    .contains_owned(flux_hir::flib::IdKind::Struct, id.into_raw())
            {
                return Err(Error::new("struct signature belongs to another crate"));
            }
            if !seen.insert(id.into_raw()) {
                return Err(Error::new("duplicate struct signature"));
            }
            let fields = Vec::<Ty>::decode(d)?;
            self.structs.insert(id, Ty::list(&fields));
        }
        decode_map(&mut self.regions.traits, d)?;
        decode_map(&mut self.regions.fns, d)?;
        decode_map(&mut self.regions.structs, d)?;
        decode_map(&mut self.regions.impls, d)?;
        decode_map(&mut self.regions.consts, d)?;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..d.length()? {
            let id = <FunctionId>::decode(d)?;
            if d.ids.is_strict()
                && !d
                    .ids
                    .contains_owned(flux_hir::flib::IdKind::Function, id.into_raw())
            {
                return Err(Error::new("signature value belongs to another crate"));
            }
            if !seen.insert(id) {
                return Err(Error::new("duplicate signature value"));
            }
            let value = <flux_hir::SizedDispatch>::decode(d)?;
            self.sized_dispatch.insert(id, value);
        }
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..d.length()? {
            let id = <(FunctionId, u32)>::decode(d)?;
            if d.ids.is_strict()
                && !d
                    .ids
                    .contains_owned(flux_hir::flib::IdKind::Function, id.0.into_raw())
            {
                return Err(Error::new("signature value belongs to another crate"));
            }
            if !seen.insert(id) {
                return Err(Error::new("duplicate signature value"));
            }
            let value = <Ty>::decode(d)?;
            self.hidden.insert(id, value);
        }
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..d.length()? {
            let id = <ConstId>::decode(d)?;
            if d.ids.is_strict()
                && !d
                    .ids
                    .contains_owned(flux_hir::flib::IdKind::Const, id.into_raw())
            {
                return Err(Error::new("signature value belongs to another crate"));
            }
            if !seen.insert(id) {
                return Err(Error::new("duplicate signature value"));
            }
            let value = <Option<u128>>::decode(d)?;
            self.array_lens.insert(id, value);
        }
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..d.length()? {
            let id = <ConstId>::decode(d)?;
            if d.ids.is_strict()
                && !d
                    .ids
                    .contains_owned(flux_hir::flib::IdKind::Const, id.into_raw())
            {
                return Err(Error::new("signature value belongs to another crate"));
            }
            if !seen.insert(id) {
                return Err(Error::new("duplicate signature value"));
            }
            let value = <Ty>::decode(d)?;
            self.arg_tys.insert(id, value);
        }
        for id in Vec::<StructId>::decode(d)? {
            if d.ids.is_strict()
                && !d
                    .ids
                    .contains_owned(flux_hir::flib::IdKind::Struct, id.into_raw())
            {
                return Err(Error::new("union belongs to another crate"));
            }
            self.unions.insert(id);
        }
        self.impl_index = crate::impl_index::ImplIndex::new(items, self);
        self.traits_in_scope = crate::in_scope::TraitsInScope::new(items);
        self.builtins = Builtins::of(items);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_hir::flib::{IdKind, IdMap};
    use flux_intern::Symbol;

    fn round_trip<T: Encode + Decode>(value: &T) -> T {
        let mut e = Encoder::new();
        value.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        let result = T::decode(&mut d).unwrap();
        d.finish().unwrap();
        result
    }

    #[test]
    fn quantified_object_recipes_round_trip_and_check_their_shape() {
        let reference = Ty::new(TyKind::Ref(Mutability::Not, Ty::int(IntTy::U8)));
        let output = Ty::tuple(&[reference, reference]);
        let make = |principal: FnRegion, ty: Ty| {
            let signature = FnSigTy {
                regions: Some(FnPtrRegions::intern(
                    1,
                    1,
                    &[
                        vec![principal],
                        vec![FnRegion::Bound(0), FnRegion::Free(0)],
                        vec![],
                    ],
                    &[],
                )),
                ..FnSigTy::new(
                    &[Ty::new(TyKind::Ref(Mutability::Not, Ty::UNIT)), ty],
                    Ty::UNIT,
                    false,
                )
            };
            DynTy {
                callable: None,
                quantified: Some(Ty::new(TyKind::FnPtr(signature))),
                auto_traits: &[],
                trait_: flux_hir::TraitId::from_raw(3),
                name: Symbol::intern("Lending"),
                args: &[],
                assoc_names: crate::ty::symbol_list(&[Symbol::intern("Output")]),
                assoc_tys: Ty::list(&[output]),
            }
        };
        let valid = make(FnRegion::Bound(0), output);
        assert!(valid.valid_quantified(1));
        assert_eq!(
            round_trip(&Ty::new(TyKind::Dynamic(valid))),
            Ty::new(TyKind::Dynamic(valid))
        );
        assert!(!valid.valid_quantified(0));
        for invalid in [
            make(FnRegion::Static, output),
            make(FnRegion::Bound(0), Ty::BOOL),
        ] {
            let mut encoder = Encoder::new();
            invalid.encode(&mut encoder).unwrap();
            assert!(DynTy::decode(&mut Decoder::new(&encoder.into_bytes())).is_err());
        }
    }

    #[test]
    fn structural_types_round_trip_with_callable_regions() {
        let parameter = Ty::param(0, Symbol::intern("T"));
        let recipe = FnPtrRegions::intern(
            1,
            1,
            &[
                vec![FnRegion::Bound(0)],
                vec![FnRegion::Type(0), FnRegion::Free(0)],
            ],
            &[parameter],
        );
        let sig = FnSigTy {
            inputs_output: Ty::list(&[Ty::raw_ptr(Mutability::Not, parameter), parameter]),
            is_unsafe: true,
            abi: flux_hir::Abi::Rust,
            regions: Some(recipe),
        };
        let ty = Ty::tuple(&[
            Ty::array_of(parameter, Ty::usize_value(5)),
            Ty::new(TyKind::FnPtr(sig)),
            Ty::value(Ty::int(IntTy::U128), u128::MAX),
        ]);
        assert_eq!(round_trip(&ty), ty);
        assert_eq!(round_trip(&sig), sig);
    }

    #[test]
    fn bound_dependent_pointer_projections_round_trip_and_reject_cycles() {
        let parameter = Ty::param(0, Symbol::intern("T"));
        let alias = AliasTy {
            trait_: flux_hir::TraitId::from_raw(3),
            trait_name: Symbol::intern("Family"),
            name: Symbol::intern("Output"),
            args: Ty::list(&[parameter]),
            owner_lifetimes: 1,
            family_lifetimes: 0,
            family_args: &[],
        };
        let make = |inputs: Vec<FnRegion>| FnSigTy {
            inputs_output: Ty::list(&[Ty::alias(alias)]),
            is_unsafe: false,
            abi: flux_hir::Abi::Rust,
            regions: Some(FnPtrRegions::intern_with_projections(
                1,
                0,
                &[vec![FnRegion::Projection(0)]],
                &[parameter],
                &[(alias, inputs)],
            )),
        };
        let sig = make(vec![FnRegion::Bound(0), FnRegion::Type(0)]);
        assert_eq!(round_trip(&sig), sig);
        let replaced = sig.map(|ty| ty.subst(&[Ty::int(IntTy::U32)]));
        assert_eq!(
            replaced.regions.unwrap().projections[0].alias.args,
            &[Ty::int(IntTy::U32)]
        );
        let mut encoder = Encoder::new();
        make(vec![FnRegion::Projection(0)])
            .encode(&mut encoder)
            .unwrap();
        assert!(FnSigTy::decode(&mut Decoder::new(&encoder.into_bytes())).is_err());
    }

    #[test]
    fn item_references_are_remapped_inside_types() {
        let original = StructId::from_raw(7);
        let destination = StructId::from_raw(41);
        let ty = Ty::adt(original, Symbol::intern("Packet"), &[Ty::BOOL]);
        let mut map = IdMap::strict();
        map.insert(IdKind::Struct, 7, 0);
        let mut e = Encoder::with_ids(map);
        ty.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut map = IdMap::strict();
        map.insert(IdKind::Struct, 0, 41);
        let mut d = Decoder::with_ids(&bytes, map);
        assert_eq!(
            Ty::decode(&mut d).unwrap(),
            Ty::adt(destination, Symbol::intern("Packet"), &[Ty::BOOL])
        );
        d.finish().unwrap();
    }

    #[test]
    fn signature_selection_keeps_regions_and_hidden_types() {
        let selected = FunctionId::from_raw(3);
        let excluded = FunctionId::from_raw(5);
        let loaded = FunctionId::from_raw(11);
        let mut sigs = Signatures::default();
        let signature = FnSig {
            params: vec![Ty::BOOL].into_boxed_slice(),
            ret: Ty::int(IntTy::U8),
        };
        sigs.fns.insert(selected, signature.clone());
        sigs.fns.insert(excluded, signature.clone());
        sigs.hidden.insert((selected, 0), Ty::int(IntTy::U8));
        let regions = FnRegions {
            owner: 1,
            params: vec![vec![Region::Param(0)].into_boxed_slice()].into_boxed_slice(),
            ret: vec![Region::Static].into_boxed_slice(),
            outlives: vec![Outlives::Region(Region::Param(0), Region::Static)].into_boxed_slice(),
            ..FnRegions::default()
        };
        sigs.regions.fns.insert(selected, regions.clone());
        let mut map = IdMap::strict();
        map.insert(IdKind::Function, 3, 0);
        map.mark_owned(IdKind::Function, 3);
        let mut e = Encoder::with_ids(map);
        sigs.encode_flib(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut map = IdMap::strict();
        map.insert(IdKind::Function, 0, 11);
        map.mark_owned(IdKind::Function, 11);
        let mut d = Decoder::with_ids(&bytes, map);
        let mut decoded = Signatures::default();
        decoded
            .decode_flib_into(&mut d, &ItemTree::default())
            .unwrap();
        d.finish().unwrap();
        assert_eq!(decoded.fns.iter().count(), 1);
        assert_eq!(decoded.fns[loaded], signature);
        assert_eq!(decoded.regions.fns[loaded], regions);
        assert_eq!(decoded.hidden[&(loaded, 0)], Ty::int(IntTy::U8));
    }

    #[test]
    fn malformed_and_unchecked_types_fail() {
        assert!(Ty::decode(&mut Decoder::new(&[255])).is_err());
        assert!(Ty::decode(&mut Decoder::new(&[5, 255, 255, 255, 255])).is_err());
        for ty in [Ty::ERROR, Ty::infer(InferVar(9))] {
            assert!(ty.encode(&mut Encoder::new()).is_err());
        }
        let mut e = Encoder::new();
        Ty::array_of(Ty::BOOL, Ty::usize_value(4))
            .encode(&mut e)
            .unwrap();
        let bytes = e.into_bytes();
        for n in 0..bytes.len() {
            assert!(
                Ty::decode(&mut Decoder::new(&bytes[..n])).is_err(),
                "truncation at {n}"
            );
        }
        let mut nested = vec![14u8; 256];
        nested.push(2);
        assert!(Ty::decode(&mut Decoder::new(&nested)).is_err());
    }
}

#[cfg(test)]
mod type_form_tests {
    use super::*;
    use flux_intern::Symbol;

    #[test]
    fn every_checked_type_constructor_round_trips() {
        let name = Symbol::intern("Thing");
        let trait_ = flux_hir::TraitId::from_raw(2);
        let function = FunctionId::from_raw(4);
        let args = Ty::list(&[Ty::BOOL]);
        let alias = AliasTy {
            trait_,
            trait_name: name,
            name,
            args,
            owner_lifetimes: 1,
            family_lifetimes: 1,
            family_args: Ty::list(&[Ty::int(IntTy::U8)]),
        };
        let sig = FnSigTy::new(&[Ty::BOOL], Ty::UNIT, false);
        let forms = vec![
            Ty::int(IntTy::I128),
            Ty::float(FloatTy::F64),
            Ty::BOOL,
            Ty::CHAR,
            Ty::STR,
            Ty::NEVER,
            Ty::tuple(&[Ty::BOOL]),
            Ty::adt(StructId::from_raw(3), name, args),
            Ty::param(0, name),
            Ty::alias(alias),
            Ty::opaque(OpaqueTy {
                function,
                index: 0,
                name,
                args,
                lifetimes: 1,
            }),
            Ty::raw_ptr(Mutability::Mut, Ty::BOOL),
            Ty::new(TyKind::Ref(Mutability::Not, Ty::BOOL)),
            Ty::array_of(Ty::BOOL, Ty::usize_value(9)),
            Ty::slice(Ty::BOOL),
            Ty::new(TyKind::CStr),
            Ty::new(TyKind::Dynamic(DynTy {
                quantified: None,
                callable: Some(Ty::new(TyKind::FnPtr(FnSigTy::new(
                    &[Ty::BOOL],
                    Ty::UNIT,
                    false,
                )))),
                auto_traits: crate::ty::object_auto_traits(&[TraitRef {
                    trait_,
                    name,
                    args: &[],
                }]),
                trait_,
                name,
                args,
                assoc_names: crate::ty::symbol_list(&[name]),
                assoc_tys: args,
            })),
            Ty::new(TyKind::FnDef(FnDefTy {
                function,
                name,
                args,
                sig,
            })),
            Ty::new(TyKind::FnPtr(sig)),
            Ty::closure(
                ClosureId {
                    owner: ClosureOwner::Const(ConstId::from_raw(5)),
                    expr: flux_hir::ExprId::from_raw(6),
                },
                &[
                    ClosureKind::Fn.as_ty(),
                    Ty::tuple(&[Ty::UNIT, Ty::UNIT]),
                    Ty::UNIT,
                ],
            ),
            Ty::value(Ty::BOOL, 1),
        ];
        for ty in forms {
            let mut e = Encoder::new();
            ty.encode(&mut e).unwrap();
            let bytes = e.into_bytes();
            let mut d = Decoder::new(&bytes);
            assert_eq!(Ty::decode(&mut d).unwrap(), ty, "{ty}");
            d.finish().unwrap();
        }
    }
}

#[cfg(test)]
mod nesting_tests {
    use super::*;
    use flux_hir::flib::MAX_NESTING_DEPTH;

    fn pointer_chain(count: u32) -> Ty {
        let mut ty = Ty::BOOL;
        for _ in 0..count {
            ty = Ty::raw_ptr(Mutability::Not, ty);
        }
        ty
    }

    #[test]
    fn pointer_chains_round_trip_at_the_exact_nesting_boundary() {
        let ty = pointer_chain(MAX_NESTING_DEPTH - 1);
        let mut e = Encoder::new();
        ty.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        assert_eq!(Ty::decode(&mut d).unwrap(), ty);
        d.finish().unwrap();
        let too_deep = pointer_chain(MAX_NESTING_DEPTH);
        assert_eq!(
            too_deep.encode(&mut Encoder::new()),
            Err(Error::new("type semantic nesting limit exceeded"))
        );
        // Build the equivalent invalid wire form independently of the writer.
        let mut malformed = Vec::new();
        for wire in 0..MAX_NESTING_DEPTH {
            malformed.push(0);
            malformed.extend_from_slice(&wire.to_le_bytes());
            malformed.extend_from_slice(&[11, 0]);
        }
        malformed.push(0);
        malformed.extend_from_slice(&MAX_NESTING_DEPTH.to_le_bytes());
        malformed.push(2);
        assert_eq!(
            Ty::decode(&mut Decoder::new(&malformed)),
            Err(Error::new("metadata nesting limit exceeded"))
        );
    }

    #[test]
    fn container_and_type_nesting_share_the_same_budget() {
        let boundary = vec![pointer_chain(MAX_NESTING_DEPTH - 2)];
        let mut e = Encoder::new();
        boundary.encode(&mut e).unwrap();
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        assert_eq!(Vec::<Ty>::decode(&mut d).unwrap(), boundary);
        d.finish().unwrap();
        let excessive = vec![pointer_chain(MAX_NESTING_DEPTH - 1)];
        assert_eq!(
            excessive.encode(&mut Encoder::new()),
            Err(Error::new("metadata nesting limit exceeded"))
        );
    }
}

#[cfg(test)]
mod dag_tests {
    use super::*;
    use flux_hir::flib::MAX_NESTING_DEPTH;

    fn dag(nodes: u32) -> Ty {
        let mut ty = Ty::BOOL;
        for _ in 0..nodes {
            ty = Ty::tuple(&[ty, ty]);
        }
        ty
    }
    fn encode(ty: Ty) -> Vec<u8> {
        let mut e = Encoder::new();
        ty.encode(&mut e).unwrap();
        e.into_bytes()
    }

    #[test]
    fn shared_types_have_linear_wire_size_and_canonical_reencoding() {
        let small = encode(dag(20));
        let large = encode(dag(40));
        assert!(
            large.len() <= small.len() * 2,
            "wire lengths {} and {}",
            small.len(),
            large.len()
        );
        assert!(
            large.len() < 1024,
            "40-node DAG expanded to {} bytes",
            large.len()
        );
        let mut d = Decoder::new(&large);
        let decoded = Ty::decode(&mut d).unwrap();
        d.finish().unwrap();
        assert_eq!(decoded, dag(40));
        assert_eq!(large, encode(decoded));
        let small_key =
            crate::type_identity::stable_key(&ItemTree::default(), &[], dag(20)).unwrap();
        let large_key =
            crate::type_identity::stable_key(&ItemTree::default(), &[], dag(40)).unwrap();
        assert!(
            large_key.len() <= small_key.len() * 2,
            "stable key lengths {} and {}",
            small_key.len(),
            large_key.len()
        );
        assert!(
            large_key.len() < 2048,
            "stable type key expanded shared DAG"
        );
    }

    #[test]
    fn shared_type_bytes_match_a_fresh_process() {
        const VARIABLE: &str = "FLUX_FLIB_DAG_EXPECTED_BYTES";
        if std::env::var_os(VARIABLE).is_none() {
            for i in 0..100 {
                let _ = Ty::param(i, flux_intern::Symbol::intern(&format!("unrelated_{i}")));
            }
        }
        let wire = encode(dag(40));
        let key = crate::type_identity::stable_key(&ItemTree::default(), &[], dag(40)).unwrap();
        let actual = format!("{wire:02x?}:{key:02x?}");
        if let Ok(expected) = std::env::var(VARIABLE) {
            assert_eq!(actual, expected);
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "flib::dag_tests::shared_type_bytes_match_a_fresh_process",
            ])
            .env(VARIABLE, actual)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn unresolved_forward_cyclic_and_duplicate_type_definitions_fail() {
        let cases = [
            vec![1, 0, 0, 0, 0],                       // no definition before this reference
            vec![0, 0, 0, 0, 0, 11, 0, 1, 1, 0, 0, 0], // child references a future slot
            vec![0, 0, 0, 0, 0, 11, 0, 1, 0, 0, 0, 0], // child references its pending parent
            vec![0, 1, 0, 0, 0, 2],                    // first definition skips slot zero
        ];
        for wire in cases {
            assert!(Ty::decode(&mut Decoder::new(&wire)).is_err(), "{wire:?}");
        }
        for slot in [0u32, 1] {
            let mut wire = vec![0, 0, 0, 0, 0, 2, 0];
            wire.extend_from_slice(&slot.to_le_bytes());
            wire.push(2);
            let mut d = Decoder::new(&wire);
            Ty::decode(&mut d).unwrap();
            assert!(
                Ty::decode(&mut d).is_err(),
                "duplicate wire/semantic definition slot {slot}"
            );
        }
    }

    #[test]
    fn shallow_backreferences_do_not_bypass_semantic_depth_limit() {
        let mut ty = Ty::BOOL;
        for _ in 1..MAX_NESTING_DEPTH {
            ty = Ty::raw_ptr(Mutability::Not, ty);
        }
        let mut e = Encoder::new();
        ty.encode(&mut e).unwrap();
        assert_eq!(
            Ty::raw_ptr(Mutability::Not, ty).encode(&mut e),
            Err(Error::new("type semantic nesting limit exceeded"))
        );
        let mut wire = encode(ty);
        wire.push(0);
        wire.extend_from_slice(&MAX_NESTING_DEPTH.to_le_bytes());
        wire.extend_from_slice(&[11, 0, 1, 0, 0, 0, 0]);
        let mut d = Decoder::new(&wire);
        assert_eq!(Ty::decode(&mut d).unwrap(), ty);
        assert_eq!(
            Ty::decode(&mut d),
            Err(Error::new("type semantic nesting limit exceeded"))
        );
    }
}
