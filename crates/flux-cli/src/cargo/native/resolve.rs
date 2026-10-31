//! Native lockfile resolution. Metadata selection is separate from source compilation.
use super::{Features, Lock, Result};
use semver::{Version, VersionReq};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod catalog;
use catalog::{Catalog, Id};

#[derive(clap::Args)]
pub(crate) struct ResolveOptions {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    /// Use cached index records, archives and git checkouts only.
    #[arg(long)]
    offline: bool,
    /// Check that the resolution matches Cargo.lock without changing it.
    #[arg(long)]
    locked: bool,
    /// Sparse crates.io index or a mirror with the same package identities.
    #[arg(long, default_value = "https://index.crates.io")]
    index_url: String,
    /// Disable resolver 3's preference for Rust-version-compatible releases.
    #[arg(long)]
    ignore_rust_version: bool,
}
#[derive(Clone, Default)]
struct State {
    nodes: BTreeMap<Id, Features>,
    bindings: BTreeMap<(Id, usize), Id>,
}
#[derive(Clone)]
struct Request {
    parent: Id,
    dependency: usize,
}
struct Solver {
    catalog: Catalog,
    roots: BTreeSet<Id>,
    rust_version: Option<Version>,
    decisions_left: usize,
}

pub(super) fn run(options: &ResolveOptions) -> Result<ExitCode> {
    let manifest = super::canonical(&options.manifest_path)?;
    let (root, table) = super::workspace(&manifest)?;
    super::validate_configuration(&root, &table, false)?;
    let path = root.join("Cargo.lock");
    let original = if path.exists() {
        Some(std::fs::read(&path).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let previous: Option<Lock> = original
        .as_ref()
        .map(|b| {
            toml::from_str(std::str::from_utf8(b).map_err(|e| e.to_string())?)
                .map_err(|e| format!("invalid Cargo.lock: {e}"))
        })
        .transpose()?;
    if previous
        .as_ref()
        .is_some_and(|l| !matches!(l.version, 3 | 4))
    {
        return Err("expected Cargo.lock version 3 or 4".into());
    }
    if options.locked && previous.is_none() {
        return Err("--locked requires an existing Cargo.lock".into());
    }
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
        .ok_or("set CARGO_HOME to locate dependency metadata")?;
    let mut catalog = Catalog::new(home, options.index_url.clone(), options.offline);
    let excluded = super::strings(table.get("workspace").and_then(|w| w.get("exclude")))?;
    if excluded.iter().any(|p| p.contains(['*', '?', '['])) {
        return Err("workspace exclude globs are not supported yet".into());
    }
    let excluded: Vec<_> = excluded.iter().map(|p| root.join(p)).collect();
    let mut manifests = super::members(&root, &table, &manifest)?;
    manifests.retain(|p| !excluded.iter().any(|e| p.starts_with(e)));
    let mut roots = BTreeSet::new();
    let mut index = 0;
    while index < manifests.len() {
        let file = manifests[index].clone();
        index += 1;
        let id = catalog.local(&file, None)?;
        if !roots.insert(id.clone()) {
            continue;
        }
        // Path dependencies inside the workspace are implicit workspace members.
        let paths: Vec<_> = catalog.packages[&id]
            .dependencies
            .iter()
            .filter_map(|d| d.path.clone())
            .collect();
        for path in paths {
            let path = super::canonical(&path)?;
            if path.starts_with(&root)
                && !excluded.iter().any(|e| path.starts_with(e))
                && super::workspace(&path)?.0 == root
                && !manifests.contains(&path)
            {
                manifests.push(path);
            }
        }
    }
    if roots.is_empty() {
        return Err("workspace has no packages to resolve".into());
    }
    let resolver = table
        .get("workspace")
        .and_then(|w| w.get("resolver"))
        .or_else(|| table.get("package").and_then(|p| p.get("resolver")))
        .and_then(toml::Value::as_str);
    if resolver.is_some_and(|r| !matches!(r, "1" | "2" | "3")) {
        return Err("unsupported feature resolver".into());
    }
    let version_three = resolver == Some("3")
        || (resolver.is_none()
            && (table
                .get("package")
                .and_then(|p| p.get("edition"))
                .and_then(toml::Value::as_str)
                == Some("2024")
                || (table
                    .get("package")
                    .and_then(|p| p.get("edition"))
                    .and_then(|e| e.get("workspace"))
                    .and_then(toml::Value::as_bool)
                    == Some(true)
                    && table
                        .get("workspace")
                        .and_then(|w| w.get("package"))
                        .and_then(|p| p.get("edition"))
                        .and_then(toml::Value::as_str)
                        == Some("2024"))));
    let mut fallback = version_three;
    for path in super::configuration_paths(&root).into_iter().rev() {
        if path.is_file() {
            let configuration = super::read_table(&path)?;
            if let Some(policy) = configuration
                .get("resolver")
                .and_then(|r| r.get("incompatible-rust-versions"))
            {
                fallback = match policy.as_str() {
                    Some("fallback") => true,
                    Some("allow") => false,
                    _ => {
                        return Err(
                            "expected resolver.incompatible-rust-versions = allow or fallback"
                                .into(),
                        );
                    }
                };
            }
        }
    }
    let rust_version = if options.ignore_rust_version || !fallback {
        None
    } else {
        let minimum = roots
            .iter()
            .filter_map(|id| catalog.packages[id].rust_version.clone())
            .min();
        Some(if let Some(version) = minimum {
            version
        } else {
            let output = crate::cargo::output(
                Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
                    .arg("--version"),
                "rustc version",
            )?;
            Version::parse(
                output
                    .split_whitespace()
                    .nth(1)
                    .ok_or("invalid rustc version")?,
            )
            .map_err(|e| e.to_string())?
        })
    };
    let mut links = BTreeSet::new();
    for id in &roots {
        if let Some(link) = &catalog.packages[id].links
            && !links.insert(link.clone())
        {
            return Err(format!("conflicting native links declaration `{link}`"));
        }
    }
    let mut state = State::default();
    for id in &roots {
        state.nodes.insert(
            id.clone(),
            Features {
                names: catalog.packages[id].features.keys().cloned().collect(),
                ..Default::default()
            },
        );
    }
    let mut solver = Solver {
        catalog,
        roots,
        rust_version,
        decisions_left: 100_000,
    };
    let solution = solver.search(state, 0)?;
    let text = lockfile(&solution, &solver.catalog)?;
    let generated: Lock =
        toml::from_str(&text).map_err(|e| format!("cannot encode lockfile: {e}"))?;
    if let Some(old) = &previous {
        for package in &old.package {
            if let Some(new) = generated.package.iter().find(|p| {
                p.name == package.name && p.version == package.version && p.source == package.source
            }) && new.checksum != package.checksum
            {
                return Err(format!(
                    "registry checksum changed for {} {}",
                    package.name, package.version
                ));
            }
        }
    }
    let unchanged = previous
        .as_ref()
        .is_some_and(|old| same_resolution(old, &generated));
    if options.locked && !unchanged {
        return Err("resolution would change Cargo.lock; rerun without --locked".into());
    }
    let current = if path.exists() {
        Some(std::fs::read(&path).map_err(|e| e.to_string())?)
    } else {
        None
    };
    if current != original {
        return Err("Cargo.lock changed during resolution; retry with a stable lockfile".into());
    }
    if !unchanged {
        let temporary = super::fetch::Temporary::new(&root)?;
        let staged = temporary.0.join("Cargo.lock");
        std::fs::write(&staged, text).map_err(|e| e.to_string())?;
        std::fs::rename(&staged, &path).map_err(|e| format!("cannot install Cargo.lock: {e}"))?;
    }
    eprintln!(
        "resolved {} packages without Cargo: {}",
        solution.nodes.len(),
        path.display()
    );
    Ok(ExitCode::SUCCESS)
}
impl Solver {
    fn search(&mut self, mut state: State, depth: usize) -> Result<State> {
        if depth > 512 || self.decisions_left == 0 {
            return Err("dependency resolution exceeded its search limit".into());
        }
        self.decisions_left -= 1;
        let pending = self.propagate(&mut state)?;
        if pending.is_empty() {
            validate_cycles(&state, &self.catalog)?;
            return Ok(state);
        }
        let mut choice = None;
        for request in pending {
            let declaration =
                self.catalog.packages[&request.parent].dependencies[request.dependency].clone();
            let mut candidates = self.catalog.candidates(&declaration)?;
            candidates.retain(|id| {
                state.nodes.keys().all(|old| {
                    old.name != id.name
                        || old.source != id.source
                        || old == id
                        || !compatible(&old.version, &id.version)
                })
            });
            candidates.sort_by(|a, b| {
                let reused = state
                    .nodes
                    .contains_key(b)
                    .cmp(&state.nodes.contains_key(a));
                let rust =
                    self.rust_version
                        .as_ref()
                        .map_or(std::cmp::Ordering::Equal, |version| {
                            self.catalog
                                .rust_compatible(b, version)
                                .cmp(&self.catalog.rust_compatible(a, version))
                        });
                reused.then(rust).then_with(|| b.version.cmp(&a.version))
            });
            if candidates.is_empty() {
                return Err(format!(
                    "no compatible version for {} ({}) required by {} {}",
                    declaration.name,
                    declaration.requirement,
                    request.parent.name,
                    request.parent.version
                ));
            }
            if choice
                .as_ref()
                .is_none_or(|(_, ids): &(Request, Vec<Id>)| candidates.len() < ids.len())
            {
                choice = Some((request, candidates));
            }
        }
        let (request, candidates) = choice.ok_or("missing dependency choice")?;
        let mut last_error = String::new();
        for id in candidates {
            self.catalog.ensure(&id)?;
            if let Some(links) = &self.catalog.packages[&id].links
                && state.nodes.keys().any(|old| {
                    old != &id && self.catalog.packages[old].links.as_ref() == Some(links)
                })
            {
                last_error = format!("conflicting native links declaration `{links}`");
                continue;
            }
            let mut branch = state.clone();
            branch.nodes.entry(id.clone()).or_default();
            branch
                .bindings
                .insert((request.parent.clone(), request.dependency), id);
            match self.search(branch, depth + 1) {
                Ok(done) => return Ok(done),
                Err(error) => last_error = error,
            }
            if self.decisions_left == 0 {
                return Err("dependency resolution exceeded its search limit".into());
            }
        }
        Err(last_error)
    }
    fn propagate(&self, state: &mut State) -> Result<Vec<Request>> {
        loop {
            let mut changed = false;
            let ids: Vec<_> = state.nodes.keys().cloned().collect();
            let mut pending = Vec::new();
            for id in ids {
                let package = &self.catalog.packages[&id];
                let mut features = state.nodes[&id].clone();
                super::expand_features(
                    &package.features,
                    &package.optional,
                    &package.required,
                    &mut features,
                )
                .map_err(|e| format!("{e} in {} {}", id.name, id.version))?;
                // Cargo's lockfile pass retains weak forwarding edges. Build planning
                // still uses ordinary weak activation through expand_features above.
                for feature in &features.names {
                    for value in package.features.get(feature).into_iter().flatten() {
                        if let Some((alias, forwarded)) = value.split_once("?/") {
                            features.optional.insert(alias.into());
                            features
                                .forwarded
                                .entry(alias.into())
                                .or_default()
                                .insert(forwarded.into());
                        }
                    }
                }
                if features != state.nodes[&id] {
                    changed = true;
                    state.nodes.insert(id.clone(), features.clone());
                }
                for (index, dependency) in package.dependencies.iter().enumerate() {
                    if dependency.dev && !self.roots.contains(&id) {
                        continue;
                    }
                    if dependency.optional && !features.optional.contains(&dependency.alias) {
                        continue;
                    }
                    let Some(child) = state.bindings.get(&(id.clone(), index)).cloned() else {
                        pending.push(Request {
                            parent: id.clone(),
                            dependency: index,
                        });
                        continue;
                    };
                    let metadata = &self.catalog.packages[&child];
                    let requested = state
                        .nodes
                        .get_mut(&child)
                        .ok_or("missing selected dependency")?;
                    for feature in dependency.features.iter().chain(
                        features
                            .forwarded
                            .get(&dependency.alias)
                            .into_iter()
                            .flatten(),
                    ) {
                        changed |= requested.names.insert(feature.clone());
                    }
                    if dependency.default_features && metadata.features.contains_key("default") {
                        changed |= requested.names.insert("default".into());
                    }
                }
            }
            if !changed {
                return Ok(pending);
            }
        }
    }
}
fn compatible(a: &Version, b: &Version) -> bool {
    if a.major != 0 || b.major != 0 {
        a.major == b.major
    } else if a.minor != 0 || b.minor != 0 {
        a.minor == b.minor
    } else {
        a.patch == b.patch
    }
}
fn validate_cycles(state: &State, catalog: &Catalog) -> Result<()> {
    fn visit(
        id: &Id,
        state: &State,
        catalog: &Catalog,
        active: &mut BTreeSet<Id>,
        done: &mut BTreeSet<Id>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if !active.insert(id.clone()) {
            return Err(format!("dependency cycle at {} {}", id.name, id.version));
        }
        for ((parent, index), child) in &state.bindings {
            if parent == id && !catalog.packages[parent].dependencies[*index].dev {
                visit(child, state, catalog, active, done)?;
            }
        }
        active.remove(id);
        done.insert(id.clone());
        Ok(())
    }
    let mut done = BTreeSet::new();
    for id in state.nodes.keys() {
        visit(id, state, catalog, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}
fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}
fn lockfile(state: &State, catalog: &Catalog) -> Result<String> {
    let mut text = String::from(
        "# This file is automatically @generated by Flux.\n# It is not intended for manual editing.\nversion = 4\n",
    );
    for id in state.nodes.keys() {
        text.push_str(&format!(
            "\n[[package]]\nname = {}\nversion = {}\n",
            quote(&id.name)?,
            quote(&id.version.to_string())?
        ));
        if let Some(source) = &id.source {
            text.push_str(&format!("source = {}\n", quote(source)?));
        }
        if let Some(checksum) = &catalog.packages[id].checksum {
            text.push_str(&format!("checksum = {}\n", quote(checksum)?));
        }
        let dependencies: BTreeSet<_> = state
            .bindings
            .iter()
            .filter(|((p, _), _)| p == id)
            .map(|(_, d)| d)
            .collect();
        if !dependencies.is_empty() {
            text.push_str("dependencies = [\n");
            for dependency in dependencies {
                let count = state
                    .nodes
                    .keys()
                    .filter(|p| p.name == dependency.name)
                    .count();
                let mut name = dependency.name.clone();
                if count > 1 {
                    name.push(' ');
                    name.push_str(&dependency.version.to_string());
                    if state
                        .nodes
                        .keys()
                        .filter(|p| p.name == dependency.name && p.version == dependency.version)
                        .count()
                        > 1
                        && let Some(source) = &dependency.source
                    {
                        name.push_str(&format!(" ({})", super::source_reference(source)));
                    }
                }
                text.push_str(&format!(" {},\n", quote(&name)?));
            }
            text.push_str("]\n");
        }
    }
    Ok(text)
}
fn same_resolution(a: &Lock, b: &Lock) -> bool {
    type Packages = BTreeMap<(String, String, Option<String>), (Option<String>, BTreeSet<Id>)>;
    fn packages(lock: &Lock) -> Option<Packages> {
        let mut packages = BTreeMap::new();
        for package in &lock.package {
            let mut dependencies = BTreeSet::new();
            for entry in &package.dependencies {
                let mut parts = entry.splitn(3, ' ');
                let name = parts.next()?;
                let version = parts.next();
                let source = parts.next().map(|s| s.trim_matches(['(', ')']));
                let mut matches: Vec<_> = lock
                    .package
                    .iter()
                    .filter(|d| {
                        d.name == name
                            && version.is_none_or(|v| v == d.version)
                            && source.is_none_or(|s| {
                                d.source
                                    .as_deref()
                                    .is_some_and(|actual| super::lock_source_matches(s, actual))
                            })
                    })
                    .collect();
                if matches.len() > 1 && source.is_none() {
                    matches.retain(|d| d.source.is_none());
                }
                if matches.len() != 1 {
                    return None;
                }
                let dependency = matches[0];
                dependencies.insert(Id {
                    name: dependency.name.clone(),
                    version: Version::parse(&dependency.version).ok()?,
                    source: dependency.source.clone(),
                });
            }
            if packages
                .insert(
                    (
                        package.name.clone(),
                        package.version.clone(),
                        package.source.clone(),
                    ),
                    (package.checksum.clone(), dependencies),
                )
                .is_some()
            {
                return None;
            }
        }
        Some(packages)
    }
    packages(a).zip(packages(b)).is_some_and(|(a, b)| a == b)
}
