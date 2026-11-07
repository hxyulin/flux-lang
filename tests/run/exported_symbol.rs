#[unsafe(no_mangle)]
extern "C" fn flux_export_regression(value: u32) -> u32 { value + 5 }
unsafe extern "C" {
    #[link_name = "flux_export_regression"]
    fn imported(value: u32) -> u32;
}
fn main() { println!("{}", unsafe { imported(7) }); }
