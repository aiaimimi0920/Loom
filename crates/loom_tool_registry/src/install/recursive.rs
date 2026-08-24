use super::*;

/// Install an art package and, recursively, its dependent arts. `fetch_dependent`
/// returns the zip bytes for a dependent art id (wired to the store over HTTP).
/// Dependencies are installed before their parent so the parent's lockfile can
/// pin each child to its exact qualified id, version, and digest. Reports use
/// root-first traversal order as part of the current install API.
pub fn install_art_recursive<F>(
    root_zip: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
    fetch_dependent: &F,
) -> Result<Vec<ArtInstallReport>, ArtInstallError>
where
    F: Fn(&str) -> Result<Vec<u8>, ArtInstallError>,
{
    fn install_one<F>(
        zip: &[u8],
        requested_reference: Option<&str>,
        control_plane_root: &Path,
        framework_registry: &FrameworkRegistry,
        tool_registry: &ToolRegistry,
        fetch_dependent: &F,
        visiting: &mut std::collections::BTreeSet<String>,
        newly_installed: &mut Vec<String>,
    ) -> Result<Vec<ArtInstallReport>, ArtInstallError>
    where
        F: Fn(&str) -> Result<Vec<u8>, ArtInstallError>,
    {
        let manifest = read_manifest_from_zip(zip)?;
        if !is_safe_art_id(&manifest.id) {
            return Err(ArtInstallError::InvalidArtId(manifest.id));
        }
        let identity = manifest.qualified_id();
        let was_installed = tool_registry
            .list_tools()
            .map_err(|error| ArtInstallError::Registry(error.to_string()))?
            .iter()
            .any(|tool| tool.qualified_id() == identity);
        if let Some(reference) = requested_reference {
            if !art_reference_matches_qualified(reference, &identity) {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "store dependency `{reference}` resolved to unexpected Art `{identity}`"
                )));
            }
        }
        if !visiting.insert(identity.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency cycle detected at `{identity}`"
            )));
        }

        let result = (|| {
            let dependencies = read_dependencies(&manifest).arts;
            let mut descendants = Vec::new();
            for reference in dependencies {
                if !is_safe_art_reference(&reference) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "Art dependency reference `{reference}` is invalid"
                    )));
                }
                if visiting
                    .iter()
                    .any(|candidate| art_reference_matches_qualified(&reference, candidate))
                {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "Art dependency cycle detected through `{reference}`"
                    )));
                }
                if let Some(installed) = tool_registry
                    .get_tool(&reference)
                    .map_err(|error| ArtInstallError::Registry(error.to_string()))?
                {
                    verify_art_package_integrity(
                        control_plane_root,
                        &installed,
                        framework_registry,
                    )?;
                    continue;
                }
                let child_zip = fetch_dependent(&reference)?;
                descendants.extend(install_one(
                    &child_zip,
                    Some(&reference),
                    control_plane_root,
                    framework_registry,
                    tool_registry,
                    fetch_dependent,
                    visiting,
                    newly_installed,
                )?);
            }

            let report =
                install_art_from_zip(zip, control_plane_root, framework_registry, tool_registry)?;
            if !was_installed {
                newly_installed.push(identity.clone());
            }
            let mut reports = Vec::with_capacity(descendants.len() + 1);
            reports.push(report);
            reports.extend(descendants);
            Ok(reports)
        })();
        visiting.remove(&identity);
        result
    }

    let mut newly_installed = Vec::new();
    let result = install_one(
        root_zip,
        None,
        control_plane_root,
        framework_registry,
        tool_registry,
        fetch_dependent,
        &mut std::collections::BTreeSet::new(),
        &mut newly_installed,
    );
    if result.is_err() {
        for identity in newly_installed.into_iter().rev() {
            let _ = uninstall_art_package(control_plane_root, &identity, tool_registry);
        }
    }
    result
}
