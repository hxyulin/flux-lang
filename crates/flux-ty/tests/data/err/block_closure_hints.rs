fn apply<F: FnOnce(&(u8, u16)) -> u16>(f: F) -> u16 { f(&(1, 2)) }
fn wrong_type() { let _ = apply({ |value: u8| value as u16 }); }
fn wrong_arity() { let _ = apply({ || 0 }); }
