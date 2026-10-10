//! Per-invocation admission shared by every native framework execution facade.

use super::*;

/// Kept inside one invocation, never serialized or cached with a persistent host.
/// A caller-side readiness result cannot authorize a later process invocation.
pub(crate) struct FrameworkExecutionAdmission {
    packages_root: PathBuf,
    package_dir: PathBuf,
    manifest_text: String,
    manifest: FrameworkPackageManifest,
    digest: String,
}

impl FrameworkExecutionAdmission {
    pub(crate) fn capture(
        packages_root: &Path,
        package_dir: &Path,
        manifest_text: &str,
    ) -> Result<Self, String> {
        let manifest: FrameworkPackageManifest =
            serde_json::from_str(manifest_text).map_err(|error| error.to_string())?;
        let mut admission = Self {
            packages_root: fs::canonicalize(packages_root).map_err(|error| error.to_string())?,
            package_dir: fs::canonicalize(package_dir).map_err(|error| error.to_string())?,
            manifest_text: manifest_text.to_owned(),
            manifest,
            digest: String::new(),
        };
        admission.digest = admission.verified_digest()?;
        Ok(admission)
    }

    pub(crate) fn revalidate(&self) -> Result<(), String> {
        if self.verified_digest()? != self.digest {
            return Err("framework package changed during execution preparation".to_owned());
        }
        Ok(())
    }

    fn verified_digest(&self) -> Result<String, String> {
        let control_root = self
            .packages_root
            .parent()
            .ok_or("missing control-plane root")?;
        // This is a read-only registry view: do not run startup recovery/pruning on
        // the execution path, including when an operator overrides the package root.
        let registry = FrameworkRegistry {
            root: control_root.to_path_buf(),
            path: control_root.join(FRAMEWORKS_FILE),
        };
        let identity = self.manifest.qualified_id();
        let states = registry
            .installation_states()
            .map_err(|error| error.to_string())?;
        let state = states.get(&identity).ok_or("framework is not installed")?;
        if !state.enabled {
            return Err("framework is disabled".to_owned());
        }
        if state.version != self.manifest.version {
            return Err("framework installation version changed".to_owned());
        }
        let installed_digest = state.package_digest.as_deref().filter(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        }).ok_or("framework installation has no valid pinned digest; reinstall the framework package")?;
        let active = resolve_framework_package_dir(&self.packages_root, &identity)
            .and_then(|path| fs::canonicalize(path).map_err(FrameworkError::Io))
            .map_err(|error| error.to_string())?;
        if active != self.package_dir {
            return Err("active framework package changed".to_owned());
        }
        let current_manifest =
            read_bounded_framework_metadata(&active.join(FRAMEWORK_MANIFEST_FILE))
                .map_err(|error| error.to_string())?;
        if current_manifest != self.manifest_text.as_bytes() {
            return Err("framework manifest changed".to_owned());
        }
        // Re-read current trust/revocation, permissions and dependency lockfiles,
        // even for an already-running MCP host that is about to receive new input.
        let digest = readiness::verify_framework_package_authority(
            &self.packages_root,
            &active,
            &self.manifest,
        )?;
        if !digest.eq_ignore_ascii_case(installed_digest) {
            return Err(
                "framework package does not match its pinned installation digest".to_owned(),
            );
        }
        Ok(digest)
    }
}
