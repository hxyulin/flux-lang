// Ready, pending and poll_fn futures with the ready! macro, adapter and
// marker Debug output, Layout::extend, float sums, AsciiExt, and the Unix
// file-type, positioned-I/O and directory-entry extensions.
#![allow(deprecated)]
use std::alloc::Layout;
use std::future::{self, Future, IntoFuture};
use std::marker::PhantomData;
use std::pin::{pin, Pin};
use std::task::{ready, Context, Poll, Waker};

fn double(cx: &mut Context<'_>, inner: &mut Pin<&mut future::Ready<u32>>) -> Poll<u32> {
    let value = ready!(inner.as_mut().poll(cx));
    Poll::Ready(value * 2)
}

fn main() {
    let mut cx = Context::from_waker(Waker::noop());
    let mut ready = pin!(future::ready(21));
    println!("{:?}", double(&mut cx, &mut ready));
    let mut pending = pin!(future::pending::<u8>());
    println!("{:?}", pending.as_mut().poll(&mut cx));
    let mut count = 0;
    let mut polled = pin!(future::poll_fn(|_| { count += 1; if count < 3 { Poll::Pending } else { Poll::Ready(count) } }));
    while polled.as_mut().poll(&mut cx).is_pending() {}
    let mut boxed = Box::pin(future::ready(4u8).into_future());
    println!("{} {:?} {:?}", count, boxed.as_mut().poll(&mut cx), future::ready(1).into_inner());
    println!("{:?} {:?}", Poll::Ready(Ok::<u8, ()>(1)).map_ok(|v| v + 1), Poll::<Result<u8, u8>>::Pending.map_err(|e| e));

    let numbers = [1, 2, 3, 4];
    println!("{:?}", numbers.iter().filter(|n| **n > 1));
    println!("{:?}", numbers.iter().zip(numbers.iter().skip(1)).take(2));
    println!("{:?}", numbers.iter().zip(numbers.iter().filter(|n| **n > 2)));
    println!("{:?}", numbers.iter().rev().copied().peekable());
    println!("{:?} {:?}", numbers.iter().cycle(), std::iter::repeat_n(7, 2));
    let marker: PhantomData<u8> = PhantomData;
    println!("{:?} {:?}", marker, std::sync::Mutex::new([1, 2]));
    println!("{}", format!("{:?}", std::time::Instant::now()).starts_with("Instant { tv_sec: "));

    let (layout, offset) = Layout::new::<u8>().extend(Layout::new::<u64>()).unwrap();
    println!("{} {} {} {:?}", layout.size(), layout.align(), offset, Layout::new::<u16>().align_to(8).map(|l| l.align()));
    let empty: f64 = Vec::<f64>::new().into_iter().sum();
    let total: f32 = [0.5f32, 0.25].iter().sum();
    let product: f64 = [1.5, 2.0].into_iter().product();
    println!("{:?} {:?} {:?}", empty, total, product);
    use std::ascii::AsciiExt;
    println!("{} {:?} {}", AsciiExt::to_ascii_uppercase("abc"), AsciiExt::to_ascii_lowercase(&b"XY"[..]), AsciiExt::eq_ignore_ascii_case(&'a', &'A'));
    println!("{:?}", char::decode_utf16([0xD83D, 0xDE00, 0x61]).map(|r| r.unwrap()).collect::<String>());
    {
        use std::prelude::v1::*;
        let v: Vec<u8> = Vec::new();
        println!("{}", v.len());
    }

    use std::os::unix::fs::{DirEntryExt, FileExt, FileTypeExt, MetadataExt};
    let root = std::env::temp_dir().join(format!("flux-future-misc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("positioned.txt");
    let file = std::fs::File::options().read(true).write(true).create(true).open(&path).unwrap();
    file.write_all_at(b"hello world", 0).unwrap();
    file.write_at(b"W", 6).unwrap();
    let mut buffer = [0u8; 5];
    file.read_exact_at(&mut buffer, 6).unwrap();
    println!("{:?} {:?}", std::str::from_utf8(&buffer), file.read_exact_at(&mut [0u8; 4], 9).map_err(|e| e.kind()));
    let kind = std::fs::metadata(&path).unwrap().file_type();
    println!("{} {} {} {}", kind.is_fifo(), kind.is_socket(), kind.is_block_device(), kind.is_char_device());
    let entry = std::fs::read_dir(&root).unwrap().next().unwrap().unwrap();
    println!("{}", entry.ino() == std::fs::metadata(&path).unwrap().ino());
    std::fs::remove_dir_all(&root).unwrap();

    static LEGACY_ONCE: std::sync::Once = std::sync::ONCE_INIT;
    LEGACY_ONCE.call_once(|| println!("once"));
    LEGACY_ONCE.call_once(|| println!("twice"));

    use std::num::{NonZeroI32, NonZeroU8, NonZeroUsize};
    println!("{} {} {:?} {:?}", NonZeroUsize::MIN, NonZeroU8::MAX, NonZeroU8::MIN.checked_add(254), NonZeroU8::MAX.checked_add(1));
    let eight = NonZeroUsize::new(8).unwrap();
    println!("{:?} {} {} {:?}", eight.checked_mul(eight), eight.ilog2(), eight.is_power_of_two(), NonZeroI32::MIN.checked_abs());
    println!("{:?}", std::iter::zip([1, 2], ['a', 'b']).collect::<Vec<_>>());
    let filled: u32 = unsafe { std::mem::uninitialized() };
    println!("{:#x}", filled);
}
