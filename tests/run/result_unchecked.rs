//! Unchecked extraction moves the payload and leaves its drop to the caller.
//! The inactive variant needs neither Copy nor Debug.
struct Payload(i32);

impl Drop for Payload {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn take_ok<T, E>(value: T) -> T {
    let result: Result<T, E> = Ok(value);
    // SAFETY: the result was constructed as Ok.
    unsafe { core::hint::black_box(result).unwrap_unchecked() }
}

fn take_err<T, E>(error: E) -> E {
    let result: Result<T, E> = Err(error);
    // SAFETY: the result was constructed as Err.
    unsafe { core::hint::black_box(result).unwrap_err_unchecked() }
}

fn main() {
    println!(
        "{} {}",
        take_ok::<_, Payload>(17),
        take_err::<Payload, _>(23)
    );
    let number = 31;
    println!("{}", take_ok::<_, Payload>(&number));
    {
        let value = take_ok::<_, Payload>(Payload(41));
        println!("ok {}", value.0);
        let error = take_err::<Payload, _>(Payload(43));
        println!("err {}", error.0);
        println!("before drop");
    }
    println!("after drop");
    let pair = take_ok::<_, Payload>((Payload(47), Payload(53)));
    let first = pair.0;
    println!("moved {} remaining {}", first.0, pair.1.0);
    drop(first);
    println!("after first");
    drop(pair.1);
    // Match the actual unchanged memchr conversion call shape.
    let bytes = [0u8; 8];
    let origin = bytes.as_ptr();
    // SAFETY: both pointers are in the same array, and end follows origin,
    // so the distance is nonnegative and fits in usize.
    unsafe {
        let end = core::hint::black_box(origin.add(5));
        println!(
            "{}",
            usize::try_from(end.offset_from(origin)).unwrap_unchecked()
        );
    }
}
