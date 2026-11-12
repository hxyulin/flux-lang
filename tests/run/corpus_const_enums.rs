#[repr(u8)]
#[derive(Copy, Clone)]
enum Byte { Lead = 0xEF, Last = 0xBD }
#[repr(i16)]
enum Signed { Negative = -7, Positive = 513 }
const LEAD: Byte = unsafe { std::mem::transmute(0xEFu8) };
const BYTES: [Byte; 2] = unsafe { std::mem::transmute([0xEFu8, 0xBDu8]) };
const RAW: [u8; 2] = unsafe { std::mem::transmute(BYTES) };
const SIGNED: Signed = unsafe { std::mem::transmute(-7i16) };
enum Error { Invalid }
trait Convert { type Error; fn convert() -> Result<(), Self::Error>; }
struct Value;
impl Convert for Value {
    type Error = Error;
    fn convert() -> Result<(), Self::Error> { Err(Self::Error::Invalid) }
}
const LOCAL_BYTE:u8={let mut bytes=[0u8;4];bytes[2]=9;unsafe{*bytes.as_ptr().add(2)}};
trait Marker { type Data; }
struct Data { field: u32 }
struct M;
impl Marker for M { type Data = Data; }
struct Baked;
impl Baked { const DATA: &'static <M as Marker>::Data = &Data { field: 7 }; }
#[repr(transparent)]
struct Raw([u8; 2]);
#[repr(transparent)]
struct View([Raw]);
const RAW_VIEW: &[Raw] = unsafe { std::slice::from_raw_parts(b"abcd".as_ptr() as *const Raw, 2) };
const VIEW: &View = unsafe { &*(RAW_VIEW as *const [Raw] as *const View) };
trait Family<'a>:'static{type Output:'a;}
struct Borrowed<'a>(&'a u8);
impl<'a>Family<'a>for Borrowed<'static>{type Output=Borrowed<'a>;}
trait Named{
 type Owned:'static+for<'a>Family<'a,Output=Self::View<'a>>;
 type View<'a>:'a;
 const SINGLETON:&'static Self::View<'static>;
}
struct Name;
impl Named for Name {
 type Owned=Borrowed<'static>;
 type View<'a>=Borrowed<'a>;
 const SINGLETON:&'static Self::Owned=&Borrowed(&23);
}
fn first<'a:'a>(x:&'a u8)->&'a u8{x}
fn forward<'s>(x:&'s u8)->&'s u8{first::<'s>(x)}
#[repr(transparent)]
struct MarkedView<T:?Sized>{marker:std::marker::PhantomData<T>,bytes:[u8]}
const MARKED:&MarkedView<str>=unsafe{std::mem::transmute::<&[u8],&MarkedView<str>>(b"abc")};
#[repr(C)]
#[derive(Copy,Clone)]
union Bits { bytes:[u8;4], other:[u8;4] }
const UNION_BYTES:&[Bits]=unsafe{std::slice::from_raw_parts(b"abcdwxyz".as_ptr() as *const Bits,2)};
static STATIC_DATA:<M as Marker>::Data=Data{field:41};
const STATIC_FIELD:&u32=&STATIC_DATA.field;
static STATIC_REF:&Data=&Data{field:43};
const STATIC_REBORROW:&Data=&*STATIC_REF;
fn size<const N:usize>()->usize{N}
macro_rules! sized_call {()=>{size::<{2+3}>()}}
fn main() {
    println!("{} {}",size::<{1+2}>(),sized_call!());
    println!("{}",STATIC_REBORROW.field);
    println!("{}",STATIC_FIELD);
    println!("{:?}", unsafe{UNION_BYTES[1].bytes});
    println!("{:?}", &MARKED.bytes);
    println!("{} {}", Name::SINGLETON.0, forward(&37));
    println!("{:?} {:?}", RAW_VIEW[0].0, VIEW.0[1].0);
    println!("{}", Baked::DATA.field);
    println!("{}", LOCAL_BYTE);
    println!("{}", Value::convert().is_err());
    assert_eq!(LEAD as u8, 0xEF);
    assert_eq!(BYTES[1] as u8, 0xBD);
    assert_eq!(RAW, [0xEF, 0xBD]);
    assert_eq!(SIGNED as i16, -7);
    println!("{} {:?} {}", LEAD as u8, RAW, SIGNED as i16);
}
