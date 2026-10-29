//! The pinned IR source closure, shared by IR and float-folder comparisons.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

const NAMES: [&str; 3] = ["smallvec", "keel-asm-ir", "keel-ir"];

pub fn packages(metadata: &Value) -> Vec<&Value> {
    let packages: Vec<_> = NAMES
        .iter()
        .map(|name| {
            metadata["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == *name)
                .unwrap()
        })
        .collect();
    for (index, package) in packages.iter().enumerate() {
        let node = metadata["resolve"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == package["id"])
            .unwrap();
        assert!(node["features"].as_array().unwrap().is_empty());
        let actual: Vec<_> = node["deps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["kind"].is_null())
            })
            .map(|d| &d["pkg"])
            .collect();
        let expected: Vec<_> = index
            .checked_sub(1)
            .map(|i| &packages[i]["id"])
            .into_iter()
            .collect();
        assert_eq!(actual, expected, "IR closure for {}", NAMES[index]);
    }
    packages
}

pub fn source(package: &Value) -> PathBuf {
    PathBuf::from(
        package["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["kind"].as_array().unwrap().iter().any(|k| k == "lib"))
            .unwrap()["src_path"]
            .as_str()
            .unwrap(),
    )
}

pub fn compile_reference(metadata: &Value, root: &Path) -> PathBuf {
    for (index, package) in packages(metadata).iter().enumerate() {
        let name = NAMES[index].replace('-', "_");
        let mut command = Command::new("rustc");
        command
            .arg(format!(
                "--edition={}",
                package["edition"].as_str().unwrap()
            ))
            .arg(format!("--crate-name={name}"))
            .args(["--crate-type=rlib", "-Cpanic=abort", "-L"])
            .arg(format!("dependency={}", root.display()))
            .arg(source(package))
            .arg("-o")
            .arg(root.join(format!("lib{name}.rlib")));
        if let Some(previous) = index.checked_sub(1) {
            let dependency = NAMES[previous].replace('-', "_");
            command.arg("--extern").arg(format!(
                "{dependency}={}",
                root.join(format!("lib{dependency}.rlib")).display()
            ));
        }
        let output = super::process::run(
            &mut command,
            root,
            &format!("rustc-{name}-library"),
            std::time::Duration::from_secs(60),
        )
        .completed();
        super::oracle::success(&output, &format!("rustc {name} library"));
    }
    root.join("libkeel_ir.rlib")
}

pub fn sources(db: &mut SourceDb, metadata: &Value) -> Vec<SourceCrate> {
    packages(metadata)
        .iter()
        .enumerate()
        .map(|(index, package)| {
            let edition = match package["edition"].as_str().unwrap() {
                "2018" => Edition::E2018,
                "2024" => Edition::E2024,
                _ => panic!("unexpected IR dependency edition"),
            };
            SourceCrate {
                root: db.load_with_edition(&source(package), edition).unwrap(),
                name: Symbol::intern(&NAMES[index].replace('-', "_")),
                dependencies: index
                    .checked_sub(1)
                    .map(|previous| (Symbol::intern(&NAMES[previous].replace('-', "_")), previous))
                    .into_iter()
                    .collect(),
            }
        })
        .collect()
}
