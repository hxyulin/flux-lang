use core::array::TryFromSliceError;

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(align(64))]
struct Aligned(u64);

fn convert<const N: usize>(s: &[u32]) -> Result<[u32; N], TryFromSliceError> {
    s.try_into()
}

fn main() {
    let mut source = [11, 22, 33, 44, 55];
    let copied = convert::<3>(&source[1..4]).unwrap();
    assert_eq!(copied, [22, 33, 44]);
    assert_eq!(source, [11, 22, 33, 44, 55]);
    source[2] = 99;
    assert_eq!(copied, [22, 33, 44]);
    assert!(convert::<3>(&source[..2]).is_err());
    assert!(convert::<3>(&source[..4]).is_err());
    assert!(convert::<0>(&source).is_err());
    assert_eq!(convert::<0>(&source[2..2]).unwrap(), []);
    let units: &[()] = &[(), (), ()];
    let zst: [(); 3] = units.try_into().unwrap();
    assert_eq!(zst.len(), 3);
    let aligned = [Aligned(7), Aligned(8)];
    let slice: &[Aligned] = &aligned;
    let copy: [Aligned; 2] = slice.try_into().unwrap();
    assert_eq!(copy, aligned);
    let error = convert::<2>(&source).unwrap_err();
    let cloned = error.clone();
    assert_eq!(
        format!("{} {:?}", cloned, error),
        "could not convert slice to array TryFromSliceError(())"
    );
    println!("slice-array-ok {:?} {}", copied, source[2]);
}
