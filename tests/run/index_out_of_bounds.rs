//! An index past the end of a slice panics, after what came before ran:
//! with `panic=abort` the program aborts ("index out of bounds: the len
//! is 3 but the index is 3").

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn get(values: &[i32], index: usize) -> i32 {
    values[index]
}

fn main() {
    let values = [10, 20, 30];
    let mut i = 0;
    while i < 4 {
        unsafe { printf(c"%d\n".as_ptr(), get(&values, i)) };
        i += 1;
    }
}
