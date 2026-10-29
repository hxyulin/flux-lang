//! Manifest and target discovery for the standalone, path-only package driver.
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug)]
pub(super) struct Package {
    pub(super) name: String,
    pub(super) targets: Vec<Target>,
}

#[derive(Debug)]
pub(super) struct Target {
    pub(super) name: String,
    pub(super) kind: Vec<String>,
    pub(super) crate_types: Vec<String>,
    pub(super) src_path: PathBuf,
    pub(super) edition: String,
}

#[derive(Deserialize)]
struct PackageFields {
    name: String,
    edition: String,
    version: Option<String>,
    #[serde(rename = "rust-version")]
    rust_version: Option<String>,
    autolib: Option<bool>,
    autobins: Option<bool>,
    autoexamples: Option<bool>,
    autotests: Option<bool>,
    autobenches: Option<bool>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct TargetFields {
    name: Option<String>,
    path: Option<String>,
    edition: Option<String>,
    proc_macro: Option<bool>,
    crate_type: Option<Vec<String>>,
}

pub(super) fn load(manifest: &Path) -> Result<Package, String> {
    let root = manifest.parent().ok_or("expected a package directory")?;
    let table = super::read_toml(manifest)?;
    reject_workspace(root, &table)?;
    let package: PackageFields = table
        .get("package")
        .ok_or("expected a standalone [package]")?
        .clone()
        .try_into()
        .map_err(|e| format!("invalid package: {e}"))?;
    validate_package(&package)?;
    validate_descriptive_fields(
        table
            .get("package")
            .and_then(toml::Value::as_table)
            .ok_or("expected a package table")?,
    )?;
    if let Some(features) = table.get("features") {
        let features = features.as_table().ok_or("expected a features table")?;
        if !features.is_empty() {
            return Err("features are not supported yet".into());
        }
    }
    validate_inert_dependencies(&table)?;
    let mut targets = Vec::new();
    if let Some(lib) = table.get("lib") {
        let fields = target_fields(lib)?;
        let name = fields
            .name
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| package.name.replace('-', "_"));
        if name.contains('-') {
            return Err("library target names cannot contain hyphens".into());
        }
        let kind = if fields.proc_macro == Some(true) {
            "proc-macro"
        } else {
            "lib"
        };
        targets.push(target(
            root,
            &package.edition,
            kind,
            name,
            fields.path.as_deref().unwrap_or("src/lib.rs"),
            &fields,
        )?);
    } else if package.autolib != Some(false) && root.join("src/lib.rs").is_file() {
        targets.push(target(
            root,
            &package.edition,
            "lib",
            package.name.replace('-', "_"),
            "src/lib.rs",
            &TargetFields::default(),
        )?);
    }
    discover_targets(
        root,
        &table,
        &package,
        "bin",
        "src/bin",
        package.autobins != Some(false),
        &mut targets,
    )?;
    for (kind, directory, enabled) in [
        ("example", "examples", package.autoexamples),
        ("test", "tests", package.autotests),
        ("bench", "benches", package.autobenches),
    ] {
        discover_targets(
            root,
            &table,
            &package,
            kind,
            directory,
            enabled != Some(false),
            &mut targets,
        )?;
    }
    if targets.is_empty() {
        return Err("no targets specified in the manifest".into());
    }
    Ok(Package {
        name: package.name,
        targets,
    })
}

fn reject_workspace(root: &Path, table: &toml::Table) -> Result<(), String> {
    if table.contains_key("workspace")
        || table
            .get("package")
            .and_then(|package| package.get("workspace"))
            .is_some()
    {
        return Err("workspaces are not supported yet; expected one standalone package".into());
    }
    for ancestor in root.ancestors().skip(1) {
        let path = ancestor.join("Cargo.toml");
        if path.is_file() && super::read_toml(&path)?.contains_key("workspace") {
            return Err("workspaces are not supported yet; expected one standalone package".into());
        }
    }
    Ok(())
}

fn validate_package(package: &PackageFields) -> Result<(), String> {
    validate_name(&package.name, "package")?;
    super::edition(&package.edition)?;
    if let Some(version) = &package.version {
        validate_version(version)?;
    }
    if let Some(version) = &package.rust_version {
        let parts: Vec<_> = version.split('.').collect();
        if !(2..=3).contains(&parts.len())
            || parts.iter().any(|part| !version_number(part))
            || parts[0] != "1"
        {
            return Err("expected a rust-version such as `1.85`".into());
        }
    }

    Ok(())
}

fn validate_name(name: &str, kind: &str) -> Result<(), String> {
    if name.starts_with('-') || !flux_proc_macro::is_identifier(&name.replace('-', "_")) {
        return Err(format!("invalid {kind} name `{name}`"));
    }
    Ok(())
}

fn version_number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && (value.len() == 1 || !value.starts_with('0'))
        && value.parse::<u64>().is_ok()
}

fn validate_version(version: &str) -> Result<(), String> {
    let (version, build) = version
        .split_once('+')
        .map_or((version, None), |(version, build)| (version, Some(build)));
    let (core, pre) = version
        .split_once('-')
        .map_or((version, None), |(core, pre)| (core, Some(pre)));
    let parts: Vec<_> = core.split('.').collect();
    let valid_identifiers = |value: &str, prerelease: bool| {
        value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
                && (!prerelease
                    || !part.bytes().all(|ch| ch.is_ascii_digit())
                    || part.len() == 1
                    || !part.starts_with('0'))
        })
    };
    if parts.len() != 3
        || !parts.into_iter().all(version_number)
        || pre.is_some_and(|value| !valid_identifiers(value, true))
        || build.is_some_and(|value| !valid_identifiers(value, false))
    {
        return Err("expected a semantic package version such as `0.1.0`".into());
    }
    Ok(())
}

fn target_fields(value: &toml::Value) -> Result<TargetFields, String> {
    let fields = value.as_table().ok_or("expected a target table")?;
    for name in ["test", "bench", "doctest", "harness", "doc"] {
        if fields.get(name).is_some_and(|value| !value.is_bool()) {
            return Err(format!("expected a boolean target `{name}`"));
        }
    }
    if let Some(features) = fields.get("required-features") {
        validate_strings(features, "required-features")?;
    }
    value
        .clone()
        .try_into()
        .map_err(|e| format!("invalid target: {e}"))
}

fn validate_strings(value: &toml::Value, name: &str) -> Result<(), String> {
    if value
        .as_array()
        .is_none_or(|values| values.iter().any(|value| !value.is_str()))
    {
        return Err(format!("expected an array of strings for `{name}`"));
    }
    Ok(())
}

fn validate_inert_dependencies(table: &toml::Table) -> Result<(), String> {
    if let Some(dependencies) = table.get("dev-dependencies") {
        for (name, value) in dependencies
            .as_table()
            .ok_or("expected a dev-dependencies table")?
        {
            if value.is_str() {
                continue;
            }
            let fields = value
                .as_table()
                .ok_or_else(|| format!("expected a dev-dependency specification for `{name}`"))?;
            for key in [
                "version",
                "path",
                "package",
                "git",
                "branch",
                "tag",
                "rev",
                "registry",
                "registry-index",
            ] {
                if fields.get(key).is_some_and(|value| !value.is_str()) {
                    return Err(format!("expected a string dev-dependency `{key}`"));
                }
            }
            for key in ["optional", "default-features", "default_features"] {
                if fields.get(key).is_some_and(|value| !value.is_bool()) {
                    return Err(format!("expected a boolean dev-dependency `{key}`"));
                }
            }
            if let Some(features) = fields.get("features") {
                validate_strings(features, "dev-dependency features")?;
            }
            if fields.contains_key("workspace") {
                return Err("workspace dependency inheritance is not supported yet".into());
            }
        }
    }
    if let Some(targets) = table.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            validate_inert_dependencies(target.as_table().ok_or("expected a target table")?)?;
        }
    }
    Ok(())
}

fn validate_descriptive_fields(fields: &toml::Table) -> Result<(), String> {
    for name in [
        "description",
        "license",
        "license-file",
        "repository",
        "homepage",
        "documentation",
    ] {
        if fields.get(name).is_some_and(|value| !value.is_str()) {
            return Err(format!("expected a string for package `{name}`"));
        }
    }
    for name in ["authors", "keywords", "categories"] {
        if let Some(value) = fields.get(name) {
            validate_strings(value, name)?;
        }
    }
    if fields
        .get("readme")
        .is_some_and(|value| !value.is_bool() && !value.is_str())
    {
        return Err("expected a string or boolean for package `readme`".into());
    }
    if let Some(value) = fields.get("publish")
        && !value.is_bool()
    {
        validate_strings(value, "publish")?;
    }
    if fields
        .get("metadata")
        .is_some_and(|value| !value.is_table())
    {
        return Err("expected a table for package `metadata`".into());
    }
    Ok(())
}

fn target(
    root: &Path,
    edition: &str,
    kind: &str,
    name: String,
    path: &str,
    fields: &TargetFields,
) -> Result<Target, String> {
    if kind == "lib" || kind == "proc-macro" {
        validate_name(&name, "library target")?;
    } else if name.is_empty()
        || name.contains(['/', '\\'])
        || name == "."
        || name == ".."
        || name.chars().any(char::is_control)
    {
        return Err(format!("invalid {kind} target name `{name}`"));
    }
    let edition = fields.edition.as_deref().unwrap_or(edition);
    super::edition(edition)?;
    Ok(Target {
        name,
        kind: vec![kind.to_owned()],
        crate_types: fields.crate_type.clone().unwrap_or_else(|| {
            vec![
                if kind == "proc-macro" {
                    "proc-macro"
                } else if kind == "lib" {
                    "lib"
                } else {
                    "bin"
                }
                .to_owned(),
            ]
        }),
        src_path: root.join(path),
        edition: edition.to_owned(),
    })
}

fn discover_targets(
    root: &Path,
    table: &toml::Table,
    package: &PackageFields,
    kind: &str,
    directory: &str,
    automatic: bool,
    targets: &mut Vec<Target>,
) -> Result<(), String> {
    let mut inferred = discover_directory(root, directory)?;
    if kind == "bin" && root.join("src/main.rs").is_file() {
        inferred.push((package.name.clone(), "src/main.rs".to_owned()));
    }
    let mut selected = Vec::new();
    if let Some(declarations) = table.get(kind) {
        for declaration in declarations
            .as_array()
            .ok_or_else(|| format!("expected [[{kind}]] declarations"))?
        {
            let fields = target_fields(declaration)?;
            let name = fields
                .name
                .as_deref()
                .ok_or_else(|| format!("expected a {kind} target name"))?;
            let path = if let Some(path) = &fields.path {
                path.clone()
            } else {
                let candidates: Vec<_> = inferred
                    .iter()
                    .filter(|(candidate, _)| candidate == name)
                    .collect();
                match candidates.as_slice() {
                    [(_, path)] => path.clone(),
                    [] => {
                        return Err(format!(
                            "cannot infer source path for {kind} target `{name}`"
                        ));
                    }
                    _ => return Err(format!("ambiguous source path for {kind} target `{name}`")),
                }
            };
            if selected.iter().any(|target: &Target| target.name == name) {
                return Err(format!("duplicate {kind} target name `{name}`"));
            }
            selected.push(target(
                root,
                &package.edition,
                kind,
                name.to_owned(),
                &path,
                &fields,
            )?);
        }
    }
    if automatic {
        for (name, path) in inferred {
            let source = root.join(&path);
            if selected
                .iter()
                .any(|target| target.name == name || target.src_path == source)
            {
                continue;
            }
            // Inferred names must also be unique, rather than silently choosing
            // between `src/bin/name.rs` and `src/bin/name/main.rs`.
            if targets
                .iter()
                .any(|target| target.kind == [kind] && target.name == name)
            {
                return Err(format!("duplicate {kind} target name `{name}`"));
            }
            targets.push(target(
                root,
                &package.edition,
                kind,
                name,
                &path,
                &TargetFields::default(),
            )?);
        }
    }
    targets.extend(selected);
    Ok(())
}

fn discover_directory(root: &Path, directory: &str) -> Result<Vec<(String, String)>, String> {
    let directory_path = root.join(directory);
    let entries = match std::fs::read_dir(&directory_path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot read {}: {error}", directory_path.display())),
    };
    let mut targets = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read target directory: {error}"))?
            .path();
        let (name, source) =
            if path.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
                (path.file_stem(), path.clone())
            } else if path.is_dir() && path.join("main.rs").is_file() {
                (path.file_name(), path.join("main.rs"))
            } else {
                continue;
            };
        let name = name
            .and_then(|name| name.to_str())
            .ok_or("expected a UTF-8 target name")?;
        let source = source
            .strip_prefix(root)
            .map_err(|_| "expected a package target path")?;
        let source = source.to_str().ok_or("expected a UTF-8 target path")?;
        targets.push((name.to_owned(), source.to_owned()));
    }
    targets.sort();
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new(manifest: &str, files: &[&str]) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "flux-manifest-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("Cargo.toml"), manifest).unwrap();
            for file in files {
                let path = root.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, "").unwrap();
            }
            Self(root)
        }

        fn load(&self) -> Result<Package, String> {
            load(&self.0.join("Cargo.toml"))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    const PACKAGE: &str = "[package]\nname='sample'\nedition='2024'\n";

    #[test]
    fn discovers_file_and_directory_targets_without_version() {
        let fixture = Fixture::new(
            PACKAGE,
            &[
                "src/lib.rs",
                "src/main.rs",
                "src/bin/tool.rs",
                "src/bin/nested/main.rs",
                "tests/test.rs",
                "examples/example/main.rs",
                "benches/bench.rs",
                "src/bin/ignored/helper.rs",
            ],
        );
        let package = fixture.load().unwrap();
        let mut targets: Vec<_> = package
            .targets
            .iter()
            .map(|target| {
                (
                    target.kind[0].as_str(),
                    target.name.as_str(),
                    target
                        .src_path
                        .strip_prefix(&fixture.0)
                        .unwrap()
                        .to_str()
                        .unwrap(),
                )
            })
            .collect();
        targets.sort();
        assert_eq!(
            targets,
            [
                ("bench", "bench", "benches/bench.rs"),
                ("bin", "nested", "src/bin/nested/main.rs"),
                ("bin", "sample", "src/main.rs"),
                ("bin", "tool", "src/bin/tool.rs"),
                ("example", "example", "examples/example/main.rs"),
                ("lib", "sample", "src/lib.rs"),
                ("test", "test", "tests/test.rs"),
            ]
        );
    }

    #[test]
    fn explicit_targets_override_inferred_names_and_paths() {
        let fixture = Fixture::new(
            &format!(
                "{PACKAGE}[[bin]]\nname='renamed'\npath='src/main.rs'\n[[bin]]\nname='tool'\npath='custom.rs'\n"
            ),
            &["src/main.rs", "src/bin/tool.rs", "custom.rs"],
        );
        let package = fixture.load().unwrap();
        assert_eq!(package.targets.len(), 2);
        assert_eq!(package.targets[0].name, "renamed");
        assert_eq!(package.targets[1].name, "tool");
        assert_eq!(package.targets[1].src_path, fixture.0.join("custom.rs"));
    }

    #[test]
    fn explicit_targets_remain_when_discovery_is_disabled() {
        let fixture = Fixture::new(
            &format!(
                "{PACKAGE}autolib=false\nautobins=false\nautoexamples=false\nautotests=false\nautobenches=false\n[[bin]]\nname='chosen'\npath='entry.rs'\n"
            ),
            &[
                "src/lib.rs",
                "src/main.rs",
                "src/bin/tool.rs",
                "tests/test.rs",
                "examples/example.rs",
                "benches/bench.rs",
                "entry.rs",
            ],
        );
        let package = fixture.load().unwrap();
        assert_eq!(package.targets.len(), 1);
        assert_eq!(package.targets[0].name, "chosen");
    }

    #[test]
    fn infers_explicit_paths_and_rejects_ambiguity_or_duplicate_names() {
        let fixture = Fixture::new(
            &format!("{PACKAGE}autobins=false\n[[bin]]\nname='tool'\n"),
            &["src/bin/tool/main.rs"],
        );
        assert_eq!(
            fixture.load().unwrap().targets[0].src_path,
            fixture.0.join("src/bin/tool/main.rs")
        );
        for (extra, files, expected) in [
            (
                "[[bin]]\nname='tool'\n",
                vec!["src/bin/tool.rs", "src/bin/tool/main.rs"],
                "ambiguous source",
            ),
            (
                "",
                vec!["src/main.rs", "src/bin/sample.rs"],
                "duplicate bin",
            ),
            (
                "[[bin]]\nname='tool'\npath='a.rs'\n[[bin]]\nname='tool'\npath='b.rs'\n",
                vec!["a.rs", "b.rs"],
                "duplicate bin",
            ),
            (
                "[[bin]]\npath='src/main.rs'\n",
                vec!["src/main.rs"],
                "target name",
            ),
        ] {
            let fixture = Fixture::new(&format!("{PACKAGE}{extra}"), &files);
            assert!(fixture.load().unwrap_err().contains(expected), "{extra}");
        }
    }

    #[test]
    fn library_name_defaults_and_proc_macro_kind_are_preserved() {
        let fixture = Fixture::new(
            "[package]\nname='macro-package'\nedition='2021'\n[lib]\nproc-macro=true\npath='macro.rs'\n",
            &["macro.rs"],
        );
        let package = fixture.load().unwrap();
        let target = &package.targets[0];
        assert_eq!(target.name, "macro_package");
        assert_eq!(target.kind, ["proc-macro"]);
        assert_eq!(target.crate_types, ["proc-macro"]);
        assert_eq!(target.edition, "2021");
    }

    #[test]
    fn checks_package_versions_and_identifier_names() {
        for version in [
            "0.1.0",
            "1.2.3-alpha.1+build.01",
            "1.2.3+1.2",
            "1.2.3-999999999999999999999999999999",
        ] {
            validate_version(version).unwrap();
        }
        for version in [
            "1",
            "1.2",
            "01.2.3",
            "1.2.3-01",
            "1.2.3-",
            "1.2.3+",
            "1.2.3+a+b",
        ] {
            assert!(validate_version(version).is_err(), "{version}");
        }
        for name in ["sample", "sample-name", "föö", "_name", "_"] {
            validate_name(name, "package").unwrap();
        }
        for name in [
            "",
            "1sample",
            "-sample",
            "--",
            "foo/bar",
            "a b",
            "a!",
            "a\u{037a}",
        ] {
            assert!(validate_name(name, "package").is_err(), "{name}");
        }
    }
}
