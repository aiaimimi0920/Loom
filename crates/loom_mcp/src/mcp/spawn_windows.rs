//! Windows command discovery and PowerShell launch adaptation.

use super::*;

pub(super) fn spawn_command_spec(config: &McpServerConfig) -> SpawnCommandSpec {
    #[cfg(windows)]
    if let Some(spec) = resolve_windows_spawn_command(config) {
        return spec;
    }

    SpawnCommandSpec::direct(config.command.clone(), config.args.clone())
}

#[cfg(windows)]
pub(super) fn resolve_windows_spawn_command(config: &McpServerConfig) -> Option<SpawnCommandSpec> {
    let command = Path::new(&config.command);

    if is_windows_powershell_script(command) {
        return Some(windows_powershell_spawn_spec(command, &config.args));
    }

    if command.extension().is_some() {
        return None;
    }

    // A packaged server runs the file the installer extracted and `verify_installed_entry` hashed.
    // Resolving an extensionless command here would search `PATHEXT`, and for a bare name `PATH`
    // too, which can only ever land on a file other than the verified one. Packaged servers get no
    // such search; the verifier rejects the extensionless command before it reaches this point.
    if config.package.is_some() {
        return None;
    }

    let resolved = resolve_windows_command_path(command)?;
    if is_windows_powershell_script(&resolved) {
        return Some(windows_powershell_spawn_spec(&resolved, &config.args));
    }

    Some(SpawnCommandSpec::direct(
        resolved.display().to_string(),
        config.args.clone(),
    ))
}

#[cfg(windows)]
pub(super) fn resolve_windows_command_path(command: &Path) -> Option<PathBuf> {
    let extensions = windows_path_extensions();

    if is_windows_path_qualified(command) {
        return resolve_windows_command_in_paths(command, &[], &extensions);
    }

    let search_paths = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    resolve_windows_command_in_paths(command, &search_paths, &extensions)
}

#[cfg(windows)]
pub(super) fn resolve_windows_command_in_paths(
    command: &Path,
    search_paths: &[PathBuf],
    extensions: &[String],
) -> Option<PathBuf> {
    if command.extension().is_some() {
        return None;
    }

    if is_windows_path_qualified(command) {
        return resolve_windows_command_candidates(command, extensions);
    }

    search_paths.iter().find_map(|search_path| {
        resolve_windows_command_candidates(&search_path.join(command), extensions)
    })
}

#[cfg(windows)]
pub(super) fn resolve_windows_command_candidates(
    command: &Path,
    extensions: &[String],
) -> Option<PathBuf> {
    extensions
        .iter()
        .map(|extension| append_windows_extension(command, extension))
        .find(|candidate| candidate.is_file())
}

#[cfg(windows)]
pub(super) fn windows_path_extensions() -> Vec<String> {
    let value = std::env::var_os("PATHEXT");
    windows_path_extensions_from(value.as_deref())
}

#[cfg(windows)]
pub(super) fn windows_path_extensions_from(value: Option<&std::ffi::OsStr>) -> Vec<String> {
    let extensions = value
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_windows_extension)
        .collect::<Vec<_>>();

    if extensions.is_empty() {
        [".com", ".exe", ".bat", ".cmd"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        extensions
    }
}

#[cfg(windows)]
pub(super) fn normalize_windows_extension(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('.') {
        trimmed.to_ascii_lowercase()
    } else {
        format!(".{}", trimmed.to_ascii_lowercase())
    }
}

#[cfg(windows)]
pub(super) fn append_windows_extension(command: &Path, extension: &str) -> PathBuf {
    let mut candidate = command.as_os_str().to_os_string();
    candidate.push(extension);
    PathBuf::from(candidate)
}

#[cfg(windows)]
pub(super) fn is_windows_path_qualified(command: &Path) -> bool {
    command.is_absolute()
        || command
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
}

#[cfg(windows)]
pub(super) fn is_windows_powershell_script(command: &Path) -> bool {
    command
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("ps1"))
}

#[cfg(windows)]
pub(super) fn windows_powershell_spawn_spec(command: &Path, args: &[String]) -> SpawnCommandSpec {
    let mut command_args = vec![
        "-NoProfile".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        command.display().to_string(),
    ];
    command_args.extend(args.iter().cloned());
    SpawnCommandSpec::direct("powershell.exe", command_args)
}
