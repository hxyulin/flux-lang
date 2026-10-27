trait Pass: Sync {
    fn value(&self) -> usize;
    fn extra(&self) -> usize {
        self.value() + 1
    }
}
struct Empty;
impl Pass for Empty {
    fn value(&self) -> usize {
        7
    }
}
#[derive(Clone, Copy)]
struct Builtin {
    value: usize,
}
impl Pass for Builtin {
    fn value(&self) -> usize {
        self.value
    }
}
const BUILTINS: [Builtin; 2] = [Builtin { value: 11 }, Builtin { value: 13 }];
static SAVED: Builtin = Builtin { value: 17 };
const SINGLE: &dyn Pass = &Builtin { value: 5 };
const PASSES: &[&dyn Pass] = &[&Empty, &BUILTINS[0], &BUILTINS[1], &SAVED, SINGLE];
static STATIC_PASSES: [&dyn Pass; 2] = [&Empty, &SAVED];
struct Tag<T>(core::marker::PhantomData<T>);
trait GenericPass {
    fn value(&self) -> usize;
}
impl<T> GenericPass for Tag<T> {
    fn value(&self) -> usize {
        core::mem::size_of::<T>()
    }
}
struct Holder<T>(T);
impl<T: Pass + 'static> Holder<T> {
    const PASS: &'static dyn Pass = &Empty;
    const GENERIC: &'static dyn GenericPass = &Tag::<T>(core::marker::PhantomData);
}
trait Base {
    fn base(&self) -> usize;
}
trait Child: Base {
    fn child(&self) -> usize;
}
impl Base for Builtin {
    fn base(&self) -> usize {
        self.value + 20
    }
}
impl Child for Builtin {
    fn child(&self) -> usize {
        self.value + 30
    }
}
const CHILD: &dyn Child = &Builtin { value: 3 };
const BASE: &dyn Base = CHILD;
const PAIR: (&dyn Pass, &dyn Base) = (&BUILTINS[1], BASE);
const AGAIN: &dyn Base = &*BASE;
trait Associated: Sync {
    type Out;
    fn get(&self) -> Self::Out;
}
impl Associated for Builtin {
    type Out = usize;
    fn get(&self) -> usize {
        self.value
    }
}
const ASSOCIATED: &dyn Associated<Out = usize> = &BUILTINS[0];
struct Dropping(usize);
impl Drop for Dropping {
    fn drop(&mut self) {
        println!("unexpected static destruction");
    }
}
impl Pass for Dropping {
    fn value(&self) -> usize {
        self.0
    }
}
static DROPPING: Dropping = Dropping(19);
const DROPPING_REF: &dyn Pass = &DROPPING;
fn is_seven(x: &u32) -> bool {
    *x == 7
}
const PREDICATE: &dyn Fn(&u32) -> bool = &|x| *x == 7;
const NAMED_PREDICATE: &dyn Fn(&u32) -> bool = &is_seven;
// Unused constants must not root their vtable methods during monomorphization.
trait Unused {
    fn unused(&self);
}
struct Never<T>(core::marker::PhantomData<T>);
fn recursive<T>() {
    recursive::<Option<T>>();
}
impl<T> Unused for Never<T> {
    fn unused(&self) {
        recursive::<T>();
    }
}
const UNUSED: &dyn Unused = &Never::<u8>(core::marker::PhantomData);

fn main() {
    let mut sum = 0;
    for pass in PASSES {
        sum += pass.value() + pass.extra();
    }
    assert_eq!(sum, 111);
    assert_eq!(STATIC_PASSES[0].value() + STATIC_PASSES[1].value(), 24);
    assert_eq!(Holder::<Builtin>::PASS.value(), 7);
    assert_eq!(
        Holder::<Builtin>::GENERIC.value(),
        core::mem::size_of::<Builtin>()
    );
    assert_eq!(CHILD.child(), 33);
    assert_eq!(BASE.base(), 23);
    assert_eq!(AGAIN.base(), 23);
    assert_eq!(ASSOCIATED.get(), 11);
    assert_eq!(DROPPING_REF.value(), 19);
    assert!(PREDICATE(&7));
    assert!(!PREDICATE(&8));
    assert!(NAMED_PREDICATE(&7));
    assert!(!NAMED_PREDICATE(&8));
    assert_eq!(PAIR.0.value() + PAIR.1.base(), 36);
    assert_eq!(
        core::mem::size_of_val(SINGLE),
        core::mem::size_of::<Builtin>()
    );
    assert_eq!(
        core::mem::align_of_val(SINGLE),
        core::mem::align_of::<Builtin>()
    );
    println!("const traits {} {} {}", sum, BASE.base(), PAIR.0.value());
}
