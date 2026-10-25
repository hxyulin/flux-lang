#[derive(Clone, Copy)]
enum Large {
    V0(u64,u64,u64,u64,u64,u64,u64,u64),
    V1(u64,u64,u64,u64,u64,u64,u64,u64),
    V2(u64,u64,u64,u64,u64,u64,u64,u64),
    V3(u64,u64,u64,u64,u64,u64,u64,u64),
    V4(u64,u64,u64,u64,u64,u64,u64,u64),
    V5(u64,u64,u64,u64,u64,u64,u64,u64),
    V6(u64,u64,u64,u64,u64,u64,u64,u64),
    V7(u64,u64,u64,u64,u64,u64,u64,u64),
    V8(u64,u64,u64,u64,u64,u64,u64,u64),
    V9(u64,u64,u64,u64,u64,u64,u64,u64),
    V10(u64,u64,u64,u64,u64,u64,u64,u64),
    V11(u64,u64,u64,u64,u64,u64,u64,u64),
    V12(u64,u64,u64,u64,u64,u64,u64,u64),
    V13(u64,u64,u64,u64,u64,u64,u64,u64),
    V14(u64,u64,u64,u64,u64,u64,u64,u64),
    V15(u64,u64,u64,u64,u64,u64,u64,u64),
    V16(u64,u64,u64,u64,u64,u64,u64,u64),
    V17(u64,u64,u64,u64,u64,u64,u64,u64),
    V18(u64,u64,u64,u64,u64,u64,u64,u64),
    V19(u64,u64,u64,u64,u64,u64,u64,u64),
    V20(u64,u64,u64,u64,u64,u64,u64,u64),
    V21(u64,u64,u64,u64,u64,u64,u64,u64),
    V22(u64,u64,u64,u64,u64,u64,u64,u64),
    V23(u64,u64,u64,u64,u64,u64,u64,u64),
    V24(u64,u64,u64,u64,u64,u64,u64,u64),
    V25(u64,u64,u64,u64,u64,u64,u64,u64),
    V26(u64,u64,u64,u64,u64,u64,u64,u64),
    V27(u64,u64,u64,u64,u64,u64,u64,u64),
    V28(u64,u64,u64,u64,u64,u64,u64,u64),
    V29(u64,u64,u64,u64,u64,u64,u64,u64),
    V30(u64,u64,u64,u64,u64,u64,u64,u64),
    V31(u64,u64,u64,u64,u64,u64,u64,u64),
    V32(u64,u64,u64,u64,u64,u64,u64,u64),
    V33(u64,u64,u64,u64,u64,u64,u64,u64),
    V34(u64,u64,u64,u64,u64,u64,u64,u64),
    V35(u64,u64,u64,u64,u64,u64,u64,u64),
    V36(u64,u64,u64,u64,u64,u64,u64,u64),
    V37(u64,u64,u64,u64,u64,u64,u64,u64),
    V38(u64,u64,u64,u64,u64,u64,u64,u64),
    V39(u64,u64,u64,u64,u64,u64,u64,u64),
    V40(u64,u64,u64,u64,u64,u64,u64,u64),
    V41(u64,u64,u64,u64,u64,u64,u64,u64),
    V42(u64,u64,u64,u64,u64,u64,u64,u64),
    V43(u64,u64,u64,u64,u64,u64,u64,u64),
    V44(u64,u64,u64,u64,u64,u64,u64,u64),
    V45(u64,u64,u64,u64,u64,u64,u64,u64),
    V46(u64,u64,u64,u64,u64,u64,u64,u64),
    V47(u64,u64,u64,u64,u64,u64,u64,u64),
    V48(u64,u64,u64,u64,u64,u64,u64,u64),
    V49(u64,u64,u64,u64,u64,u64,u64,u64),
    V50(u64,u64,u64,u64,u64,u64,u64,u64),
    V51(u64,u64,u64,u64,u64,u64,u64,u64),
    V52(u64,u64,u64,u64,u64,u64,u64,u64),
    V53(u64,u64,u64,u64,u64,u64,u64,u64),
    V54(u64,u64,u64,u64,u64,u64,u64,u64),
    V55(u64,u64,u64,u64,u64,u64,u64,u64),
    V56(u64,u64,u64,u64,u64,u64,u64,u64),
    V57(u64,u64,u64,u64,u64,u64,u64,u64),
    V58(u64,u64,u64,u64,u64,u64,u64,u64),
    V59(u64,u64,u64,u64,u64,u64,u64,u64),
    V60(u64,u64,u64,u64,u64,u64,u64,u64),
    V61(u64,u64,u64,u64,u64,u64,u64,u64),
    V62(u64,u64,u64,u64,u64,u64,u64,u64),
    V63(u64,u64,u64,u64,u64,u64,u64,u64),
    V64(u64,u64,u64,u64,u64,u64,u64,u64),
    V65(u64,u64,u64,u64,u64,u64,u64,u64),
    V66(u64,u64,u64,u64,u64,u64,u64,u64),
    V67(u64,u64,u64,u64,u64,u64,u64,u64),
    V68(u64,u64,u64,u64,u64,u64,u64,u64),
    V69(u64,u64,u64,u64,u64,u64,u64,u64),
    V70(u64,u64,u64,u64,u64,u64,u64,u64),
    V71(u64,u64,u64,u64,u64,u64,u64,u64),
    V72(u64,u64,u64,u64,u64,u64,u64,u64),
    V73(u64,u64,u64,u64,u64,u64,u64,u64),
    V74(u64,u64,u64,u64,u64,u64,u64,u64),
    V75(u64,u64,u64,u64,u64,u64,u64,u64),
    V76(u64,u64,u64,u64,u64,u64,u64,u64),
    V77(u64,u64,u64,u64,u64,u64,u64,u64),
    V78(u64,u64,u64,u64,u64,u64,u64,u64),
    V79(u64,u64,u64,u64,u64,u64,u64,u64),
    V80(u64,u64,u64,u64,u64,u64,u64,u64),
    V81(u64,u64,u64,u64,u64,u64,u64,u64),
    V82(u64,u64,u64,u64,u64,u64,u64,u64),
    V83(u64,u64,u64,u64,u64,u64,u64,u64),
    V84(u64,u64,u64,u64,u64,u64,u64,u64),
    V85(u64,u64,u64,u64,u64,u64,u64,u64),
    V86(u64,u64,u64,u64,u64,u64,u64,u64),
    V87(u64,u64,u64,u64,u64,u64,u64,u64),
    V88(u64,u64,u64,u64,u64,u64,u64,u64),
    V89(u64,u64,u64,u64,u64,u64,u64,u64),
    V90(u64,u64,u64,u64,u64,u64,u64,u64),
    V91(u64,u64,u64,u64,u64,u64,u64,u64),
    V92(u64,u64,u64,u64,u64,u64,u64,u64),
    V93(u64,u64,u64,u64,u64,u64,u64,u64),
    V94(u64,u64,u64,u64,u64,u64,u64,u64),
    V95(u64,u64,u64,u64,u64,u64,u64,u64),
}
#[derive(Clone, Copy)]
struct Record { a: (u64,u64,u64,u64,u64,u64,u64,u64), b: (u64,u64,u64,u64,u64,u64,u64,u64), c: (u64,u64,u64,u64,u64,u64,u64,u64) }
struct Owned<'a> { value: Record, drops: &'a std::cell::Cell<u32> }
impl Drop for Owned<'_> { fn drop(&mut self) { self.drops.set(self.drops.get()+1); } }
fn identity<T>(value: T) -> T { value }
fn pass(value: Large) -> Large { identity(value) }
fn apply<F: FnMut(Large)->Large>(f: &mut F, value: Large) -> Large { f(value) }
fn invoke_once<F: FnOnce()->u64>(f: F) -> u64 { f() }
fn number(value: Large) -> u64 { match value { Large::V95(a,b,c,d,e,f,g,h) => a+b+c+d+e+f+g+h, _ => 0 } }
fn change(mut value: Record) -> Record { value.a.0 += 100; value }
fn from_ref(value: &Record) -> Record { change(*value) }
fn snapshot(copy: Record, original: &mut Record) -> u64 { original.a.0 += 10; copy.a.0 }
fn main() {
    let original = Large::V95(1,2,3,4,5,6,7,8);
    assert_eq!(number(pass(original)), 36);
    let pointer: fn(Large)->Large = pass;
    assert_eq!(number(pointer(original)), 36);
    let closure_pointer: fn(Large)->Large = |x| x;
    assert_eq!(number(closure_pointer(original)), 36);
    let mut calls = 0;
    let mut closure = |x| { calls += 1; x };
    assert_eq!(number(apply(&mut closure, original)), 36);
    let object: &mut dyn FnMut(Large)->Large = &mut closure;
    assert_eq!(number(object(original)), 36);
    assert_eq!(calls, 2);
    let mut record = Record { a:(1,2,3,4,5,6,7,8), b:(9,10,11,12,13,14,15,16), c:(17,18,19,20,21,22,23,24) };
    let changed = change(record);
    assert_eq!(record.a.0, 1);
    assert_eq!(changed.a.0, 101);
    assert_eq!(changed.b.7, 16);
    assert_eq!(changed.c.7, 24);
    record = from_ref(&record);
    assert_eq!(record.a.0, 101);
    assert_eq!(snapshot(record, &mut record), 101);
    assert_eq!(record.a.0, 111);
    let drops = std::cell::Cell::new(0);
    { let owned = identity(Owned { value: record, drops: &drops }); assert_eq!(owned.value.c.7,24); }
    assert_eq!(drops.get(), 1);
    let owned = Owned { value: record, drops: &drops };
    let take = move || { let value = owned.value.c.7; drop(owned); value };
    assert_eq!(invoke_once(take), 24);
    assert_eq!(drops.get(), 2);
    println!("large {} {} {} {}", number(original), record.a.0, calls, drops.get());
}
