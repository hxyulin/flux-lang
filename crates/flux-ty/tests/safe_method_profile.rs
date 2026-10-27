//! Admission reconstructs safe Copy-by-value defaults and generic contracts.
use flux_hir::{Hir, TraitId};
use flux_ty::regions::{CallbackBinder, Region, safe_method_trait_profile as profile};
use flux_ty::{Signatures, Ty};

const SOURCE: &str = "unsafe trait Pod:Copy+'static{}unsafe impl Pod for u8{}fn from_bytes<T:Pod>(x:&[u8])->Result<&T,()>{if x.len()<core::mem::size_of::<T>()||x.as_ptr()as usize%core::mem::align_of::<T>()!=0{return Err(());}Ok(unsafe{&*x.as_ptr().cast::<T>()})}trait Reader<'data>:Clone+Copy{fn bytes(self,offset:u64,size:u64)->Result<&'data[u8],()>;fn read<T:Pod>(self,offset:&mut u64)->Result<&'data T,()>{let size=core::mem::size_of::<T>()as u64;let data=self.bytes(*offset,size)?;*offset+=size;from_bytes(data)}fn read_at<T:Pod>(self,mut offset:u64)->Result<&'data T,()>{self.read(&mut offset)}}";

fn resolve(source: &str) -> (Hir, Signatures, TraitId) {
    let (_, root, hir) = flux_hir::lower_text("copy-profile.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (sigs, diagnostics) = flux_ty::signatures(&hir.items);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let id = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.loc.file == root && !t.generics.lifetimes.is_empty())
        .unwrap()
        .0;
    (hir, sigs, id)
}

#[test]
fn represented_defaults_and_type_parameters_are_admitted() {
    for source in [
        SOURCE.to_owned(),
        SOURCE
            .replace("'data", "'input")
            .replace('T', "Element")
            .replace("Reader", "Scanner")
            .replace("read", "fetch"),
    ] {
        let (hir, sigs, id) = resolve(&source);
        assert_eq!(profile(&hir, &sigs, id), Ok(()));
        assert!(flux_ty::check_file(&hir).diagnostics.is_empty());
    }
}

#[test]
fn corrupted_copy_method_metadata_fails_closed() {
    let (hir, sigs, owner) = resolve(SOURCE);
    let method = hir.items.traits[owner]
        .functions
        .iter()
        .copied()
        .find(|&id| hir.items.functions[id].name.as_str() == "read")
        .unwrap();
    for corruption in 0..16 {
        let mut bad = sigs.clone();
        if corruption < 12 {
            let mut row = bad.regions.fns[method].clone();
            match corruption {
                0 => row.owner = 0,
                1 => row.names = Box::default(),
                2 => row.params = Box::default(),
                3 => row.params[1] = Box::default(),
                4 => row.params[1][0] = Region::Static,
                5 => row.ret[0] = Region::Param(1),
                6 => row.outlives = Box::default(),
                7 => row.has_bounds = true,
                8 => row.callback_binder = CallbackBinder::Unclassified,
                9 => row.bound_selfs = Box::default(),
                10 => row.bound_traits = Box::default(),
                _ => row.projections = Box::new([Box::new([Region::Static])]),
            }
            bad.regions.fns.insert(method, row);
        } else {
            match corruption {
                12 => {
                    bad.fn_predicates.insert(method, Box::default());
                }
                13 => {
                    let mut bounds = bad.fn_predicates[method].to_vec();
                    bounds[0].self_ty = Ty::param(0, flux_intern::kw::SelfUpper);
                    bad.fn_predicates.insert(method, bounds.into());
                }
                14 => {
                    let mut signature = bad.fns[method].clone();
                    signature.ret = Ty::BOOL;
                    bad.fns.insert(method, signature);
                }
                _ => {
                    let mut owner_row = bad.regions.traits[owner].clone();
                    owner_row.names = Box::default();
                    bad.regions.traits.insert(owner, owner_row);
                }
            }
        }
        assert!(
            profile(&hir, &bad, owner).is_err(),
            "corruption {corruption}"
        );
    }
}

#[test]
fn borrowed_default_elision_is_reconstructed() {
    let source = "trait Reader<'data>{fn data(&self)->&'data u32;fn get(&self)->&'data u32{self.data()}fn choose(&self,_other:&u32)->&Self{self}}";
    let (hir, sigs, owner) = resolve(source);
    assert_eq!(profile(&hir, &sigs, owner), Ok(()));
    let method = hir.items.traits[owner]
        .functions
        .iter()
        .copied()
        .find(|&id| hir.items.functions[id].name.as_str() == "choose")
        .unwrap();
    let mut bad = sigs.clone();
    let mut row = bad.regions.fns[method].clone();
    assert_ne!(row.params[0], row.params[1]);
    row.ret = row.params[1].clone();
    bad.regions.fns.insert(method, row);
    assert!(profile(&hir, &bad, owner).is_err());
}
