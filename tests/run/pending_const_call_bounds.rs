trait Field<const N:usize> {type Value:Copy;fn get(&self)->Self::Value;}
impl Field<0> for (u32,u64) {type Value=u32;fn get(&self)->u32 {self.0}}
impl Field<1> for (u32,u64) {type Value=u64;fn get(&self)->u64 {self.1}}
const fn index()->usize {1}
fn get<const N:usize>(value:&(u32,u64))-><(u32,u64) as Field<N>>::Value
where (u32,u64):Field<N> {Field::<N>::get(value)}
struct View<'a>(&'a(u32,u64));
impl View<'_> {
    fn get<const N:usize>(&self)-><(u32,u64) as Field<N>>::Value
    where (u32,u64):Field<N> {Field::<N>::get(self.0)}
}
fn main(){
    let value=(7u32,11u64);
    assert_eq!(get::<{index()}>(&value),11u64);
    assert_eq!(View(&value).get::<{index()-1}>(),7u32);
    println!("{} {}",get::<{index()-1}>(&value),View(&value).get::<{index()}>());
}
