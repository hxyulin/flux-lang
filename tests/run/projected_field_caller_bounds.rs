use std::marker::PhantomData;

trait TypeFn<T> {
    type Output;
}

struct Invoke<F>(PhantomData<F>);

impl<F: TypeFn<T>, T> TypeFn<T> for Invoke<F> {
    type Output = F::Output;
}

type Call<F, T> = <F as TypeFn<T>>::Output;

struct Result<F, T>
where
    Invoke<F>: TypeFn<T>,
{
    out: Call<Invoke<F>, T>,
}

enum Wrapped<F, T>
where
    Invoke<F>: TypeFn<T>,
{
    Output(Call<Invoke<F>, T>),
}

fn invoke<F, T>(result: Result<F, T>) -> Call<Invoke<F>, T>
where
    Invoke<F>: TypeFn<T>,
{
    result.out
}

fn wrapped<F, T>(result: Result<F, T>) -> Call<Invoke<F>, T>
where
    Invoke<F>: TypeFn<T>,
{
    let Wrapped::Output(value) = Wrapped::<F, T>::Output(result.out);
    value
}

struct Identity;

impl TypeFn<u32> for Identity {
    type Output = u32;
}

fn main() {
    assert_eq!(invoke::<Identity, u32>(Result { out: 42 }), 42);
    assert_eq!(wrapped::<Identity, u32>(Result { out: 43 }), 43);
}
