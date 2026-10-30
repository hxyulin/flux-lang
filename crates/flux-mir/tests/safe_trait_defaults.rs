//! Default methods preserve the owner's data lifetime and local generic bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

const PREFIX: &str = "unsafe trait Pod:Copy+'static{}unsafe impl Pod for u8{}fn from_bytes<T:Pod>(bytes:&[u8])->Result<&T,()>{if bytes.len()<core::mem::size_of::<T>()||bytes.as_ptr()as usize%core::mem::align_of::<T>()!=0{return Err(());}Ok(unsafe{&*bytes.as_ptr().cast::<T>()})}trait Reader<'a>:Clone+Copy{fn bytes(self,offset:u64,size:u64)->Result<&'a[u8],()>;fn read<T:Pod>(self,offset:&mut u64)->Result<&'a T,()>{let size=core::mem::size_of::<T>()as u64;let bytes=self.bytes(*offset,size)?;*offset+=size;from_bytes(bytes)}fn read_at<T:Pod>(self,mut offset:u64)->Result<&'a T,()>{self.read(&mut offset)}}impl<'a>Reader<'a>for &'a[u8]{fn bytes(self,offset:u64,size:u64)->Result<&'a[u8],()>{self.get(offset as usize..).ok_or(())?.get(..size as usize).ok_or(())}}";

#[test]
fn safe_trait_defaults_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-safe-defaults-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        ("generic_callable","trait R<'a>:Copy{fn get<F:Fn()->u8>(self,f:F)->u8{f()}}fn main(){}".into(),true),
        ("owner_type_parameter","trait R<'a,T: 'a>{fn get(&self)->u8{7}}fn main(){}".into(),true),
        ("unsafe_method","trait R<'a>:Copy{unsafe fn get(self)->&'a u8{panic!()}}fn main(){}".into(),true),
        ("generic_default", format!("{PREFIX}fn take<'a,R:Reader<'a>>(r:R)->&'a u8{{r.read_at(0).unwrap()}}fn main(){{let a=[7u8];assert_eq!(*take(a.as_slice()),7);}}"),true),
        ("short_offset",format!("{PREFIX}fn take<'a,R:Reader<'a>>(r:R)->&'a u8{{let mut offset=0;r.read(&mut offset).unwrap()}}fn main(){{let a=[7u8];assert_eq!(*take(a.as_slice()),7);}}"),true),
        ("short_receiver_borrow",format!("{PREFIX}fn take<'a,R:Reader<'a>>(r:&R)->&'a u8{{(*r).read_at(0).unwrap()}}fn main(){{let a=[7u8];assert_eq!(*take(&a.as_slice()),7);}}"),true),
        ("default_scalar", "trait R<'a>:Copy{fn raw(self)->&'a u32;fn get(self)->&'a u32{self.raw()}}impl<'a>R<'a>for &'a u32{fn raw(self)->&'a u32{self}}fn main(){let x=7;assert_eq!(*(&x).get(),7);}".into(),true),
        ("borrowed_default_data", "trait R<'a>{fn data(&self)->&'a u32;fn get(&self)->&'a u32{self.data()}}struct S<'a>(&'a u32);impl<'a>R<'a>for S<'a>{fn data(&self)->&'a u32{self.0}}fn get<'a>(x:&'a u32)->&'a u32{let s=S(x);s.get()}fn main(){}".into(),true),
        ("borrowed_receiver_elision", "trait R<'a>{fn data(&self)->&'a u32;fn choose(&self,_other:&u32)->&Self{self}}struct S<'a>(&'a u32);impl<'a>R<'a>for S<'a>{fn data(&self)->&'a u32{self.0}}fn main(){let x=7;let s=S(&x);let y=8;assert_eq!(*s.choose(&y).data(),7);}".into(),true),
        ("mutable_borrowed_default", "trait R<'a>{fn data(&self)->&'a u32;fn view(&mut self)->&'a u32{self.data()}}struct S<'a>(&'a u32);impl<'a>R<'a>for S<'a>{fn data(&self)->&'a u32{self.0}}fn get<'a>(x:&'a u32)->&'a u32{let mut s=S(x);s.view()}fn main(){}".into(),true),
        ("default_local_escape","trait R<'a>:Copy{fn get(self)->&'a u32{let x=7;&x}}fn main(){}".into(),false),
        ("default_offset_escape","trait R<'a>:Copy{fn get(self,offset:&mut u32)->&'a u32{offset}}fn main(){}".into(),false),
        ("default_static_return","trait R<'a>:Copy{fn raw(self)->&'a u32;fn get(self)->&'static u32{self.raw()}}fn main(){}".into(),false),
        ("data_escape",format!("{PREFIX}fn get<'a>()->&'a u8{{let data=[7u8];data.as_slice().read_at(0).unwrap()}}fn main(){{}}"),false),
        ("missing_pod_caller_bound",format!("{PREFIX}fn get<'a,R:Reader<'a>,T>(r:R)->&'a T{{r.read_at(0).unwrap()}}fn main(){{}}"),false),
        ("nonstatic_pod_impl","unsafe trait Pod:Copy+'static{}#[derive(Clone,Copy)]struct B<'a>(&'a u8);unsafe impl<'a>Pod for B<'a>{}fn main(){}".into(),false),
        ("static_only_impl","trait R<'a>:Copy{fn get(self)->&'a u32;}impl R<'static>for &'static u32{fn get(self)->&'static u32{self}}fn f<'a>(x:&'a u32)->&'a u32{x.get()}fn main(){}".into(),false),
        ("stricter_impl_type_bound","unsafe trait Pod:Copy+'static{}trait R<'a>:Copy{fn get<T:Pod>(self)->&'a T;}#[derive(Clone,Copy)]struct S;impl<'a>R<'a>for S{fn get<T:Pod+Default>(self)->&'a T{panic!()}}fn main(){}".into(),false),
        ("borrowed_receiver_escape","trait R<'a>{fn data(&self)->&'a u32;fn view(&self)->&'a Self{self}}fn main(){}".into(),false),
        ("borrowed_float_default", "trait R<'a>{fn f(&self,x:f32)->f32{x}}struct S;impl<'a>R<'a>for S{}fn main(){assert_eq!(S.f(1.5),1.5);}".into(),true),
        ("by_value_float_default", "trait R<'a>:Copy{fn f(self,x:f64)->f64{x}}#[derive(Clone,Copy)]struct S;impl<'a>R<'a>for S{}fn main(){assert_eq!(S.f(2.5),2.5);}".into(),true),
    ];
    for (name, source, accepted) in cases {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let errors: Vec<_> = typed
            .diagnostics
            .iter()
            .chain(hir.diagnostics.iter())
            .collect();
        if errors.is_empty() {
            let mir = flux_mir::build_file(&hir, &typed);
            assert_eq!(
                mir.diagnostics.is_empty(),
                accepted,
                "{name}: {:?}",
                mir.diagnostics
            );
        } else {
            assert!(!accepted, "{name}: {errors:?}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unrepresented_default_method_shapes_stay_gated() {
    let root =
        std::env::temp_dir().join(format!("flux-safe-default-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (name, source) = (
        "function_pointer_signature",
        "trait R<'a>:Copy{fn get(self,f:fn(&u8)->u8)->u8{f(&7)}}fn main(){}",
    );
    let path = root.join(format!("{name}.rs"));
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(
        typed
            .diagnostics
            .iter()
            .any(|d| d.message == "lifetime parameters on traits are not supported yet"),
        "{name}: {:?}",
        typed.diagnostics
    );
    std::fs::remove_dir_all(root).unwrap();
}
