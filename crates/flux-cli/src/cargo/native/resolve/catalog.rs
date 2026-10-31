//! Source metadata used by the resolver; registry crates need no archive download.
use super::*;
use serde::Deserialize;

mod git;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Id {
    pub name: String,
    pub version: Version,
    pub source: Option<String>,
}
#[derive(Clone)]
pub(super) struct Dependency {
    pub name: String,
    pub alias: String,
    pub requirement: VersionReq,
    pub features: Vec<String>,
    pub default_features: bool,
    pub optional: bool,
    pub dev: bool,
    pub path: Option<PathBuf>,
    git: Option<super::super::Table>,
    inherited_git: Option<(String, PathBuf)>,
}
pub(super) struct Package {
    pub dependencies: Vec<Dependency>,
    pub features: BTreeMap<String, Vec<String>>,
    pub optional: BTreeSet<String>,
    pub required: BTreeSet<String>,
    pub links: Option<String>,
    pub checksum: Option<String>,
    pub rust_version: Option<Version>,
    manifest: Option<PathBuf>,
}
#[derive(Clone, Deserialize)]
struct Entry {
    name: String,
    vers: String,
    #[serde(default)]
    deps: Vec<IndexDependency>,
    cksum: String,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    features2: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    yanked: bool,
    rust_version: Option<String>,
    links: Option<String>,
}
#[derive(Clone, Deserialize)]
struct IndexDependency {
    name: String,
    package: Option<String>,
    req: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    optional: bool,
    default_features: bool,
    kind: Option<String>,
    registry: Option<String>,
    artifact: Option<serde_json::Value>,
}
pub(super) struct Catalog {
    pub packages: BTreeMap<Id, Package>,
    home: PathBuf,
    index_url: String,
    offline: bool,
    entries: BTreeMap<String, Vec<Entry>>,
    failures: BTreeMap<String, String>,
    git_sources: BTreeMap<String, String>,
}
impl Catalog {
    pub fn new(home: PathBuf, index_url: String, offline: bool) -> Self {
        Self {
            packages: BTreeMap::new(),
            home,
            index_url: index_url.trim_end_matches('/').into(),
            offline,
            entries: BTreeMap::new(),
            failures: BTreeMap::new(),
            git_sources: BTreeMap::new(),
        }
    }
    pub fn local(&mut self, path: &Path, source: Option<String>) -> Result<Id> {
        let path = super::super::canonical(path)?;
        if let Some((id, _)) = self
            .packages
            .iter()
            .find(|(id, p)| id.source == source && p.manifest.as_ref() == Some(&path))
        {
            return Ok(id.clone());
        }
        let package = super::super::load(&path)?;
        let version = Version::parse(super::super::string(&package.fields, "version")?)
            .map_err(|e| e.to_string())?;
        let id = Id {
            name: super::super::string(&package.fields, "name")?.into(),
            version,
            source: source.clone(),
        };
        if self.packages.contains_key(&id) {
            return Err(format!(
                "conflicting local package identity {} {}",
                id.name, id.version
            ));
        }
        let git_root = if source.is_some() {
            Some(
                path.ancestors()
                    .skip(1)
                    .find(|p| p.join(".git").exists())
                    .ok_or("git manifest is outside its checkout")?
                    .to_path_buf(),
            )
        } else {
            None
        };
        let mut declarations: Vec<_> = package
            .dependencies
            .into_iter()
            .map(|d| (d, false))
            .collect();
        // Development dependencies participate only when this package is a workspace root.
        let (workspace, ws) = super::super::workspace(&path)?;
        let mut tables = vec![&package.table];
        if let Some(targets) = package.table.get("target").and_then(toml::Value::as_table) {
            for value in targets.values() {
                tables.push(value.as_table().ok_or("expected target dependency table")?);
            }
        }
        for table in tables {
            if let Some(dev) = table.get("dev-dependencies") {
                let table = super::super::Table::from_iter([("dependencies".into(), dev.clone())]);
                let mut values = Vec::new();
                super::super::dependency_tables(
                    &table,
                    &workspace,
                    &ws,
                    None,
                    &mut values,
                    super::super::string(&package.fields, "edition")? == "2024",
                )?;
                declarations.extend(values.into_iter().map(|d| (d, true)));
            }
        }
        let mut dependencies = Vec::new();
        for (declaration, dev) in declarations {
            let spec = declaration.spec;
            if spec.contains_key("registry-index")
                || spec
                    .get("registry")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|r| r != "crates-io")
            {
                return Err("alternate registries are not supported yet".into());
            }
            let optional = super::super::boolean(&spec, "optional", false)?;
            if dev && optional {
                return Err("development dependencies cannot be optional".into());
            }
            let git = spec.get("git").map(|_| spec.clone());
            let local = spec.get("path").and_then(toml::Value::as_str).map(|p| {
                path.parent()
                    .expect("canonical manifest has a parent")
                    .join(p)
                    .join("Cargo.toml")
            });
            if local.is_some() && git.is_some() {
                return Err("dependency cannot specify both path and git".into());
            }
            dependencies.push(Dependency {
                name: spec
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(&declaration.alias)
                    .into(),
                alias: declaration.alias,
                requirement: VersionReq::parse(
                    spec.get("version")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("*"),
                )
                .map_err(|e| e.to_string())?,
                features: super::super::strings(spec.get("features"))?,
                default_features: super::super::boolean(&spec, "default-features", true)?,
                optional,
                dev,
                path: local,
                git,
                inherited_git: source.clone().zip(git_root.clone()),
            });
        }
        let package = Package::new(
            dependencies,
            package.features,
            package
                .fields
                .get("links")
                .and_then(toml::Value::as_str)
                .map(str::to_owned),
            None,
            package
                .fields
                .get("rust-version")
                .and_then(toml::Value::as_str)
                .map(rust_version)
                .transpose()?,
            Some(path),
        );
        self.packages.insert(id.clone(), package);
        Ok(id)
    }
    pub fn candidates(&mut self, dependency: &Dependency) -> Result<Vec<Id>> {
        if let Some(path) = &dependency.path {
            let path = super::super::canonical(path)?;
            let source = if let Some((source, root)) = &dependency.inherited_git {
                if !path.starts_with(root) {
                    return Err("git path dependency leaves its checkout".into());
                }
                Some(source.clone())
            } else {
                None
            };
            let id = self.local(&path, source)?;
            return Ok(
                if id.name == dependency.name && dependency.requirement.matches(&id.version) {
                    vec![id]
                } else {
                    Vec::new()
                },
            );
        }
        if let Some(spec) = &dependency.git {
            return self.git_candidates(&dependency.name, &dependency.requirement, spec);
        }
        self.load_index(&dependency.name)?;
        Ok(self.entries[&dependency.name]
            .iter()
            .filter_map(|entry| {
                let version = Version::parse(&entry.vers).ok()?;
                let id = Id {
                    name: entry.name.clone(),
                    version,
                    source: Some(super::super::fetch::REGISTRY.into()),
                };
                if !dependency.requirement.matches(&id.version)
                    || entry.yanked
                    || (self.offline && !self.available(&id))
                {
                    None
                } else {
                    Some(id)
                }
            })
            .collect())
    }
    pub fn ensure(&mut self, id: &Id) -> Result<()> {
        if self.packages.contains_key(id) {
            return Ok(());
        }
        let entry = self
            .entries
            .get(&id.name)
            .and_then(|v| v.iter().find(|e| e.vers == id.version.to_string()))
            .ok_or("missing registry metadata")?
            .clone();
        let mut dependencies = Vec::new();
        for dep in entry.deps {
            if dep.kind.as_deref() == Some("dev") {
                continue;
            }
            if dep
                .kind
                .as_deref()
                .is_some_and(|k| !matches!(k, "normal" | "build"))
            {
                return Err("unsupported registry dependency kind".into());
            }
            if dep
                .registry
                .as_deref()
                .is_some_and(|r| r != "https://github.com/rust-lang/crates.io-index")
            {
                return Err("alternate registry dependencies are not supported yet".into());
            }
            if dep.artifact.is_some() {
                return Err("artifact dependencies are not supported yet".into());
            }
            dependencies.push(Dependency {
                name: dep.package.unwrap_or_else(|| dep.name.clone()),
                alias: dep.name,
                requirement: VersionReq::parse(&dep.req).map_err(|e| e.to_string())?,
                features: dep.features,
                default_features: dep.default_features,
                optional: dep.optional,
                dev: false,
                path: None,
                git: None,
                inherited_git: None,
            });
        }
        let mut features = entry.features;
        for (name, values) in entry.features2 {
            features.entry(name).or_default().extend(values);
        }
        let package = Package::new(
            dependencies,
            features,
            entry.links,
            Some(entry.cksum),
            entry
                .rust_version
                .as_deref()
                .map(rust_version)
                .transpose()?,
            None,
        );
        self.packages.insert(id.clone(), package);
        Ok(())
    }
    pub fn rust_compatible(&self, id: &Id, version: &Version) -> bool {
        if let Some(package) = self.packages.get(id) {
            return package.rust_version.as_ref().is_none_or(|v| v <= version);
        }
        self.entries
            .get(&id.name)
            .and_then(|entries| entries.iter().find(|e| e.vers == id.version.to_string()))
            .and_then(|e| e.rust_version.as_deref())
            .is_none_or(|v| rust_version(v).is_ok_and(|v| v <= *version))
    }
    fn available(&self, id: &Id) -> bool {
        [super::super::fetch::CACHE, "github.com-1ecc6299db9ec823"]
            .iter()
            .any(|cache| {
                self.home
                    .join("registry/cache")
                    .join(cache)
                    .join(format!("{}-{}.crate", id.name, id.version))
                    .is_file()
                    || self
                        .home
                        .join("registry/src")
                        .join(cache)
                        .join(format!("{}-{}/Cargo.toml", id.name, id.version))
                        .is_file()
            })
    }
    fn load_index(&mut self, name: &str) -> Result<()> {
        if self.entries.contains_key(name) {
            return Ok(());
        }
        if let Some(error) = self.failures.get(name) {
            return Err(error.clone());
        }
        let result = self.read_index(name);
        match result {
            Ok(entries) => {
                self.entries.insert(name.into(), entries);
                Ok(())
            }
            Err(error) => {
                self.failures.insert(name.into(), error.clone());
                Err(error)
            }
        }
    }
    fn read_index(&self, name: &str) -> Result<Vec<Entry>> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("invalid registry package name".into());
        }
        let lowercase: String = name.chars().map(|c| c.to_ascii_lowercase()).collect();
        let suffix = match lowercase.len() {
            1 => format!("1/{lowercase}"),
            2 => format!("2/{lowercase}"),
            3 => format!("3/{}/{lowercase}", &lowercase[..1]),
            _ => format!("{}/{}/{lowercase}", &lowercase[..2], &lowercase[2..4]),
        };
        let parent = self.home.join("registry/index").join(format!(
            "flux-{}",
            super::super::fetch::hex(self.index_url.as_bytes())
        ));
        let path = parent.join(&suffix);
        let bytes = if self.offline {
            if path.is_file() {
                std::fs::read(&path).map_err(|e| e.to_string())?
            } else if self.index_url == "https://index.crates.io" {
                let mut data = None;
                for cache in [super::super::fetch::CACHE, "github.com-1ecc6299db9ec823"] {
                    let path = self
                        .home
                        .join("registry/index")
                        .join(cache)
                        .join(".cache")
                        .join(&suffix);
                    if path.is_file() {
                        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                        if !bytes.starts_with(&[3, 2, 0, 0, 0]) {
                            return Err("unsupported Cargo sparse index cache format".into());
                        }
                        data = Some(bytes);
                        break;
                    }
                }
                data.ok_or_else(|| {
                    format!("offline registry index missing for {name}; resolve online once")
                })?
            } else {
                return Err(format!("offline registry index missing for {name}"));
            }
        } else {
            let temporary = super::super::fetch::Temporary::new(
                path.parent().ok_or("index path has no parent")?,
            )?;
            let file = temporary.0.join("index");
            super::super::fetch::download(
                &format!("{}/{}", self.index_url.trim_end_matches('/'), suffix),
                &file,
            )?;
            let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
            parse_entries(&bytes, name)?;
            std::fs::rename(file, &path)
                .map_err(|e| format!("cannot cache registry index: {e}"))?;
            bytes
        };
        parse_entries(&bytes, name)
    }
}
impl Package {
    fn new(
        dependencies: Vec<Dependency>,
        mut features: BTreeMap<String, Vec<String>>,
        links: Option<String>,
        checksum: Option<String>,
        rust_version: Option<Version>,
        manifest: Option<PathBuf>,
    ) -> Self {
        let optional: BTreeSet<_> = dependencies
            .iter()
            .filter(|d| d.optional)
            .map(|d| d.alias.clone())
            .collect();
        let required = dependencies
            .iter()
            .filter(|d| !d.optional)
            .map(|d| d.alias.clone())
            .collect();
        let explicit: BTreeSet<_> = features
            .values()
            .flatten()
            .filter_map(|s| s.strip_prefix("dep:").map(str::to_owned))
            .collect();
        for alias in &optional {
            if !explicit.contains(alias) {
                features
                    .entry(alias.clone())
                    .or_insert_with(|| vec![format!("dep:{alias}")]);
            }
        }
        Self {
            dependencies,
            features,
            optional,
            required,
            links,
            checksum,
            rust_version,
            manifest,
        }
    }
}
fn parse_entries(bytes: &[u8], name: &str) -> Result<Vec<Entry>> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("registry index exceeds 32 MiB".into());
    }
    let records: Vec<_> = if bytes.starts_with(&[3, 2, 0, 0, 0]) {
        bytes
            .split(|b| *b == 0)
            .filter(|b| b.starts_with(b"{"))
            .collect()
    } else {
        bytes
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
            .collect()
    };
    let mut versions = BTreeSet::new();
    let mut entries = Vec::new();
    for record in records {
        let value: serde_json::Value = serde_json::from_slice(record)
            .map_err(|e| format!("invalid registry index for {name}: {e}"))?;
        if value
            .get("v")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|v| v > 2)
        {
            continue;
        }
        let entry: Entry = serde_json::from_value(value)
            .map_err(|e| format!("invalid registry record for {name}: {e}"))?;
        if entry.name != name {
            return Err("registry index package name mismatch".into());
        }
        Version::parse(&entry.vers).map_err(|e| e.to_string())?;
        if !versions.insert(entry.vers.clone()) {
            return Err("duplicate registry index version".into());
        }
        if entry.cksum.len() != 64
            || !entry
                .cksum
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid registry index checksum".into());
        }
        if let Some(version) = &entry.rust_version {
            rust_version(version)?;
        }
        entries.push(entry);
    }
    Ok(entries)
}
fn rust_version(text: &str) -> Result<Version> {
    let normalized = if text.bytes().filter(|c| *c == b'.').count() == 1 {
        format!("{text}.0")
    } else {
        text.to_owned()
    };
    Version::parse(&normalized).map_err(|e| format!("invalid rust-version: {e}"))
}
