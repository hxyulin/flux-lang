fn main(){
    for ch in ['a','A','é','É','Σ','İ','ß','\u{fb03}','\u{345}','\u{10400}','\0','🦀'] {
        let lower:String=ch.to_lowercase().collect();let upper:String=ch.to_uppercase().collect();
        println!("{:x} {} {} {:?} {:?}",ch as u32,ch.is_lowercase(),ch.is_uppercase(),lower,upper);
    }
    let mut iter='\u{fb03}'.to_uppercase();
    assert_eq!(iter.len(),3);assert_eq!(iter.next(),Some('F'));assert_eq!(iter.next_back(),Some('I'));
    assert_eq!(format!("{}",iter),"F");
    println!("{:?} {:#?}",iter,iter);
    let mut cloned=iter.clone();assert_eq!(cloned.next(),Some('F'));assert_eq!(cloned.next(),None);assert_eq!(cloned.next_back(),None);
}
