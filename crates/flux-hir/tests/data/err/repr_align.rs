#[repr(align(0))]
struct Zero;
#[repr(align(3))]
struct NotPowerOfTwo;
#[repr(align(1073741824))]
struct TooLarge;
#[repr(align(8u8))]
struct Suffixed;
#[repr(align("8"))]
struct String;
#[repr(align(-8))]
struct Negative;
#[repr(align(8, 16))]
struct Multiple;
#[repr(align())]
struct Missing;
#[repr(align(N))]
struct NotLiteral;
#[repr(align(32), packed)]
struct Packed;
#[repr(align(7))]
enum InvalidEnum { A }
#[repr(align(8))]
enum EmptyEnum {}
