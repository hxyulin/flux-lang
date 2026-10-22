//! How values are laid out in memory: rustc's layout algorithm
//! (`rustc_abi::layout`), for aarch64, so that `size_of`, `align_of`
//! and every value in memory are what rustc makes them.
//!
//! A struct's, tuple's or closure's fields are reordered to leave little
//! padding, largest alignment first, with the field that has the most
//! invalid values (its *niche*: a reference's 0, a `bool`'s 2 to 255)
//! placed first in its group, unless the struct is `#[repr(C)]`. An
//! enum is either *tagged*, its discriminant in the smallest integer that
//! holds them (wider where the variants' first fields leave room) before
//! each variant's fields, or *niche-filling*: its largest variant's
//! fields, the other variants told apart by invalid values of the
//! largest niche among them (`Option<&T>`'s `None` is a null pointer),
//! whichever is smaller (or has the larger niche left). An enum with one
//! variant (that may hold a value) is laid out as a struct.

use std::cell::RefCell;
use std::rc::Rc;

use flux_hir::{ItemTree, StructId};
use flux_ty::{ClosureArgs, IntTy, Signatures, Ty, TyKind};
use rustc_hash::FxHashMap;

use crate::Discriminants;

/// Invalid values of a scalar a layout holds, which an enum around it
/// may use for its other variants: the scalar is `bytes` wide at
/// `offset`, and its valid values are `start..=end`, wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Niche {
    pub offset: u64,
    pub bytes: u8,
    pub start: u128,
    pub end: u128,
}

impl Niche {
    fn max(bytes: u8) -> u128 {
        if bytes >= 16 {
            u128::MAX
        } else {
            (1u128 << (8 * bytes)) - 1
        }
    }

    /// How many values are invalid.
    pub fn available(&self) -> u128 {
        self.start.wrapping_sub(self.end).wrapping_sub(1) & Self::max(self.bytes)
    }

    /// Takes `count` invalid values for an enum's other variants: the
    /// first of them, and the niche left, as rustc's `Niche::reserve`.
    fn reserve(&self, count: u128) -> Option<(u128, Niche)> {
        let max = Self::max(self.bytes);
        if count > self.available() {
            return None;
        }
        let move_start = || {
            let start = self.start.wrapping_sub(count) & max;
            Some((start, Niche { start, ..*self }))
        };
        let move_end = || {
            let start = self.end.wrapping_add(1) & max;
            let end = self.end.wrapping_add(count) & max;
            Some((start, Niche { end, ..*self }))
        };
        let distance_end_zero = max - self.end;
        let sign_extend = |value: u128| -> i128 {
            let bits = 8 * u32::from(self.bytes);
            let shift = 128 - bits;
            ((value << shift) as i128) >> shift
        };
        if count == 1 && !(self.start == 0 && self.end == 1) {
            // The one closest to zero.
            let next_up = sign_extend(self.end.wrapping_add(1) & max).unsigned_abs();
            let next_down = sign_extend(self.start.wrapping_sub(1) & max).unsigned_abs();
            if next_down <= next_up {
                move_start()
            } else {
                move_end()
            }
        } else if self.start > self.end {
            move_end()
        } else if self.start <= distance_end_zero {
            if count <= self.start {
                move_start()
            } else {
                move_end()
            }
        } else {
            let end = self.end.wrapping_add(count) & max;
            if (1..=self.end).contains(&end) {
                move_start()
            } else {
                move_end()
            }
        }
    }
}

/// How an enum's tag tells its variants apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagEncoding {
    /// The tag is the discriminant.
    Direct,
    /// The tag is a niche of variant `untagged`'s fields: the variants
    /// `first..=last` other than it are `start`, `start + 1`, ... there.
    Niche {
        untagged: u32,
        first: u32,
        last: u32,
        start: u128,
    },
}

/// Where an enum's tag is: `bytes` wide at `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub offset: u64,
    pub bytes: u8,
    /// Extended by its sign to the discriminant's type.
    pub signed: bool,
    pub encoding: TagEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// A scalar, or a pointer to a slice or `str` (its address, then
    /// its length).
    Primitive,
    /// Fields at these offsets, in declaration order.
    Struct(Box<[u64]>),
    Array {
        stride: u64,
        count: u64,
    },
    /// An enum: each variant's fields' offsets, from the enum's start,
    /// and its tag, or none if only variant `present` may hold a value.
    Enum {
        tag: Option<Tag>,
        present: u32,
        variants: Box<[Box<[u64]>]>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub size: u64,
    pub align: u64,
    pub niche: Option<Niche>,
    pub uninhabited: bool,
    /// The layout is rustc's: not for a type flux lays out otherwise (a
    /// `&CStr`, one pointer here and two in rustc).
    pub exact: bool,
    pub shape: Shape,
}

impl Layout {
    fn scalar(bytes: u8, valid: Option<(u128, u128)>) -> Layout {
        Layout {
            size: u64::from(bytes),
            align: u64::from(bytes),
            niche: valid
                .map(|(start, end)| Niche {
                    offset: 0,
                    bytes,
                    start,
                    end,
                })
                .filter(|n| n.available() > 0),
            uninhabited: false,
            exact: true,
            shape: Shape::Primitive,
        }
    }

    fn zst(uninhabited: bool) -> Layout {
        Layout {
            size: 0,
            align: 1,
            niche: None,
            uninhabited,
            exact: true,
            shape: Shape::Struct(Box::new([])),
        }
    }

    fn is_1zst(&self) -> bool {
        self.size == 0 && self.align == 1
    }

    /// The offset of field `field` of variant `variant` (0 for a struct
    /// or tuple).
    pub fn field_offset(&self, variant: u32, field: u32) -> u64 {
        match &self.shape {
            Shape::Struct(offsets) => offsets.get(field as usize).copied().unwrap_or(0),
            Shape::Enum { variants, .. } => variants
                .get(variant as usize)
                .and_then(|v| v.get(field as usize))
                .copied()
                .unwrap_or(0),
            Shape::Array { stride, .. } => stride * u64::from(field),
            Shape::Primitive => 8 * u64::from(field),
        }
    }
}

/// Whether fields are laid out as written, rather than reordered.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    AlwaysSized,
    /// A tuple, whose last element stays last: it may be unsized.
    MaybeUnsized,
    /// After a prefix of this size and alignment: an enum's tag.
    Prefixed(u64, u64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bias {
    Start,
    End,
}

/// A struct's fields laid out: their offsets, and the struct's size,
/// alignment and largest niche.
struct Univariant {
    offsets: Vec<u64>,
    size: u64,
    align: u64,
    niche: Option<Niche>,
    uninhabited: bool,
}

fn align_to(value: u64, align: u64) -> u64 {
    value.next_multiple_of(align.max(1))
}

/// Computes and caches the layouts of monomorphic types.
pub struct Layouts<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    discriminants: &'a Discriminants,
    cache: RefCell<FxHashMap<Ty, Rc<Layout>>>,
}

impl<'a> Layouts<'a> {
    pub fn new(
        items: &'a ItemTree,
        sigs: &'a Signatures,
        discriminants: &'a Discriminants,
    ) -> Self {
        Layouts {
            items,
            sigs,
            discriminants,
            cache: RefCell::new(FxHashMap::default()),
        }
    }

    pub fn of(&self, ty: Ty) -> Rc<Layout> {
        if let Some(layout) = self.cache.borrow().get(&ty) {
            return layout.clone();
        }
        let layout = Rc::new(self.compute(ty));
        self.cache.borrow_mut().insert(ty, layout.clone());
        layout
    }

    /// The discriminant of `variant` of `strukt`, as bits of its type.
    pub fn discriminant(&self, strukt: StructId, variant: u32) -> u128 {
        crate::discriminant_bits(self.items, self.discriminants, strukt, variant)
            .unwrap_or(u128::from(variant))
    }

    fn compute(&self, ty: Ty) -> Layout {
        const PTR_MAX: u128 = u64::MAX as u128;
        match ty.kind() {
            TyKind::Int(int) => {
                let bytes = (int.bits() / 8) as u8;
                Layout::scalar(bytes, None)
            }
            TyKind::Bool => Layout::scalar(1, Some((0, 1))),
            TyKind::Char => Layout::scalar(4, Some((0, 0x10_FFFF))),
            TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee) => {
                let nonnull = matches!(ty.kind(), TyKind::Ref(..));
                let valid = nonnull.then_some((1, PTR_MAX));
                match pointee.kind() {
                    TyKind::Slice(_) | TyKind::Str => {
                        let mut layout = Layout::scalar(8, valid);
                        layout.size = 16;
                        layout
                    }
                    // The data pointer, then the vtable's, which is never
                    // null: a raw pointer's niche is there.
                    TyKind::Dynamic(_) => {
                        let mut layout = Layout::scalar(8, Some((1, PTR_MAX)));
                        layout.size = 16;
                        if !nonnull && let Some(niche) = &mut layout.niche {
                            niche.offset = 8;
                        }
                        layout
                    }
                    // rustc's `&CStr` has its length too.
                    TyKind::CStr => Layout {
                        exact: false,
                        ..Layout::scalar(8, valid)
                    },
                    _ => Layout::scalar(8, valid),
                }
            }
            TyKind::FnPtr(_) => Layout::scalar(8, Some((1, PTR_MAX))),
            TyKind::FnDef(_) => Layout::zst(false),
            TyKind::Never => Layout::zst(true),
            TyKind::Tuple([]) => Layout::zst(false),
            TyKind::Tuple(elements) => self.univariant_layout(elements, true, Kind::MaybeUnsized),
            TyKind::Closure(_, args) => self.univariant_layout(
                ClosureArgs::split(args).upvar_types(),
                true,
                Kind::AlwaysSized,
            ),
            TyKind::Array(elem, len) => {
                let elem = self.of(elem);
                let count = len.as_usize().unwrap_or(0);
                Layout {
                    size: elem.size * count,
                    align: elem.align,
                    niche: if count == 0 { None } else { elem.niche },
                    uninhabited: count != 0 && elem.uninhabited,
                    exact: elem.exact,
                    shape: Shape::Array {
                        stride: elem.size,
                        count,
                    },
                }
            }
            TyKind::Adt(adt, _) => self.adt(ty, adt.strukt),
            // Unsized, or not a type of a value: never laid out in memory
            // on their own.
            TyKind::Slice(elem) => {
                let elem = self.of(elem);
                Layout {
                    size: 0,
                    align: elem.align,
                    niche: None,
                    uninhabited: false,
                    exact: true,
                    shape: Shape::Array {
                        stride: elem.size,
                        count: 0,
                    },
                }
            }
            TyKind::Str | TyKind::CStr | TyKind::Dynamic(_) => Layout {
                shape: Shape::Array {
                    stride: 1,
                    count: 0,
                },
                ..Layout::zst(false)
            },
            TyKind::Param(_)
            | TyKind::Alias(_)
            | TyKind::Opaque(_)
            | TyKind::Infer(_)
            | TyKind::Value(_)
            | TyKind::Error => Layout::zst(false),
        }
    }

    fn univariant_layout(&self, fields: &[Ty], reorder: bool, kind: Kind) -> Layout {
        let layouts: Vec<Rc<Layout>> = fields.iter().map(|&f| self.of(f)).collect();
        let refs: Vec<&Layout> = layouts.iter().map(|l| &**l).collect();
        let u = univariant(&refs, reorder, kind);
        Layout {
            size: u.size,
            align: u.align,
            niche: u.niche,
            uninhabited: u.uninhabited,
            exact: refs.iter().all(|l| l.exact),
            shape: Shape::Struct(u.offsets.into()),
        }
    }

    fn adt(&self, ty: Ty, strukt: StructId) -> Layout {
        let items = self.items;
        let data = &items.structs[strukt];
        let lang = |name: &str| flux_ty::is_lang_struct(items, strukt, name);
        let Some(variants) = self.sigs.variants(ty) else {
            let fields = self.sigs.fields(ty).unwrap_or_default();
            // A struct's last field is unsized only if its type may be,
            // which flux's never may.
            let mut layout = self.univariant_layout(fields, !data.repr_c, Kind::AlwaysSized);
            if lang("non_null") {
                // `#[rustc_layout_scalar_valid_range_start(1)]`.
                layout.niche = Some(Niche {
                    offset: 0,
                    bytes: 8,
                    start: 1,
                    end: u64::MAX as u128,
                });
            }
            if lang("alignment") {
                // rustc's `AlignmentEnum`: the powers of two.
                layout.niche = Some(Niche {
                    offset: 0,
                    bytes: 8,
                    start: 1,
                    end: 1 << 63,
                });
            }
            // rustc's `UnsafeCell`, which `Cell` and `RefCell`'s value
            // are, hides its value's niche.
            if lang("cell") || lang("ref_cell") {
                layout.niche = None;
            }
            if lang("maybe_uninit") {
                // A union, which has no niche.
                layout.niche = None;
                layout.uninhabited = false;
            }
            return layout;
        };
        let repr_int = data.repr.and_then(IntTy::from_name);
        let inhibit = data.repr_c || repr_int.is_some();
        let layouts: Vec<Vec<Rc<Layout>>> = variants
            .iter()
            .map(|fields| fields.iter().map(|&f| self.of(f)).collect())
            .collect();
        let exact = layouts.iter().flatten().all(|l| l.exact);
        let absent = |fields: &[Rc<Layout>]| {
            fields.iter().any(|f| f.uninhabited) && fields.iter().all(|f| f.is_1zst())
        };
        let present: Vec<u32> = (0..layouts.len() as u32)
            .filter(|&v| inhibit || !absent(&layouts[v as usize]))
            .collect();
        let Some(&first_present) = present.first() else {
            return Layout::zst(true);
        };
        let all_offsets = |kind: Kind| -> Vec<Box<[u64]>> {
            layouts
                .iter()
                .map(|fields| {
                    let refs: Vec<&Layout> = fields.iter().map(|l| &**l).collect();
                    univariant(&refs, !inhibit, kind).offsets.into()
                })
                .collect()
        };
        if present.len() == 1 && !inhibit {
            let refs: Vec<&Layout> = layouts[first_present as usize]
                .iter()
                .map(|l| &**l)
                .collect();
            let u = univariant(&refs, true, Kind::AlwaysSized);
            let mut offsets = all_offsets(Kind::AlwaysSized);
            offsets[first_present as usize] = u.offsets.into();
            return Layout {
                size: u.size,
                align: u.align,
                niche: u.niche,
                uninhabited: u.uninhabited,
                exact,
                shape: Shape::Enum {
                    tag: None,
                    present: first_present,
                    variants: offsets.into(),
                },
            };
        }
        let mut layout = self.enum_layout(strukt, &layouts, data.repr_c, repr_int);
        layout.exact = exact;
        layout
    }

    /// rustc's `layout_of_enum`.
    fn enum_layout(
        &self,
        strukt: StructId,
        layouts: &[Vec<Rc<Layout>>],
        repr_c: bool,
        repr_int: Option<IntTy>,
    ) -> Layout {
        let inhibit = repr_c || repr_int.is_some();
        let reorder = !inhibit;
        let absent = |fields: &[Rc<Layout>]| {
            fields.iter().any(|f| f.uninhabited) && fields.iter().all(|f| f.is_1zst())
        };
        let refs_of = |v: usize| -> Vec<&Layout> { layouts[v].iter().map(|l| &**l).collect() };
        let niche_filling = || -> Option<Layout> {
            if inhibit || layouts.len() < 2 {
                return None;
            }
            let mut align = 1;
            let mut variants: Vec<Univariant> = (0..layouts.len())
                .map(|v| univariant(&refs_of(v), reorder, Kind::AlwaysSized))
                .collect();
            for v in &variants {
                align = align.max(v.align);
            }
            // The last of the largest, as `max_by_key` finds it.
            let largest = variants.iter().enumerate().fold(0, |best, (i, v)| {
                if v.size >= variants[best].size {
                    i
                } else {
                    best
                }
            });
            let needs_disc = |v: usize| v != largest && !absent(&layouts[v]);
            let first = (0..layouts.len()).find(|&v| needs_disc(v))?;
            let last = (0..layouts.len()).rev().find(|&v| needs_disc(v))?;
            let count = (last - first + 1) as u128;
            let niche = variants[largest].niche?;
            let (start, reserved) = niche.reserve(count)?;
            let size = align_to(variants[largest].size, align);
            for (i, v) in variants.iter_mut().enumerate() {
                if i == largest {
                    continue;
                }
                v.niche = None;
                if v.size <= niche.offset {
                    continue;
                }
                let this_offset = align_to(niche.offset + u64::from(niche.bytes), v.align);
                if this_offset + v.size > size {
                    return None;
                }
                for offset in &mut v.offsets {
                    *offset += this_offset;
                }
                v.size += this_offset;
            }
            let uninhabited = variants.iter().all(|v| v.uninhabited);
            Some(Layout {
                size,
                align,
                niche: Some(reserved).filter(|n| n.available() > 0),
                uninhabited,
                exact: true,
                shape: Shape::Enum {
                    tag: Some(Tag {
                        offset: niche.offset,
                        bytes: niche.bytes,
                        signed: false,
                        encoding: TagEncoding::Niche {
                            untagged: largest as u32,
                            first: first as u32,
                            last: last as u32,
                            start,
                        },
                    }),
                    present: largest as u32,
                    variants: variants
                        .into_iter()
                        .map(|v| v.offsets.into_boxed_slice())
                        .collect(),
                },
            })
        };
        let niche_layout = niche_filling();

        // The tagged layout.
        let discr_int = repr_int.unwrap_or(IntTy::Isize);
        let discr_bits = discr_int.bits();
        let necessary: Vec<u128> = (0..layouts.len())
            .filter(|&v| repr_c || layouts[v].iter().all(|f| !f.uninhabited))
            .map(|v| self.discriminant(strukt, v as u32))
            .collect();
        let (min_negative, max_positive): (i128, u128) = if discr_int.is_signed() {
            necessary.iter().fold((0, 0), |(min, max), &bits| {
                let shift = 128 - discr_bits;
                let value = ((bits << shift) as i128) >> shift;
                match u128::try_from(value) {
                    Ok(v) => (min, max.max(v)),
                    Err(_) => (min.min(value), max),
                }
            })
        } else {
            (0, necessary.iter().copied().max().unwrap_or(0))
        };
        let (min_bytes, signed) = discr_range(min_negative, max_positive, repr_int, repr_c);
        let mut prefix_align = u64::from(min_bytes);
        if repr_c {
            for l in layouts.iter().flatten() {
                prefix_align = prefix_align.max(l.align);
            }
        }
        let mut align = 1;
        let mut size = 0;
        let mut start_align = 256;
        let mut variants: Vec<Univariant> = Vec::new();
        for v in 0..layouts.len() {
            let fields = refs_of(v);
            let u = univariant(
                &fields,
                reorder,
                Kind::Prefixed(u64::from(min_bytes), prefix_align),
            );
            // The first field that cannot move to make room for a wider
            // tag.
            let mut by_offset: Vec<usize> = (0..fields.len()).collect();
            by_offset.sort_by_key(|&i| u.offsets[i]);
            if let Some(&i) = by_offset.iter().find(|&&i| !fields[i].is_1zst()) {
                start_align = start_align.min(fields[i].align);
            }
            size = size.max(u.size);
            align = align.max(u.align);
            variants.push(u);
        }
        size = align_to(size, align);
        let mut tag_bytes = if inhibit {
            min_bytes
        } else {
            match start_align {
                1 | 2 | 4 | 8 | 16 => start_align as u8,
                _ => min_bytes,
            }
        };
        if tag_bytes <= min_bytes {
            tag_bytes = min_bytes;
        } else {
            let old = u64::from(min_bytes);
            let new = u64::from(tag_bytes);
            for v in &mut variants {
                for offset in &mut v.offsets {
                    if *offset <= old {
                        *offset = new;
                    }
                }
                if v.size <= old {
                    v.size = new;
                }
            }
        }
        let max = Niche::max(tag_bytes);
        let tags: Vec<u128> = necessary.iter().map(|&d| d & max).collect();
        let (start, end) = smallest_range_containing(&tags, tag_bytes).unwrap_or((0, 0));
        let tag_niche = Niche {
            offset: 0,
            bytes: tag_bytes,
            start,
            end,
        };
        let uninhabited = variants.iter().all(|v| v.uninhabited);
        let tagged = Layout {
            size,
            align,
            niche: Some(tag_niche).filter(|n| n.available() > 0),
            uninhabited,
            exact: true,
            shape: Shape::Enum {
                tag: Some(Tag {
                    offset: 0,
                    bytes: tag_bytes,
                    signed,
                    encoding: TagEncoding::Direct,
                }),
                present: 0,
                variants: variants
                    .into_iter()
                    .map(|v| v.offsets.into_boxed_slice())
                    .collect(),
            },
        };
        match niche_layout {
            Some(niche) => {
                let available = |l: &Layout| l.niche.map_or(0, |n| n.available());
                if tagged.size > niche.size
                    || tagged.size == niche.size && available(&tagged) < available(&niche)
                {
                    niche
                } else {
                    tagged
                }
            }
            None => tagged,
        }
    }
}

/// The integer a tag needs for discriminants from `min_negative` to
/// `max_positive`, its size in bytes, and whether it is signed: rustc's
/// `discr_range_of_repr`.
fn discr_range(
    min_negative: i128,
    max_positive: u128,
    repr_int: Option<IntTy>,
    repr_c: bool,
) -> (u8, bool) {
    if let Some(int) = repr_int {
        return ((int.bits() / 8) as u8, int.is_signed());
    }
    let fit_unsigned = |x: u128| -> u8 {
        match x {
            0..=0xff => 1,
            0x100..=0xffff => 2,
            0x1_0000..=0xffff_ffff => 4,
            0x1_0000_0000..=0xffff_ffff_ffff_ffff => 8,
            _ => 16,
        }
    };
    let fit_signed = |x: i128| -> u8 {
        if (-0x80..=0x7f).contains(&x) {
            1
        } else if (-0x8000..=0x7fff).contains(&x) {
            2
        } else if (-0x8000_0000..=0x7fff_ffff).contains(&x) {
            4
        } else if (i128::from(i64::MIN)..=i128::from(i64::MAX)).contains(&x) {
            8
        } else {
            16
        }
    };
    let unsigned_fit = fit_unsigned((min_negative as u128).max(max_positive));
    let signed_fit = fit_signed(min_negative).max(fit_signed(max_positive as i128));
    let at_least = if repr_c { 4 } else { 1 };
    if unsigned_fit <= signed_fit {
        (unsigned_fit.max(at_least), false)
    } else {
        (signed_fit.max(at_least), true)
    }
}

/// rustc's `WrappingRange::smallest_range_containing`.
fn smallest_range_containing(values: &[u128], bytes: u8) -> Option<(u128, u128)> {
    let min = *values.iter().min()?;
    let max = *values.iter().max()?;
    let half = 1u128 << (8 * u32::from(bytes) - 1);
    if max - min <= half {
        return Some((min, max));
    }
    let mask = Niche::max(bytes);
    let width = |(start, end): (u128, u128)| end.wrapping_sub(start) & mask;
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mut best = (min, max);
    for pair in sorted.windows(2) {
        let (end, start) = (pair[0], pair[1]);
        if start != end && width((start, end)) < width(best) {
            best = (start, end);
        }
    }
    Some(best)
}

/// rustc's `univariant`: the layout biased to put the largest niche at
/// the start, unless putting it at the end leaves more room before it.
fn univariant(fields: &[&Layout], reorder: bool, kind: Kind) -> Univariant {
    let layout = univariant_biased(fields, reorder, kind, Bias::Start);
    if kind != Kind::MaybeUnsized
        && let Some(niche) = layout.niche
    {
        let head = niche.offset;
        let len = u64::from(niche.bytes);
        let tail = layout.size - head - len;
        if fields.len() > 1 && head != 0 && tail > 0 {
            let alt = univariant_biased(fields, reorder, kind, Bias::End);
            if let Some(alt_niche) = alt.niche {
                let alt_head = alt_niche.offset;
                if alt_head > head && alt_head > tail {
                    return alt;
                }
            }
        }
    }
    layout
}

/// rustc's `univariant_biased`.
fn univariant_biased(fields: &[&Layout], reorder: bool, kind: Kind, bias: Bias) -> Univariant {
    let mut order: Vec<usize> = (0..fields.len()).collect();
    // A tuple's last element stays last.
    let end = if kind == Kind::MaybeUnsized {
        fields.len() - 1
    } else {
        fields.len()
    };
    if reorder && fields.len() > 1 {
        let max_field_align = fields[..end].iter().map(|f| f.align).max().unwrap_or(1);
        let largest_niche_size = fields[..end]
            .iter()
            .filter_map(|f| f.niche)
            .map(|n| n.available())
            .max()
            .unwrap_or(0);
        let group = |f: &Layout| -> u64 {
            let niche_size = f.niche.map_or(0, |n| n.available());
            let size_as_align = f.align.max(f.size).trailing_zeros();
            let key = if largest_niche_size > 0 {
                match bias {
                    Bias::Start => max_field_align.trailing_zeros().min(size_as_align),
                    Bias::End if niche_size == largest_niche_size => f.align.trailing_zeros(),
                    Bias::End => size_as_align,
                }
            } else {
                size_as_align
            };
            u64::from(key)
        };
        let optimizing = &mut order[..end];
        match kind {
            Kind::AlwaysSized | Kind::MaybeUnsized => optimizing.sort_by_key(|&i| {
                let f = fields[i];
                let niche_size = f.niche.map_or(0, |n| n.available());
                let niche_key = match bias {
                    Bias::Start => !niche_size,
                    Bias::End => niche_size,
                };
                let inner = match bias {
                    Bias::Start => f.niche.map_or(0, |n| n.offset),
                    Bias::End => f
                        .niche
                        .map_or(0, |n| !(f.size - u64::from(n.bytes) - n.offset)),
                };
                (std::cmp::Reverse(group(f)), niche_key, inner)
            }),
            Kind::Prefixed(..) => optimizing.sort_by_key(|&i| {
                let f = fields[i];
                (group(f), f.niche.map_or(0, |n| n.available()))
            }),
        }
    }
    let mut align = 1;
    let mut offset = 0;
    if let Kind::Prefixed(size, prefix_align) = kind {
        align = align.max(prefix_align);
        offset = align_to(size, prefix_align);
    }
    let mut offsets = vec![0; fields.len()];
    let mut niche: Option<Niche> = None;
    let mut niche_available = 0;
    for &i in &order {
        let f = fields[i];
        offset = align_to(offset, f.align);
        align = align.max(f.align);
        offsets[i] = offset;
        if let Some(mut n) = f.niche {
            let available = n.available();
            let prefer = match bias {
                Bias::Start => available > niche_available,
                Bias::End => available >= niche_available,
            };
            if prefer {
                niche_available = available;
                n.offset += offset;
                niche = Some(n);
            }
        }
        offset += f.size;
    }
    Univariant {
        offsets,
        size: align_to(offset, align),
        align,
        niche,
        uninhabited: fields.iter().any(|f| f.uninhabited),
    }
}
