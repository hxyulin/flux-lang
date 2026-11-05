//! Structural item records. Every item reference uses the enclosing codec map.

use super::{Decode, Decoder, Encode, Encoder, Error};
use crate::def_map::{Binding, BindingKind, Def, Scope};
use crate::item_tree::{
    Abi, AnonConst, ArrayLen, ArrayLenKind, AssocBinding, AssocItem, Block, Bound, Const, Derive,
    Discriminant, DynRef, Field, Function, GenericArgs, GenericParam, Generics, Impl, ImplTarget,
    ImplTraitRef, ImplType, InlineHint, ItemLoc, LifetimeParam, LifetimeRef, Module,
    OutlivesPredicate, Owner, Param, ParamKind, Path, PathAnchor, QSelf, Struct, StructKind, Trait,
    TraitType, TypeAlias, TypeOutlives, TypeRef, Use, UseCapture, UseKind, Variant, Visibility,
    WherePredicate,
};

macro_rules! record {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl Encode for $ty {
            fn encode(&self,e:&mut Encoder)->Result<(),Error>{e.nested(|e|{$(self.$field.encode(e)?;)*Ok(())})}
        }
        impl Decode for $ty {
            fn decode(d:&mut Decoder<'_>)->Result<Self,Error>{d.nested(|d|Ok(Self{$($field:Decode::decode(d)?,)*}))}
        }
    };
}
macro_rules! variants {
    ($ty:ty {$($tag:literal => $variant:ident $(($($arg:ident),*))? $({$($field:ident),*})?),* $(,)?}) => {
        impl Encode for $ty {fn encode(&self,e:&mut Encoder)->Result<(),Error>{e.nested(|e|{match self{$(Self::$variant $(($($arg),*))? $({$($field),*})?=>{($tag as u8).encode(e)?;$($($arg.encode(e)?;)*)?$($($field.encode(e)?;)*)?})*}Ok(())})}}
        impl Decode for $ty {fn decode(d:&mut Decoder<'_>)->Result<Self,Error>{d.nested(|d|match u8::decode(d)?{$($tag=>Ok(Self::$variant $(( $( {let $arg=Decode::decode(d)?; $arg} ),* ))? $({$($field:Decode::decode(d)?),*})?),)*_=>Err(Error("invalid item variant"))})}}
    };
}

record!(crate::SizedDispatch {
    selector,
    parameters,
    sized,
    fallback
});
record!(Module {
    name,
    parent,
    krate,
    file,
    dir,
    vis,
    def_range,
    block
});
record!(Block { function, node });
record!(ItemLoc { module, file, vis });
record!(Impl {
    generics,
    trait_ref,
    trait_,
    is_unsafe,
    self_ty,
    self_range,
    def_range,
    module,
    file,
    target,
    functions,
    consts,
    types
});
record!(Const {
    name,
    ty,
    ty_range,
    loc,
    owner,
    node,
    has_body,
    def_range,
    item_range,
    discriminant,
    is_static,
    extern_symbol,
    specialization,
    anon,
    macro_scope
});
record!(AnonConst {
    self_adt,
    function,
    item,
    index,
    is_arg
});
record!(TypeAlias {
    name,
    generics,
    ty,
    ty_range,
    loc,
    name_range,
    def_range
});
record!(Trait {
    name,
    generics,
    supertraits,
    loc,
    is_unsafe,
    lang,
    on_unimplemented,
    functions,
    consts,
    types,
    name_range,
    def_range
});
record!(TraitType {
    maybe_unsized,
    name,
    generics,
    bounds,
    lifetime_bounds,
    name_range,
    item_range
});
record!(ImplType {
    name,
    generics,
    ty,
    ty_range,
    name_range,
    item_range
});
record!(Bound {
    bound_lifetimes,
    path,
    range
});
record!(WherePredicate {
    ty,
    ty_range,
    bound
});
record!(Struct {
    non_exhaustive,
    is_union,
    name,
    generics,
    kind,
    fields,
    variants,
    repr,
    repr_c,
    repr_transparent,
    repr_align,
    repr_pack,
    loc,
    name_range,
    def_range
});
record!(Variant {
    non_exhaustive,
    name,
    kind,
    fields,
    discriminant,
    name_range,
    def_range
});
record!(Field {
    name,
    ty,
    vis,
    range,
    name_range,
    def_range
});
record!(Function {
    name,
    generics,
    params,
    ret,
    ret_range,
    loc,
    is_unsafe,
    target_features,
    is_extern,
    link_name,
    abi,
    is_const,
    inherits_overflow_checks,
    const_unstable,
    precise_live_drops,
    intrinsic,
    legacy_const_generic,
    track_caller,
    inline,
    cold,
    do_not_const_check,
    is_variadic,
    owner,
    has_self,
    has_body,
    node,
    name_range,
    def_range,
    sig_range,
    item_range,
    param_list,
    opaques,
    derived,
    ctor,
    macro_scope
});
record!(Param {
    ty,
    name,
    range,
    whole
});
record!(Generics {
    params,
    lifetimes,
    predicates,
    outlives,
    type_outlives,
    range
});
record!(LifetimeRef { name, range });
record!(LifetimeParam {
    may_dangle,
    name,
    range
});
record!(OutlivesPredicate { longer, shorter });
record!(TypeOutlives {
    ty,
    ty_range,
    lifetime
});
record!(GenericParam {
    may_dangle,
    name,
    range,
    default,
    synthetic,
    kind,
    maybe_unsized
});
record!(Use {
    module,
    file,
    vis,
    path,
    kind,
    range
});
record!(Path {
    edition,
    anchor,
    qself,
    segments,
    ranges,
    anchor_ranges,
    args
});
record!(QSelf {
    ty,
    ty_range,
    trait_,
    range
});
record!(GenericArgs {
    lifetimes,
    types,
    ranges,
    bindings,
    range,
    parenthesized,
    bound_lifetimes
});
record!(AssocBinding {
    name,
    args,
    ty,
    name_range,
    ty_range
});
record!(ImplTraitRef {
    maybe_unsized,
    edition,
    bounds,
    lifetimes,
    captures,
    use_range,
    range,
    name
});
record!(DynRef {
    bounds,
    lifetime,
    range
});
record!(ArrayLen { kind, range });
variants!(Visibility {0=>Public,1=>Crate(id),2=>Module(id)});
variants!(crate::NativeKind {0=>Dylib,1=>Static,2=>Framework});
record!(crate::NativeLibrary { name, kind });
variants!(Owner {0=>Impl(id),1=>Trait(id)});
variants!(AssocItem {0=>Function(id),1=>Const(id)});
variants!(ImplTarget {0=>Struct(id),1=>Primitive(name),2=>Object(id)});
variants!(Discriminant {0=>Value(value),1=>Const{konst,offset}});
variants!(StructKind {0=>Record,1=>Tuple,2=>Unit});
variants!(InlineHint {0=>Hint,1=>Always,2=>Never});
variants!(Abi {0=>Rust,1=>C,2=>CUnwind,3=>System,4=>SystemUnwind,5=>Efiapi});
variants!(Derive {0=>Clone,1=>Copy,2=>PartialEq,3=>Eq,4=>PartialOrd,5=>Ord,6=>Default,7=>Debug,8=>Hash});
variants!(ParamKind {0=>Type,1=>Const{ty,ty_range}});
variants!(UseKind {0=>Named(name),1=>Anonymous{module_only},2=>Module(name),3=>Glob});
variants!(PathAnchor {0=>Plain,1=>Crate,2=>DefinitionCrate(id),3=>SelfModule,4=>Super(count),5=>Global});
variants!(UseCapture {0=>Lifetime(value),1=>Param(name,range)});
variants!(ArrayLenKind {0=>Lit{value,suffix},1=>NegLit{value,suffix},2=>Const(id),3=>Path(path),4=>Bool(value),5=>Char(value),6=>Anon(file,node),7=>Error});
variants!(TypeRef {0=>Path(path),1=>Tuple(types),2=>Never,3=>Ptr{mutable,pointee},4=>Ref{lifetime,amp,mutable,pointee},5=>Array{elem,len},6=>Slice(ty),7=>Infer,8=>FnPtr{bound_lifetimes,params,ret,is_unsafe,abi},9=>ImplTrait(value),10=>Dyn(value),11=>ImplParam(index),12=>Opaque(index),13=>Const(value),14=>Error});
variants!(BindingKind {0=>Item,1=>Import,2=>Glob,3=>AmbiguousGlob});
variants!(Def {0=>Module(id),1=>Struct(id),2=>Trait(id),3=>Function(id),4=>Const(id),5=>Variant(id,index),6=>TypeAlias(id),7=>Static(id),8=>Macro(id),9=>Primitive(name)});
record!(Binding {
    def,
    vis,
    kind,
    file,
    range
});
record!(Scope {
    types,
    values,
    macros,
    anonymous_traits
});
variants!(crate::AsmRegister {0=>Reg,1=>Vreg,2=>InputReg,3=>InputVreg});
variants!(crate::AsmTemplatePiece {0=>Text(text),1=>Operand{index,modifier}});
record!(crate::AsmOptions {
    nomem,
    readonly,
    preserves_flags,
    nostack,
    pure
});
