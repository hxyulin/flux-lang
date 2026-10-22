// Baseline feature selection must agree between cfg! and attribute pruning.
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
fn selected() -> u32 { 16 }
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
fn selected() -> u32 { 32 }

#[cfg(any(
    all(target_arch = "aarch64", not(target_feature = "neon")),
    all(target_arch = "x86_64", not(target_feature = "sse2"))
))]
fn selected() -> u32 { missing_baseline!() }

#[cfg_attr(all(target_arch = "aarch64", target_feature = "neon"), derive(Clone, Copy))]
#[cfg_attr(all(target_arch = "x86_64", target_feature = "sse2"), derive(Clone, Copy))]
struct Selected(u32);

fn main() {
    let value = Selected(selected());
    let copied = value;
    println!("{} {} {} {}", value.0, copied.0,
        cfg!(target_feature = "neon"), cfg!(target_feature = "sse2"));
}
