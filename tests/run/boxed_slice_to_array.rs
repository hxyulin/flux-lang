use std::cell::Cell;

struct CountDrop<'a>(&'a Cell<usize>);

impl Drop for CountDrop<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn main() {
    let drops = Cell::new(0);
    let slice = vec![CountDrop(&drops), CountDrop(&drops)].into_boxed_slice();
    let address = slice.as_ptr();
    let rejected: Result<Box<[_; 3]>, _> = slice.try_into();
    let slice = match rejected {
        Err(slice) => slice,
        Ok(_) => panic!("wrong length"),
    };
    assert!(slice.as_ptr() == address);
    let accepted: Result<Box<[_; 2]>, _> = slice.try_into();
    let array = match accepted {
        Ok(array) => array,
        Err(_) => panic!("right length"),
    };
    assert!(array.as_ptr() == address);
    assert_eq!(drops.get(), 0);
    drop(array);
    assert_eq!(drops.get(), 2);

    let empty: Box<[u8; 0]> = Vec::<u8>::new().into_boxed_slice().try_into().unwrap();
    assert!(empty.is_empty());
    let units: Box<[(); 3]> = vec![(); 3].into_boxed_slice().try_into().unwrap();
    assert_eq!(units.len(), 3);
    println!("boxed conversions passed");
}
