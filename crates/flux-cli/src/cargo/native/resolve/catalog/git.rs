use super::*;

impl Catalog {
    pub(super) fn git_candidates(
        &mut self,
        name: &str,
        requirement: &VersionReq,
        spec: &super::super::super::Table,
    ) -> Result<Vec<Id>> {
        let url = super::super::super::string(spec, "git")?;
        if !["https://", "ssh://", "file://"]
            .iter()
            .any(|scheme| url.starts_with(scheme))
        {
            return Err("unsupported git dependency URL".into());
        }
        let references: Vec<_> = ["rev", "branch", "tag"]
            .iter()
            .filter_map(|key| {
                spec.get(*key)
                    .and_then(toml::Value::as_str)
                    .map(|value| (*key, value))
            })
            .collect();
        if references.len() > 1 {
            return Err("git dependency must select only one revision, branch or tag".into());
        }
        let mut identity = format!("git+{url}");
        if let Some((key, value)) = references.first() {
            identity.push_str(&format!(
                "?{key}={}",
                super::super::super::encode_git_ref(value)
            ));
        }
        let source = if let Some(source) = self.git_sources.get(&identity) {
            source.clone()
        } else {
            let cached_ref = self
                .home
                .join("git/flux-refs")
                .join(super::super::super::fetch::hex(identity.as_bytes()));
            let full = references
                .first()
                .filter(|(key, value)| {
                    *key == "rev"
                        && value.len() == 40
                        && value.bytes().all(|b| b.is_ascii_hexdigit())
                })
                .map(|(_, v)| v.to_string());
            let commit = if let Some(commit) = full {
                commit
            } else if self.offline {
                super::super::super::read(&cached_ref)
                    .map_err(|_| format!("offline git reference missing for {url}"))?
                    .trim()
                    .to_owned()
            } else {
                let reference = match references.first() {
                    Some(("branch", value)) => format!("refs/heads/{value}"),
                    Some(("tag", value)) => format!("refs/tags/{value}"),
                    Some((_, value)) => value.to_string(),
                    None => "HEAD".into(),
                };
                let output = crate::cargo::output(
                    Command::new("git")
                        .args([
                            "ls-remote",
                            "--exit-code",
                            "--",
                            url,
                            &reference,
                            &format!("{reference}^{{}}"),
                        ])
                        .env("GIT_TERMINAL_PROMPT", "0"),
                    "git reference resolution",
                )?;
                let mut commits = BTreeSet::new();
                let mut peeled = None;
                for line in output.lines() {
                    let mut fields = line.split_whitespace();
                    let commit = fields.next().ok_or("invalid git reference")?;
                    let name = fields.next().ok_or("invalid git reference name")?;
                    if name.ends_with("^{}") {
                        peeled = Some(commit.to_owned());
                    } else {
                        commits.insert(commit.to_owned());
                    }
                }
                if let Some(commit) = peeled {
                    commit
                } else if commits.len() == 1 {
                    commits.into_iter().next().ok_or("missing git commit")?
                } else {
                    return Err("git revision is missing or ambiguous; use a full commit".into());
                }
            };
            if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("expected a full git commit".into());
            }
            if !self.offline {
                let temporary = super::super::super::fetch::Temporary::new(
                    cached_ref
                        .parent()
                        .ok_or("git reference has no cache directory")?,
                )?;
                let file = temporary.0.join("reference");
                std::fs::write(&file, &commit).map_err(|e| e.to_string())?;
                std::fs::rename(file, &cached_ref).map_err(|e| e.to_string())?;
            }
            let source = format!("{identity}#{commit}");
            self.git_sources.insert(identity, source.clone());
            source
        };
        let commit = source.rsplit_once('#').ok_or("missing git commit")?.1;
        let mut checkout = find_checkout(&self.home, commit)?;
        if checkout.is_none() {
            super::super::super::fetch::git(&self.home, &source, self.offline)?;
            checkout = find_checkout(&self.home, commit)?;
        }
        let checkout = checkout.ok_or("git checkout is missing after fetch")?;
        let mut manifests = Vec::new();
        find_packages(&checkout, name, &mut manifests)?;
        let mut result = Vec::new();
        for manifest in manifests {
            let id = self.local(&manifest, Some(source.clone()))?;
            if requirement.matches(&id.version) {
                result.push(id);
            }
        }
        Ok(result)
    }
}
fn find_checkout(home: &Path, commit: &str) -> Result<Option<PathBuf>> {
    for repository in super::super::super::directories(&home.join("git/checkouts"))? {
        for checkout in super::super::super::directories(&repository)? {
            if checkout
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".flux-fetch-"))
            {
                continue;
            }
            let head = crate::cargo::output(
                Command::new("git")
                    .arg("-C")
                    .arg(&checkout)
                    .args(["rev-parse", "HEAD"]),
                "cached git revision",
            )?;
            if head.trim() == commit {
                return Ok(Some(checkout));
            }
        }
    }
    Ok(None)
}
fn find_packages(root: &Path, name: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if manifest.is_file() {
        let table = super::super::super::read_table(&manifest)?;
        if table
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            == Some(name)
        {
            files.push(manifest);
        }
    }
    for directory in super::super::super::directories(root)? {
        if directory
            .file_name()
            .is_some_and(|n| n != ".git" && n != "target")
        {
            find_packages(&directory, name, files)?;
        }
    }
    Ok(())
}
