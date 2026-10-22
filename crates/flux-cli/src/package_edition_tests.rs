//! Read the actual registry manifest without editing it or compiling dev units.
use super::*;

#[test]
#[ignore = "requires the cached Cargo.lock unicode-ident version (1.0.27)"]
fn cached_unicode_ident_normalized_manifest_selects_2021_library() {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo"));
    let dependency = std::fs::read_dir(cargo_home.join("registry/src"))
        .unwrap()
        .map(|entry| entry.unwrap().path().join("unicode-ident-1.0.27"))
        .find(|path| path.join("Cargo.toml").is_file())
        .expect("cache unicode-ident 1.0.27 first");
    let manifest = dependency.join("Cargo.toml");
    let original = std::fs::read(&manifest).unwrap();
    let library = std::fs::read(dependency.join("src/lib.rs")).unwrap();
    let tables = std::fs::read(dependency.join("src/tables.rs")).unwrap();
    let root = std::env::temp_dir().join(format!("flux-unicode-plan-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    // JSON string escaping also produces a valid TOML basic string here.
    let dep_path = serde_json::to_string(&dependency.to_string_lossy()).unwrap();
    std::fs::write(root.join("Cargo.toml"), format!("[package]\nname='unicode-probe'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nunicode-ident={{path={dep_path}}}\n")).unwrap();
    let result = plan(&Options {
        manifest_path: root.join("Cargo.toml"),
        bin: None,
        optimize: false,
    });
    std::fs::remove_dir_all(root).unwrap();
    let plan = result.unwrap();
    assert_eq!(plan.sources.len(), 2);
    assert_eq!(plan.sources[0].edition, flux_syntax::Edition::E2021);
    assert_eq!(plan.sources[0].file, dependency.join("src/lib.rs"));
    assert_eq!(plan.sources[1].edition, flux_syntax::Edition::E2024);
    assert_eq!(std::fs::read(manifest).unwrap(), original);
    assert_eq!(
        std::fs::read(dependency.join("src/lib.rs")).unwrap(),
        library
    );
    assert_eq!(
        std::fs::read(dependency.join("src/tables.rs")).unwrap(),
        tables
    );
}
