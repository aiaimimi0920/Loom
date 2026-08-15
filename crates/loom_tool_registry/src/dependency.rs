use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use loom_protocol::{PackageDependency, ResolvedDependency};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

const RUNTIME_REGISTRY_FILE: &str = "plugin-runtimes.json";
const RECOVERED_RUNTIME_REGISTRY_FILE: &str = "plugin-runtimes-recovered.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCandidate {
    pub kind: String,
    pub id: String,
    pub version: String,
    pub sha256: String,
    #[serde(default)]
    pub path: PathBuf,
}

pub fn resolve_dependencies(
    dependencies: &[PackageDependency],
    candidates: &[PackageCandidate],
) -> Result<Vec<ResolvedDependency>, String> {
    let mut resolved = Vec::new();
    for dependency in dependencies {
        let requirement = VersionReq::parse(&dependency.version).map_err(|error| {
            format!(
                "dependency `{}` has invalid version requirement `{}`: {error}",
                dependency.id, dependency.version
            )
        })?;
        let mut matches = candidates
            .iter()
            .filter(|candidate| candidate.kind == dependency.kind && candidate.id == dependency.id)
            .filter_map(|candidate| {
                Version::parse(&candidate.version)
                    .ok()
                    .filter(|version| requirement.matches(version))
                    .map(|version| (version, candidate))
            })
            .collect::<Vec<_>>();
        if let Some(expected) = dependency.sha256.as_deref() {
            let had_version_match = !matches.is_empty();
            matches.retain(|(_, candidate)| expected.eq_ignore_ascii_case(&candidate.sha256));
            if had_version_match && matches.is_empty() {
                return Err(format!(
                    "dependency `{}` digest does not match its manifest pin",
                    dependency.id
                ));
            }
        }
        matches.sort_by(|(left, _), (right, _)| right.cmp(left));
        let Some((_, selected)) = matches.first() else {
            if dependency.optional {
                continue;
            }
            return Err(format!(
                "no compatible {} dependency `{}` satisfies `{}`",
                dependency.kind, dependency.id, dependency.version
            ));
        };
        resolved.push(ResolvedDependency {
            kind: selected.kind.clone(),
            id: selected.id.clone(),
            version: selected.version.clone(),
            sha256: selected.sha256.clone(),
        });
    }
    Ok(resolved)
}

#[derive(Clone, Debug)]
pub struct RuntimeRegistry {
    path: PathBuf,
}

impl RuntimeRegistry {
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            path: control_plane_root.as_ref().join(RUNTIME_REGISTRY_FILE),
        }
    }

    pub fn list(&self) -> Result<Vec<PackageCandidate>, String> {
        let path = self.storage_path();
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn register(&self, candidate: PackageCandidate) -> Result<(), String> {
        Version::parse(&candidate.version)
            .map_err(|error| format!("runtime version is invalid: {error}"))?;
        if candidate.kind.trim().is_empty()
            || candidate.id.trim().is_empty()
            || candidate.sha256.len() != 64
            || !candidate
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !candidate.path.is_dir()
        {
            return Err("runtime record is incomplete or invalid".to_owned());
        }
        let mut records = self.list()?;
        records.retain(|record| {
            !(record.kind == candidate.kind
                && record.id == candidate.id
                && record.version == candidate.version)
        });
        records.push(candidate);
        records.sort_by(|left, right| {
            (&left.kind, &left.id, &left.version).cmp(&(&right.kind, &right.id, &right.version))
        });
        self.write(&records)
    }

    pub fn resolve(
        &self,
        dependencies: &[PackageDependency],
    ) -> Result<Vec<ResolvedDependency>, String> {
        let candidates = self
            .list()?
            .into_iter()
            .filter(|candidate| candidate.path.is_dir())
            .collect::<Vec<_>>();
        resolve_dependencies(dependencies, &candidates)
    }

    pub fn prune_stale(&self) -> Result<usize, String> {
        let mut records = self.list()?;
        let before = records.len();
        records.retain(|candidate| candidate.path.is_dir());
        let removed = before.saturating_sub(records.len());
        if removed > 0 {
            self.write(&records)?;
        }
        Ok(removed)
    }

    fn write(&self, records: &[PackageCandidate]) -> Result<(), String> {
        let path = self.storage_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut bytes = serde_json::to_vec_pretty(records).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        crate::replace_registry_file(&temporary, &path).map_err(|error| error.to_string())
    }

    fn storage_path(&self) -> PathBuf {
        match fs::metadata(&self.path) {
            Ok(_) => self.path.clone(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => self.path.clone(),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => self
                .path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(RECOVERED_RUNTIME_REGISTRY_FILE),
            Err(_) => self.path.clone(),
        }
    }
}

pub fn dependency_index_by_id(
    records: &[PackageCandidate],
) -> BTreeMap<String, Vec<PackageCandidate>> {
    let mut index = BTreeMap::<String, Vec<PackageCandidate>>::new();
    for record in records {
        index
            .entry(record.id.clone())
            .or_default()
            .push(record.clone());
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(version: &str, digest: char) -> PackageCandidate {
        PackageCandidate {
            kind: "runtime".to_owned(),
            id: "loom.runtime.python".to_owned(),
            version: version.to_owned(),
            sha256: digest.to_string().repeat(64),
            path: PathBuf::from("runtime"),
        }
    }

    #[test]
    fn resolver_selects_highest_compatible_semver_and_honors_hash_pin() {
        let dependencies = vec![PackageDependency {
            kind: "runtime".to_owned(),
            id: "loom.runtime.python".to_owned(),
            version: "^3.12".to_owned(),
            sha256: Some("b".repeat(64)),
            optional: false,
        }];
        let resolved = resolve_dependencies(
            &dependencies,
            &[
                candidate("3.12.1", 'a'),
                candidate("3.12.4", 'b'),
                candidate("3.13.0", 'c'),
            ],
        )
        .expect("resolve compatible runtime");
        assert_eq!(resolved[0].version, "3.12.4");
    }

    #[test]
    fn resolver_rejects_missing_required_dependencies() {
        let dependencies = vec![PackageDependency {
            kind: "runtime".to_owned(),
            id: "missing".to_owned(),
            version: ">=1.0".to_owned(),
            sha256: None,
            optional: false,
        }];
        assert!(resolve_dependencies(&dependencies, &[]).is_err());
    }
}
