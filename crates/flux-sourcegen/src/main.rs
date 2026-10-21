//! Rewrites flux-syntax's `src/ast/generated.rs` from `flux.ungram`.

fn main() {
    let path = flux_sourcegen::generated_path();
    let code = flux_sourcegen::generate_ast();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current == code {
        println!("{} is up to date", path.display());
    } else {
        std::fs::write(&path, code).unwrap();
        println!("wrote {}", path.display());
    }
}
