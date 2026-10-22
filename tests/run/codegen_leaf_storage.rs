//! Exercise inline and spilled codegen values, pooled locals and fat pointers.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Wide {
    values: (i32, i32, i32, i32, i32, i32),
    empty: (),
}

fn transform(mut value: Wide, extra: i32) -> Wide {
    value.values.0 += extra;
    value.values.5 += extra * 2;
    value
}

fn sum(value: &Wide, slice: &[i32]) -> i32 {
    let (a, b, c, d, e, f) = value.values;
    a + b + c + d + e + f + slice[0] + slice[2]
}

fn evaluate(extra: i32) -> i32 {
    let mut value = transform(
        Wide {
            values: (1, 2, 3, 4, 5, 6),
            empty: (),
        },
        extra,
    );
    let first = sum(&value, &[7, 8, 9]);
    let reference = &mut value.values.3;
    *reference += first;
    let () = value.empty;
    first + sum(&value, &[10, 11, 12])
}

fn main() {
    unsafe {
        printf(c"%d\n".as_ptr(), evaluate(3));
    }
}
