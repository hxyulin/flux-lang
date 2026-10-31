//! Read-only, locked planning. Host tools are scheduled, never executed here.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::{Deserialize, Serialize};

use super::Options;

mod execute;
mod fetch;
mod resolve;
pub(crate) use fetch::FetchOptions;
pub(crate) use resolve::ResolveOptions;

type Table = toml::Table;
type Result<T> = std::result::Result<T, String>;

#[derive(Deserialize)]
struct Lock {
    version: u32,
    package: Vec<Locked>,
}
#[derive(Deserialize)]
struct Locked {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}
#[derive(Clone)]
struct Dependency {
    alias: String,
    spec: Table,
    build: bool,
    condition: Option<String>,
}
struct Package {
    manifest: PathBuf,
    table: Table,
    fields: Table,
    features: BTreeMap<String, Vec<String>>,
    dependencies: Vec<Dependency>,
    library: Option<Target>,
    script: Option<PathBuf>,
}
#[derive(Clone, Serialize)]
struct Target {
    name: String,
    root: PathBuf,
    edition: String,
    kind: String,
    required_features: Vec<String>,
}
// Resolver 2/3 unify normal library and binary features, but not host/target features.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Domain {
    Host,
    Target,
}
#[derive(Default, Clone, PartialEq, Eq)]
struct Features {
    names: BTreeSet<String>,
    optional: BTreeSet<String>,
    forwarded: BTreeMap<String, BTreeSet<String>>,
}
#[derive(Serialize)]
struct Plan {
    version: u32,
    kind: &'static str,
    host: String,
    target: String,
    workspace_root: PathBuf,
    lockfile: PathBuf,
    packages: Vec<PlannedPackage>,
    units: Vec<Unit>,
    root_unit: usize,
}
#[derive(Serialize)]
struct PlannedPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    manifest: PathBuf,
    links: Option<String>,
}
#[derive(Serialize)]
struct Unit {
    package: usize,
    domain: Domain,
    feature_domain: Domain,
    target: Target,
    features: Vec<String>,
    dependencies: Vec<(String, usize)>,
    build_script: Option<usize>,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    package: usize,
    domain: Domain,
    kind: String,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ResolvedDependency {
    alias: String,
    package: usize,
    domain: Domain,
    build: bool,
}

struct Planner {
    lock: Lock,
    packages: BTreeMap<usize, Package>,
    states: BTreeMap<(usize, Domain), Features>,
    edges: BTreeMap<(usize, Domain), Vec<ResolvedDependency>>,
    home: PathBuf,
    cfg: BTreeSet<String>,
    host: String,
}

fn make_plan(options: &Options) -> Result<Plan> {
    let manifest = canonical(&options.manifest_path)?;
    let (workspace, workspace_table) = workspace(&manifest)?;
    validate_overrides(&workspace, &workspace_table)?;
    let lockfile = workspace.join("Cargo.lock");
    let lock: Lock = toml::from_str(&read(&lockfile)?)
        .map_err(|e| format!("invalid {}: {e}", lockfile.display()))?;
    if !matches!(lock.version, 3 | 4) {
        return Err("native planning requires Cargo.lock version 3 or 4".into());
    }
    let host = crate::target::host()?.to_owned();
    let cfg = crate::target::cfg()?.iter().map(|s| compact(s)).collect();
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
        .ok_or("set CARGO_HOME to locate offline sources")?;
    let mut planner = Planner {
        lock,
        packages: BTreeMap::new(),
        states: BTreeMap::new(),
        edges: BTreeMap::new(),
        home,
        cfg,
        host: host.clone(),
    };
    let members = members(&workspace, &workspace_table, &manifest)?;
    let mut selected = Vec::new();
    for path in members {
        let package = load(&path)?;
        let name = string(&package.fields, "name")?;
        if options.package.as_deref().is_none_or(|n| n == name) {
            selected.push((path, package));
        }
    }
    let (_path, package) = if options.package.is_none()
        && (manifest != workspace.join("Cargo.toml") || workspace_table.contains_key("package"))
    {
        (manifest.clone(), load(&manifest)?)
    } else if selected.len() == 1 {
        selected.pop().expect("one selected package")
    } else {
        return Err("select one workspace package with --package".into());
    };
    let root = planner.identify(&package, None)?;
    let binary = binary(&package, options.bin.as_deref())?;
    planner.packages.insert(root, package);
    let features = planner.states.entry((root, Domain::Target)).or_default();
    for feature in options.features.iter().flat_map(|s| s.split_whitespace()) {
        if let Some((dep, name)) = feature.split_once('/') {
            if !planner.packages[&root]
                .dependencies
                .iter()
                .any(|d| d.alias == dep)
            {
                return Err(format!("unknown dependency `{dep}` in requested feature"));
            }
            features.optional.insert(dep.into());
            features
                .forwarded
                .entry(dep.into())
                .or_default()
                .insert(name.into());
            if planner.packages[&root]
                .features
                .get(dep)
                .is_some_and(|v| v == &[format!("dep:{dep}")])
            {
                features.names.insert(dep.into());
            }
        } else {
            features.names.insert(feature.into());
        }
    }
    if !options.no_default_features && planner.packages[&root].features.contains_key("default") {
        features.names.insert("default".into());
    }
    planner.resolve()?;
    if binary
        .required_features
        .iter()
        .any(|f| !planner.states[&(root, Domain::Target)].names.contains(f))
    {
        return Err("selected binary requires disabled features".into());
    }
    let mut units = Vec::new();
    let mut indices = BTreeMap::new();
    let mut active = BTreeSet::new();
    let root_unit = planner.unit(
        Key {
            package: root,
            domain: Domain::Target,
            kind: "bin".into(),
        },
        Some(&binary),
        &mut units,
        &mut indices,
        &mut active,
    )?;
    let used: BTreeSet<_> = units.iter().map(|u| u.package).collect();
    let package_indices: BTreeMap<_, _> = used.iter().enumerate().map(|(i, &p)| (p, i)).collect();
    let packages = used
        .into_iter()
        .map(|id| {
            let locked = &planner.lock.package[id];
            PlannedPackage {
                name: locked.name.clone(),
                version: locked.version.clone(),
                source: locked.source.clone(),
                checksum: locked.checksum.clone(),
                manifest: planner.packages[&id].manifest.clone(),
                links: planner.packages[&id]
                    .fields
                    .get("links")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
            }
        })
        .collect();
    for unit in &mut units {
        unit.package = package_indices[&unit.package];
    }
    let plan = Plan {
        version: 1,
        kind: "flux-native-offline",
        host: host.clone(),
        target: host,
        workspace_root: workspace.clone(),
        lockfile,
        packages,
        units,
        root_unit,
    };
    Ok(plan)
}

pub(super) fn execute(options: &Options) -> Result<ExitCode> {
    let plan = make_plan(options)?;
    let output = options
        .plan
        .clone()
        .unwrap_or_else(|| plan.workspace_root.join("target/flux/native-plan.json"));
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    eprintln!(
        "planned {} packages and {} build units without Cargo: {}",
        plan.packages.len(),
        plan.units.len(),
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}

impl Planner {
    fn identify(&self, package: &Package, source: Option<&str>) -> Result<usize> {
        let name = string(&package.fields, "name")?;
        let version = string(&package.fields, "version")?;
        let candidates: Vec<_> = self
            .lock
            .package
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.name == name && p.version == version && p.source.as_deref() == source
            })
            .collect();
        match candidates.as_slice() {
            [(id, _)] => Ok(*id),
            _ => Err(format!(
                "manifest {name} {version} is missing or ambiguous in Cargo.lock; refresh it with Cargo"
            )),
        }
    }

    fn dependency(&mut self, parent: usize, dep: &Dependency) -> Result<usize> {
        let name = dep
            .spec
            .get("package")
            .and_then(toml::Value::as_str)
            .unwrap_or(&dep.alias);
        let req = dep
            .spec
            .get("version")
            .and_then(toml::Value::as_str)
            .map(semver::VersionReq::parse)
            .transpose()
            .map_err(|e| format!("invalid dependency version: {e}"))?;
        if let Some(path) = dep.spec.get("path").and_then(toml::Value::as_str) {
            let manifest = canonical(
                &self.packages[&parent]
                    .manifest
                    .parent()
                    .ok_or("manifest has no parent")?
                    .join(path)
                    .join("Cargo.toml"),
            )?;
            let package = load(&manifest)?;
            if string(&package.fields, "name")? != name {
                return Err(format!(
                    "path dependency {name} has a different package name"
                ));
            }
            let source = self.lock.package[parent]
                .source
                .as_deref()
                .filter(|s| s.starts_with("git+"));
            let id = self.identify(&package, source)?;
            self.check_locked_edge(parent, id)?;
            check_version(&self.lock.package[id], req.as_ref())?;
            if let Some(old) = self.packages.get(&id) {
                if old.manifest != manifest {
                    return Err(format!("conflicting paths for {name}"));
                }
            } else {
                self.packages.insert(id, package);
            }
            return Ok(id);
        }
        if dep.spec.contains_key("registry") || dep.spec.contains_key("registry-index") {
            return Err("alternate registries are not supported by native planning yet".into());
        }
        let git = dep.spec.get("git").and_then(toml::Value::as_str);
        let candidates: Vec<_> = self
            .lock
            .package
            .iter()
            .enumerate()
            .filter(|(id, p)| {
                p.name == name
                    && check_version(p, req.as_ref()).is_ok()
                    && p.source.as_ref().is_some_and(|s| match git {
                        Some(url) => git_source_matches(s, url, &dep.spec),
                        None => s == "registry+https://github.com/rust-lang/crates.io-index",
                    })
                    && self.check_locked_edge(parent, *id).is_ok()
            })
            .map(|(i, _)| i)
            .collect();
        let id = match candidates.as_slice() {
            [id] => *id,
            _ => {
                return Err(format!(
                    "locked dependency {name} is missing or ambiguous; refresh Cargo.lock with Cargo"
                ));
            }
        };
        if !self.packages.contains_key(&id) {
            let manifest = self.cached(id)?;
            let package = load(&manifest)?;
            if self.identify(&package, self.lock.package[id].source.as_deref())? != id {
                return Err(format!("cached manifest does not match locked {name}"));
            }
            self.packages.insert(id, package);
        }
        Ok(id)
    }

    fn check_locked_edge(&self, parent: usize, child: usize) -> Result<()> {
        let package = &self.lock.package[child];
        let valid = self.lock.package[parent].dependencies.iter().any(|entry| {
            let mut parts = entry.splitn(3, ' ');
            parts.next() == Some(&package.name)
                && parts.next().is_none_or(|v| v == package.version)
                && parts.next().is_none_or(|s| {
                    package.source.as_deref().is_some_and(|source| {
                        lock_source_matches(s.trim_matches(['(', ')']), source)
                    })
                })
        });
        if valid {
            Ok(())
        } else {
            Err(format!(
                "dependency {} is absent from the parent's locked edges",
                package.name
            ))
        }
    }

    fn cached(&self, id: usize) -> Result<PathBuf> {
        let package = &self.lock.package[id];
        cached_source(&self.home, package)?.ok_or_else(|| {
            format!(
                "offline source missing for {} {}; run `flux fetch --locked` first",
                package.name, package.version
            )
        })
    }

    fn resolve(&mut self) -> Result<()> {
        loop {
            let previous = self.states.clone();
            for (id, domain) in previous.keys().copied() {
                let mut state = self.states[&(id, domain)].clone();
                let package = &self.packages[&id];
                expand(package, &mut state)?;
                let dependencies = package.dependencies.clone();
                let has_script = package.script.is_some();
                self.states.insert((id, domain), state.clone());
                let mut edges = Vec::new();
                for dep in dependencies {
                    if dep.build && !has_script {
                        continue;
                    }
                    if let Some(condition) = &dep.condition
                        && !condition_matches(condition, &self.host, &self.cfg)?
                    {
                        continue;
                    }
                    if boolean(&dep.spec, "optional", false)?
                        && !state.optional.contains(&dep.alias)
                    {
                        continue;
                    }
                    let child = self.dependency(id, &dep)?;
                    let library = self.packages[&child]
                        .library
                        .as_ref()
                        .ok_or_else(|| format!("dependency {} has no library", dep.alias))?;
                    let child_domain =
                        if dep.build || domain == Domain::Host || library.kind == "proc-macro" {
                            Domain::Host
                        } else {
                            Domain::Target
                        };
                    let mut requested = strings(dep.spec.get("features"))?
                        .into_iter()
                        .collect::<BTreeSet<_>>();
                    requested.extend(
                        state
                            .forwarded
                            .get(&dep.alias)
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                    if boolean(&dep.spec, "default-features", true)?
                        && self.packages[&child].features.contains_key("default")
                    {
                        requested.insert("default".into());
                    }
                    self.states
                        .entry((child, child_domain))
                        .or_default()
                        .names
                        .extend(requested);
                    edges.push(ResolvedDependency {
                        alias: dep.alias.replace('-', "_"),
                        package: child,
                        domain: child_domain,
                        build: dep.build,
                    });
                }
                edges.sort();
                edges.dedup();
                for (i, edge) in edges.iter().enumerate() {
                    if edges[..i]
                        .iter()
                        .any(|other| other.alias == edge.alias && other.build == edge.build)
                    {
                        return Err(format!(
                            "conflicting active dependency alias {}",
                            edge.alias
                        ));
                    }
                }
                self.edges.insert((id, domain), edges);
            }
            if self.states == previous {
                break;
            }
        }
        Ok(())
    }

    fn unit(
        &self,
        key: Key,
        binary: Option<&Target>,
        units: &mut Vec<Unit>,
        indices: &mut BTreeMap<Key, usize>,
        active: &mut BTreeSet<Key>,
    ) -> Result<usize> {
        if let Some(&index) = indices.get(&key) {
            return Ok(index);
        }
        if !active.insert(key.clone()) {
            return Err("cyclic build units".into());
        }
        let package = &self.packages[&key.package];
        let state = &self.states[&(key.package, key.domain)];
        let script = key.kind == "build-script";
        let target = if script {
            Target {
                name: "build_script_build".into(),
                root: package.script.clone().ok_or("missing build script")?,
                edition: string(&package.fields, "edition")?.into(),
                kind: key.kind.clone(),
                required_features: Vec::new(),
            }
        } else if let Some(binary) = binary {
            binary.clone()
        } else {
            package.library.clone().ok_or("missing library")?
        };
        let mut dependencies = Vec::new();
        for edge in &self.edges[&(key.package, key.domain)] {
            if edge.build != script {
                continue;
            }
            let index = self.unit(
                Key {
                    package: edge.package,
                    domain: edge.domain,
                    kind: self.packages[&edge.package]
                        .library
                        .as_ref()
                        .ok_or("missing library")?
                        .kind
                        .clone(),
                },
                None,
                units,
                indices,
                active,
            )?;
            dependencies.push((edge.alias.clone(), index));
        }
        if key.kind == "bin"
            && let Some(library) = &package.library
        {
            let index = self.unit(
                Key {
                    kind: library.kind.clone(),
                    ..key.clone()
                },
                None,
                units,
                indices,
                active,
            )?;
            dependencies.push((library.name.clone(), index));
        }
        let build_script = if !script && package.script.is_some() {
            Some(self.unit(
                Key {
                    kind: "build-script".into(),
                    ..key.clone()
                },
                None,
                units,
                indices,
                active,
            )?)
        } else {
            None
        };
        let index = units.len();
        units.push(Unit {
            package: key.package,
            domain: if script { Domain::Host } else { key.domain },
            feature_domain: key.domain,
            target,
            features: state.names.iter().cloned().collect(),
            dependencies,
            build_script,
        });
        indices.insert(key.clone(), index);
        active.remove(&key);
        Ok(index)
    }
}

fn expand(package: &Package, state: &mut Features) -> Result<()> {
    let mut optional = BTreeSet::new();
    let mut required = BTreeSet::new();
    for dependency in &package.dependencies {
        if boolean(&dependency.spec, "optional", false)? {
            optional.insert(dependency.alias.clone());
        } else {
            required.insert(dependency.alias.clone());
        }
    }
    expand_features(&package.features, &optional, &required, state)
        .map_err(|e| format!("{e} in {}", package.manifest.display()))
}

fn expand_features(
    definitions: &BTreeMap<String, Vec<String>>,
    optional: &BTreeSet<String>,
    required: &BTreeSet<String>,
    state: &mut Features,
) -> Result<()> {
    loop {
        let before = state.clone();
        for feature in &before.names {
            let entries = definitions
                .get(feature)
                .ok_or_else(|| format!("unknown feature `{feature}`"))?;
            for entry in entries {
                if let Some(dep) = entry.strip_prefix("dep:") {
                    if !optional.contains(dep) {
                        return Err(format!(
                            "feature references unknown optional dependency `{dep}`"
                        ));
                    }
                    state.optional.insert(dep.into());
                } else if let Some((dep, feature)) = entry.split_once('/') {
                    let weak = dep.ends_with('?');
                    let dep = dep.trim_end_matches('?');
                    if !optional.contains(dep) && !required.contains(dep) {
                        return Err(format!("feature references unknown dependency `{dep}`"));
                    }
                    if !weak {
                        state.optional.insert(dep.into());
                        if definitions
                            .get(dep)
                            .is_some_and(|v| v == &[format!("dep:{dep}")])
                        {
                            state.names.insert(dep.into());
                        }
                    }
                    if !weak || state.optional.contains(dep) || required.contains(dep) {
                        state
                            .forwarded
                            .entry(dep.into())
                            .or_default()
                            .insert(feature.into());
                    }
                } else {
                    state.names.insert(entry.clone());
                }
            }
        }
        if *state == before {
            return Ok(());
        }
    }
}

fn load(path: &Path) -> Result<Package> {
    let manifest = canonical(path)?;
    let mut table = read_table(&manifest)?;
    if table.contains_key("patch") || table.contains_key("replace") {
        return Err("native planning does not support patch or replace tables yet".into());
    }
    let (root, ws) = workspace(&manifest)?;
    let fields = table
        .get_mut("package")
        .and_then(toml::Value::as_table_mut)
        .ok_or("expected [package]")?;
    for (key, value) in fields.iter_mut() {
        if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            *value = ws
                .get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get(key))
                .ok_or_else(|| format!("missing workspace package field {key}"))?
                .clone();
        }
    }
    fields
        .entry("edition".to_owned())
        .or_insert(toml::Value::String("2015".into()));
    super::edition(string(fields, "edition")?)?;
    let fields = fields.clone();
    let mut dependencies = Vec::new();
    dependency_tables(
        &table,
        &root,
        &ws,
        None,
        &mut dependencies,
        string(&fields, "edition")? == "2024",
    )?;
    if let Some(targets) = table.get("target").and_then(toml::Value::as_table) {
        for (condition, target) in targets {
            dependency_tables(
                target
                    .as_table()
                    .ok_or("expected target dependency table")?,
                &root,
                &ws,
                Some(condition),
                &mut dependencies,
                string(&fields, "edition")? == "2024",
            )?;
        }
    }
    let mut features = BTreeMap::new();
    if let Some(values) = table.get("features").and_then(toml::Value::as_table) {
        for (name, entries) in values {
            features.insert(name.clone(), strings(Some(entries))?);
        }
    }
    let explicit: BTreeSet<_> = features
        .values()
        .flatten()
        .filter_map(|s| s.strip_prefix("dep:").map(str::to_owned))
        .collect();
    for dep in &dependencies {
        if boolean(&dep.spec, "optional", false)? && !explicit.contains(&dep.alias) {
            features
                .entry(dep.alias.clone())
                .or_insert_with(|| vec![format!("dep:{}", dep.alias)]);
        }
    }
    let dir = manifest.parent().ok_or("manifest has no parent")?;
    let library = if table.contains_key("lib")
        || (boolean(&fields, "autolib", true)? && dir.join("src/lib.rs").is_file())
    {
        let lib = table
            .get("lib")
            .and_then(toml::Value::as_table)
            .cloned()
            .unwrap_or_default();
        Some(target(
            dir,
            &fields,
            &lib,
            if boolean(&lib, "proc-macro", false)? {
                "proc-macro"
            } else {
                "lib"
            },
            "src/lib.rs",
        )?)
    } else {
        None
    };
    let script = match fields.get("build") {
        Some(toml::Value::Boolean(false)) => None,
        Some(toml::Value::String(path)) => Some(canonical(&dir.join(path))?),
        None | Some(toml::Value::Boolean(true)) => {
            dir.join("build.rs").is_file().then(|| dir.join("build.rs"))
        }
        _ => return Err("expected a build script path or boolean".into()),
    };
    Ok(Package {
        manifest,
        table,
        fields,
        features,
        dependencies,
        library,
        script,
    })
}

fn dependency_tables(
    table: &Table,
    root: &Path,
    ws: &Table,
    condition: Option<&str>,
    output: &mut Vec<Dependency>,
    override_defaults: bool,
) -> Result<()> {
    for (key, build) in [("dependencies", false), ("build-dependencies", true)] {
        if let Some(deps) = table.get(key) {
            for (alias, value) in deps.as_table().ok_or("expected dependencies table")? {
                let mut spec = dependency_spec(value)?;
                for key in ["artifact", "lib", "target"] {
                    if spec.contains_key(key) {
                        return Err(format!(
                            "dependency `{alias}` uses unsupported artifact field `{key}`"
                        ));
                    }
                }
                if boolean(&spec, "workspace", false)? {
                    let inherited = ws
                        .get("workspace")
                        .and_then(|w| w.get("dependencies"))
                        .and_then(|d| d.get(alias))
                        .ok_or_else(|| format!("missing workspace dependency {alias}"))?;
                    let mut inherited = dependency_spec(inherited)?;
                    if let Some(path) = inherited.get("path").and_then(toml::Value::as_str) {
                        inherited.insert(
                            "path".into(),
                            toml::Value::String(root.join(path).to_string_lossy().into_owned()),
                        );
                    }
                    if !override_defaults
                        && spec.get("default-features").and_then(toml::Value::as_bool)
                            == Some(false)
                        && boolean(&inherited, "default-features", true)?
                    {
                        return Err(format!(
                            "cannot disable inherited default features for {alias}"
                        ));
                    }
                    let mut features = strings(inherited.get("features"))?;
                    features.extend(strings(spec.get("features"))?);
                    spec.remove("workspace");
                    inherited.extend(spec);
                    inherited.insert(
                        "features".into(),
                        toml::Value::Array(features.into_iter().map(toml::Value::String).collect()),
                    );
                    spec = inherited;
                }
                output.push(Dependency {
                    alias: alias.clone(),
                    spec,
                    build,
                    condition: condition.map(str::to_owned),
                });
            }
        }
    }
    Ok(())
}
fn dependency_spec(value: &toml::Value) -> Result<Table> {
    match value {
        toml::Value::String(v) => Ok(Table::from_iter([(
            "version".into(),
            toml::Value::String(v.clone()),
        )])),
        toml::Value::Table(t) => Ok(t.clone()),
        _ => Err("expected a dependency version or table".into()),
    }
}
fn workspace(manifest: &Path) -> Result<(PathBuf, Table)> {
    let table = read_table(manifest)?;
    let dir = manifest.parent().ok_or("manifest has no parent")?;
    if let Some(path) = table
        .get("package")
        .and_then(|p| p.get("workspace"))
        .and_then(toml::Value::as_str)
    {
        let root = canonical(&dir.join(path))?;
        return Ok((root.clone(), read_table(&root.join("Cargo.toml"))?));
    }
    for ancestor in dir.ancestors() {
        let path = ancestor.join("Cargo.toml");
        if path.is_file() {
            let table = read_table(&path)?;
            if table.contains_key("workspace") {
                return Ok((ancestor.into(), table));
            }
        }
    }
    Ok((dir.into(), table))
}
fn members(root: &Path, table: &Table, manifest: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if table.contains_key("package") {
        paths.push(root.join("Cargo.toml"));
    }
    for member in strings(table.get("workspace").and_then(|w| w.get("members")))? {
        if member.contains(['*', '?', '[']) {
            return Err("workspace member globs are not supported by native planning yet".into());
        }
        paths.push(canonical(&root.join(member).join("Cargo.toml"))?);
    }
    if !paths.iter().any(|p| p == manifest) && manifest != root.join("Cargo.toml") {
        paths.push(manifest.into());
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}
fn binary(package: &Package, name: Option<&str>) -> Result<Target> {
    let dir = package.manifest.parent().ok_or("manifest has no parent")?;
    let mut targets = BTreeMap::new();
    if boolean(&package.fields, "autobins", true)? {
        if dir.join("src/main.rs").is_file() {
            let target = target(dir, &package.fields, &Table::new(), "bin", "src/main.rs")?;
            targets.insert(target.name.clone(), target);
        }
        if dir.join("src/bin").is_dir() {
            for entry in std::fs::read_dir(dir.join("src/bin")).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                let (name, file) = if path.is_dir() {
                    (path.file_name(), path.join("main.rs"))
                } else if path.extension().is_some_and(|e| e == "rs") {
                    (path.file_stem(), path.clone())
                } else {
                    continue;
                };
                if file.is_file() {
                    let fields = Table::from_iter([
                        (
                            "name".into(),
                            toml::Value::String(
                                name.ok_or("invalid bin filename")?
                                    .to_string_lossy()
                                    .into_owned(),
                            ),
                        ),
                        (
                            "path".into(),
                            toml::Value::String(file.to_string_lossy().into_owned()),
                        ),
                    ]);
                    let target = target(dir, &package.fields, &fields, "bin", "src/main.rs")?;
                    targets.insert(target.name.clone(), target);
                }
            }
        }
    }
    if let Some(bins) = package.table.get("bin") {
        for value in bins.as_array().ok_or("expected [[bin]]")? {
            let fields = value.as_table().ok_or("expected binary table")?;
            let target = target(dir, &package.fields, fields, "bin", "src/main.rs")?;
            targets.insert(target.name.clone(), target);
        }
    }
    let wanted = name.or_else(|| {
        package
            .fields
            .get("default-run")
            .and_then(toml::Value::as_str)
    });
    let mut matches: Vec<_> = targets
        .into_values()
        .filter(|t| wanted.is_none_or(|n| n == t.name))
        .collect();
    if matches.len() == 1 {
        Ok(matches.remove(0))
    } else {
        Err("select one binary with --bin".into())
    }
}
fn target(
    dir: &Path,
    package: &Table,
    fields: &Table,
    kind: &str,
    default_path: &str,
) -> Result<Target> {
    let name = fields
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or(string(package, "name")?);
    Ok(Target {
        name: if kind == "bin" {
            name.into()
        } else {
            name.replace('-', "_")
        },
        root: canonical(
            &dir.join(
                fields
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(default_path),
            ),
        )?,
        edition: fields
            .get("edition")
            .and_then(toml::Value::as_str)
            .unwrap_or(string(package, "edition")?)
            .into(),
        kind: kind.into(),
        required_features: strings(fields.get("required-features"))?,
    })
}
fn validate_overrides(root: &Path, table: &Table) -> Result<()> {
    let resolver = table
        .get("workspace")
        .and_then(|w| w.get("resolver"))
        .or_else(|| table.get("package").and_then(|p| p.get("resolver")))
        .and_then(toml::Value::as_str);
    if resolver.is_some_and(|r| r != "2" && r != "3")
        || (resolver.is_none()
            && table
                .get("package")
                .and_then(|p| p.get("edition"))
                .and_then(toml::Value::as_str)
                .is_none_or(|e| e == "2015" || e == "2018"))
    {
        return Err("native planning requires feature resolver 2 or 3".into());
    }
    validate_configuration(root, table, true)
}

fn validate_configuration(root: &Path, table: &Table, reject_target: bool) -> Result<()> {
    if table.contains_key("patch") || table.contains_key("replace") {
        return Err("native source operations do not support patch or replace tables yet".into());
    }
    for config in configuration_paths(root) {
        if !config.is_file() {
            continue;
        }
        let table = read_table(&config)?;
        if ["source", "patch", "registries", "paths"]
            .iter()
            .any(|k| table.contains_key(*k))
            || (reject_target && table.get("build").and_then(|v| v.get("target")).is_some())
        {
            return Err(format!(
                "native planning cannot apply source or target overrides in {}",
                config.display()
            ));
        }
    }
    Ok(())
}
fn configuration_paths(root: &Path) -> Vec<PathBuf> {
    let mut configs = root
        .ancestors()
        .flat_map(|p| [p.join(".cargo/config"), p.join(".cargo/config.toml")])
        .collect::<Vec<_>>();
    if let Some(home) = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
    {
        configs.extend([home.join("config"), home.join("config.toml")]);
    }
    configs
}

fn condition_matches(condition: &str, host: &str, cfg: &BTreeSet<String>) -> Result<bool> {
    if let Some(inner) = condition
        .strip_prefix("cfg(")
        .and_then(|s| s.strip_suffix(')'))
    {
        predicate(inner, cfg)
    } else {
        Ok(condition == host)
    }
}
fn predicate(input: &str, cfg: &BTreeSet<String>) -> Result<bool> {
    let input = input.trim();
    for op in ["all", "any", "not"] {
        if let Some(inner) = input
            .strip_prefix(op)
            .map(str::trim_start)
            .and_then(|s| s.strip_prefix('('))
            .and_then(|s| s.strip_suffix(')'))
        {
            let mut depth = 0;
            let mut quoted = false;
            let mut start = 0;
            let mut args = Vec::new();
            for (i, c) in inner.char_indices() {
                match c {
                    '"' => quoted = !quoted,
                    '(' if !quoted => depth += 1,
                    ')' if !quoted => depth -= 1,
                    ',' if !quoted && depth == 0 => {
                        args.push(&inner[start..i]);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            if !inner[start..].trim().is_empty() {
                args.push(&inner[start..]);
            }
            let values: Vec<_> = args
                .into_iter()
                .map(|s| predicate(s, cfg))
                .collect::<Result<_>>()?;
            return match op {
                "all" => Ok(values.iter().all(|v| *v)),
                "any" => Ok(values.iter().any(|v| *v)),
                "not" if values.len() == 1 => Ok(!values[0]),
                _ => Err("expected one not() argument".into()),
            };
        }
    }
    let atom = compact(input);
    if atom.starts_with("feature=")
        || atom == "test"
        || atom == "debug_assertions"
        || atom.starts_with("target_feature=")
    {
        return Err(format!("unsupported target dependency predicate {input}"));
    }
    if input.contains(['(', ')', ',']) || input.is_empty() {
        return Err(format!("invalid target dependency predicate {input}"));
    }
    Ok(cfg.contains(&atom))
}
fn compact(input: &str) -> String {
    input.chars().filter(|c| !c.is_whitespace()).collect()
}
fn check_version(package: &Locked, req: Option<&semver::VersionReq>) -> Result<()> {
    let version = semver::Version::parse(&package.version).map_err(|e| e.to_string())?;
    if req.is_none_or(|r| r.matches(&version)) {
        Ok(())
    } else {
        Err(format!("locked version does not match {}", package.name))
    }
}
fn package_version(fields: &Table, path: &Path) -> Result<String> {
    if let Some(version) = fields.get("version").and_then(toml::Value::as_str) {
        return Ok(version.to_owned());
    }
    if fields
        .get("version")
        .and_then(|v| v.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true)
    {
        let (_, table) = workspace(path)?;
        return table
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .ok_or("missing workspace package version".into());
    }
    Err("missing package version".into())
}
fn find_manifests(root: &Path, name: &str, version: &str, found: &mut Vec<PathBuf>) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if manifest.is_file() {
        let table = read_table(&manifest)?;
        if table
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            == Some(name)
        {
            let fields = table
                .get("package")
                .and_then(toml::Value::as_table)
                .ok_or("missing package table")?;
            if package_version(fields, &manifest)? == version {
                found.push(manifest);
            }
        }
    }
    for directory in directories(root)? {
        if directory
            .file_name()
            .is_some_and(|n| n != ".git" && n != "target")
        {
            find_manifests(&directory, name, version, found)?;
        }
    }
    Ok(())
}
fn directories(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in
        std::fs::read_dir(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}
fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| format!("cannot open {}: {e}", path.display()))
}
fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}
fn read_table(path: &Path) -> Result<Table> {
    toml::from_str(&read(path)?).map_err(|e| format!("invalid {}: {e}", path.display()))
}
fn string<'a>(table: &'a Table, key: &str) -> Result<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("expected string `{key}`"))
}
fn boolean(table: &Table, key: &str, default: bool) -> Result<bool> {
    table.get(key).map_or(Ok(default), |v| {
        v.as_bool()
            .ok_or_else(|| format!("expected boolean `{key}`"))
    })
}
fn strings(value: Option<&toml::Value>) -> Result<Vec<String>> {
    value.map_or(Ok(Vec::new()), |v| {
        v.as_array()
            .ok_or("expected an array of strings".to_owned())?
            .iter()
            .map(|s| {
                s.as_str()
                    .map(str::to_owned)
                    .ok_or("expected a string".into())
            })
            .collect()
    })
}

fn git_source_matches(source: &str, url: &str, spec: &Table) -> bool {
    let mut identity = format!("git+{url}");
    for key in ["rev", "branch", "tag"] {
        if let Some(value) = spec.get(key).and_then(toml::Value::as_str) {
            identity.push('?');
            identity.push_str(key);
            identity.push('=');
            identity.push_str(&encode_git_ref(value));
        }
    }
    source
        .rsplit_once('#')
        .is_some_and(|(locked, _)| locked == identity)
}

pub(super) fn prepare(options: &Options, optimize: Option<bool>) -> Result<ExitCode> {
    execute::run(make_plan(options)?, options, optimize)
}

fn cached_source(home: &Path, package: &Locked) -> Result<Option<PathBuf>> {
    let source = package
        .source
        .as_deref()
        .ok_or("expected a cached dependency")?;
    let mut found = Vec::new();
    if source.starts_with("registry+") {
        for registry in directories(&home.join("registry/src"))? {
            // Only Cargo's standard crates.io caches. Source replacement needs explicit support.
            if !registry
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| {
                    s == "index.crates.io-1949cf8c6b5b557f" || s == "github.com-1ecc6299db9ec823"
                })
            {
                continue;
            }
            let root = registry.join(format!("{}-{}", package.name, package.version));
            if root.join("Cargo.toml").is_file() {
                let checksum_file = root.join(".cargo-checksum.json");
                if checksum_file.is_file() {
                    let checksum: serde_json::Value = serde_json::from_str(&read(&checksum_file)?)
                        .map_err(|e| format!("invalid cached checksum: {e}"))?;
                    if checksum["package"].as_str() != package.checksum.as_deref() {
                        return Err(format!("cached checksum differs for {}", package.name));
                    }
                }
                found.push(root.join("Cargo.toml"));
            }
        }
    } else if source.starts_with("git+") {
        let commit = source
            .rsplit_once('#')
            .ok_or("git lock entry has no commit")?
            .1;
        if commit.len() != 40 || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("expected a full locked git commit".into());
        }
        for repo in directories(&home.join("git/checkouts"))? {
            for checkout in directories(&repo)? {
                if checkout
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(".flux-fetch-"))
                {
                    continue;
                }
                let head = super::output(
                    Command::new("git")
                        .arg("-C")
                        .arg(&checkout)
                        .args(["rev-parse", "HEAD"]),
                    "cached git revision",
                )?;
                if head.trim() == commit {
                    find_manifests(&checkout, &package.name, &package.version, &mut found)?;
                }
            }
        }
    }
    found.sort();
    found.dedup();
    match found.as_slice() {
        [path] => canonical(path).map(Some),
        [] => Ok(None),
        _ => Err(format!(
            "multiple cached sources for {} {}; set CARGO_HOME to one cache",
            package.name, package.version
        )),
    }
}

pub(super) fn fetch(options: &FetchOptions) -> Result<ExitCode> {
    fetch::run(options)
}

pub(super) fn generate_lockfile(options: &ResolveOptions) -> Result<ExitCode> {
    resolve::run(options)
}

fn encode_git_ref(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(result, "%{byte:02X}");
        }
    }
    result
}

fn source_reference(source: &str) -> &str {
    if source.starts_with("git+") {
        source
            .rsplit_once('#')
            .map_or(source, |(reference, _)| reference)
    } else {
        source
    }
}
fn lock_source_matches(reference: &str, source: &str) -> bool {
    reference == source || reference == source_reference(source)
}
