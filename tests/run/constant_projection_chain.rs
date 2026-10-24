// A finite caller-equality chain exceeds the old output-walk budget.
trait Chain {
    type A0;
    type A1;
    type A2;
    type A3;
    type A4;
    type A5;
    type A6;
    type A7;
    type A8;
    type A9;
    type A10;
    type A11;
    type A12;
    type A13;
    type A14;
    type A15;
    type A16;
    type A17;
    type A18;
    type A19;
    type A20;
    type A21;
    type A22;
    type A23;
    type A24;
    type A25;
    type A26;
    type A27;
    type A28;
    type A29;
    type A30;
    type A31;
    type A32;
    type A33;
    type A34;
    type A35;
    type A36;
    type A37;
    type A38;
    type A39;
    type A40;
    type A41;
    type A42;
    type A43;
    type A44;
    type A45;
    type A46;
    type A47;
    type A48;
    type A49;
    type A50;
    type A51;
    type A52;
    type A53;
    type A54;
    type A55;
    type A56;
    type A57;
    type A58;
    type A59;
    type A60;
    type A61;
    type A62;
    type A63;
    type A64;
    type A65;
    type A66;
    type A67;
    type A68;
    type A69;
    const VALUE: Self::A0;
}
struct Value;
impl Chain for Value {
    type A0 = u8;
    type A1 = u8;
    type A2 = u8;
    type A3 = u8;
    type A4 = u8;
    type A5 = u8;
    type A6 = u8;
    type A7 = u8;
    type A8 = u8;
    type A9 = u8;
    type A10 = u8;
    type A11 = u8;
    type A12 = u8;
    type A13 = u8;
    type A14 = u8;
    type A15 = u8;
    type A16 = u8;
    type A17 = u8;
    type A18 = u8;
    type A19 = u8;
    type A20 = u8;
    type A21 = u8;
    type A22 = u8;
    type A23 = u8;
    type A24 = u8;
    type A25 = u8;
    type A26 = u8;
    type A27 = u8;
    type A28 = u8;
    type A29 = u8;
    type A30 = u8;
    type A31 = u8;
    type A32 = u8;
    type A33 = u8;
    type A34 = u8;
    type A35 = u8;
    type A36 = u8;
    type A37 = u8;
    type A38 = u8;
    type A39 = u8;
    type A40 = u8;
    type A41 = u8;
    type A42 = u8;
    type A43 = u8;
    type A44 = u8;
    type A45 = u8;
    type A46 = u8;
    type A47 = u8;
    type A48 = u8;
    type A49 = u8;
    type A50 = u8;
    type A51 = u8;
    type A52 = u8;
    type A53 = u8;
    type A54 = u8;
    type A55 = u8;
    type A56 = u8;
    type A57 = u8;
    type A58 = u8;
    type A59 = u8;
    type A60 = u8;
    type A61 = u8;
    type A62 = u8;
    type A63 = u8;
    type A64 = u8;
    type A65 = u8;
    type A66 = u8;
    type A67 = u8;
    type A68 = u8;
    type A69 = u8;
    const VALUE: u8 = 79;
}
struct S<T>(T);
impl<T> S<T>
where
    T: Chain<
        A0 = <T as Chain>::A1,
        A1 = <T as Chain>::A2,
        A2 = <T as Chain>::A3,
        A3 = <T as Chain>::A4,
        A4 = <T as Chain>::A5,
        A5 = <T as Chain>::A6,
        A6 = <T as Chain>::A7,
        A7 = <T as Chain>::A8,
        A8 = <T as Chain>::A9,
        A9 = <T as Chain>::A10,
        A10 = <T as Chain>::A11,
        A11 = <T as Chain>::A12,
        A12 = <T as Chain>::A13,
        A13 = <T as Chain>::A14,
        A14 = <T as Chain>::A15,
        A15 = <T as Chain>::A16,
        A16 = <T as Chain>::A17,
        A17 = <T as Chain>::A18,
        A18 = <T as Chain>::A19,
        A19 = <T as Chain>::A20,
        A20 = <T as Chain>::A21,
        A21 = <T as Chain>::A22,
        A22 = <T as Chain>::A23,
        A23 = <T as Chain>::A24,
        A24 = <T as Chain>::A25,
        A25 = <T as Chain>::A26,
        A26 = <T as Chain>::A27,
        A27 = <T as Chain>::A28,
        A28 = <T as Chain>::A29,
        A29 = <T as Chain>::A30,
        A30 = <T as Chain>::A31,
        A31 = <T as Chain>::A32,
        A32 = <T as Chain>::A33,
        A33 = <T as Chain>::A34,
        A34 = <T as Chain>::A35,
        A35 = <T as Chain>::A36,
        A36 = <T as Chain>::A37,
        A37 = <T as Chain>::A38,
        A38 = <T as Chain>::A39,
        A39 = <T as Chain>::A40,
        A40 = <T as Chain>::A41,
        A41 = <T as Chain>::A42,
        A42 = <T as Chain>::A43,
        A43 = <T as Chain>::A44,
        A44 = <T as Chain>::A45,
        A45 = <T as Chain>::A46,
        A46 = <T as Chain>::A47,
        A47 = <T as Chain>::A48,
        A48 = <T as Chain>::A49,
        A49 = <T as Chain>::A50,
        A50 = <T as Chain>::A51,
        A51 = <T as Chain>::A52,
        A52 = <T as Chain>::A53,
        A53 = <T as Chain>::A54,
        A54 = <T as Chain>::A55,
        A55 = <T as Chain>::A56,
        A56 = <T as Chain>::A57,
        A57 = <T as Chain>::A58,
        A58 = <T as Chain>::A59,
        A59 = <T as Chain>::A60,
        A60 = <T as Chain>::A61,
        A61 = <T as Chain>::A62,
        A62 = <T as Chain>::A63,
        A63 = <T as Chain>::A64,
        A64 = <T as Chain>::A65,
        A65 = <T as Chain>::A66,
        A66 = <T as Chain>::A67,
        A67 = <T as Chain>::A68,
        A68 = <T as Chain>::A69,
        A69 = u8,
    >,
{
    const VALUE: u8 = T::VALUE;
}
fn main() {
    assert_eq!(S::<Value>::VALUE, 79);
    println!("constant projection chain {}", S::<Value>::VALUE);
}
