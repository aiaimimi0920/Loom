//! No-follow bounded reads and destructive cache-root validation shared by desktop commands.

use super::*;
use std::fs::{File, OpenOptions};

pub(super) const MAX_SETTINGS_FILE_BYTES: u64 = 1024 * 1024;
pub(super) const MAX_PACKAGE_CATALOG_BYTES: u64 = 4 * 1024 * 1024;

pub(super) fn read_bounded_regular_file(
    path: &Path,
    max_bytes: u64,
    subject: &str,
) -> Result<Vec<u8>, String> {
    let file = open_regular_file(path, subject)?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("无法读取{subject}信息 `{}`：{error}", path.display()))?;
    if metadata.len() > max_bytes {
        return Err(format!(
            "{subject}超过 {max_bytes} 字节限制：{}",
            path.display()
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取{subject} `{}`：{error}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{subject}超过 {max_bytes} 字节限制：{}",
            path.display()
        ));
    }
    Ok(bytes)
}

pub(super) fn read_bounded_utf8_file(
    path: &Path,
    max_bytes: u64,
    subject: &str,
) -> Result<String, String> {
    String::from_utf8(read_bounded_regular_file(path, max_bytes, subject)?)
        .map_err(|error| format!("{subject}不是 UTF-8 `{}`：{error}", path.display()))
}

pub(super) fn write_utf8_regular_file(
    path: &Path,
    contents: &str,
    subject: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建{subject}目录 `{}`：{error}", parent.display()))?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    configure_no_follow(&mut options);
    let mut file = options
        .open(path)
        .map_err(|error| format!("无法打开{subject} `{}`：{error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("无法检查{subject} `{}`：{error}", path.display()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata_is_windows_reparse(&metadata)
    {
        return Err(format!("{subject}不是普通文件：{}", path.display()));
    }
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("无法写入{subject} `{}`：{error}", path.display()))
}

pub(super) fn validate_destructive_cache_root(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(format!("拒绝清理不安全的缓存路径 `{}`。", path.display()));
    }
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        return Err(format!(
            "拒绝清理包含相对跳转的缓存路径 `{}`。",
            path.display()
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || metadata_is_windows_reparse(&metadata) =>
        {
            return Err(format!("拒绝清理链接或非目录缓存 `{}`。", path.display()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!("无法检查缓存目录 `{}`：{error}", path.display()));
        }
    }

    let candidate = canonical_or_absolute(path)?;
    for protected in protected_destructive_roots() {
        let protected = canonical_or_absolute(&absolute_path(&protected)?)?;
        if protected.starts_with(&candidate) {
            return Err(format!(
                "拒绝清理包含受保护目录 `{}` 的缓存路径 `{}`。",
                protected.display(),
                candidate.display()
            ));
        }
    }
    Ok(())
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(path))
        .map_err(|error| format!("无法解析相对路径 `{}`：{error}", path.display()))
}

fn open_regular_file(path: &Path, subject: &str) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let file = options
        .open(path)
        .map_err(|error| format!("无法打开{subject} `{}`：{error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("无法检查{subject} `{}`：{error}", path.display()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata_is_windows_reparse(&metadata)
    {
        return Err(format!("{subject}不是普通文件：{}", path.display()));
    }
    Ok(file)
}

fn canonical_or_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("路径不是绝对路径：{}", path.display()));
    }
    let mut ancestor = path;
    let mut missing_parts = Vec::new();
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut canonical) => {
                for part in missing_parts.iter().rev() {
                    canonical.push(part);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let part = ancestor
                    .file_name()
                    .ok_or_else(|| format!("无法找到路径 `{}` 的现有父目录。", path.display()))?;
                missing_parts.push(part.to_os_string());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| format!("无法找到路径 `{}` 的现有父目录。", path.display()))?;
            }
            Err(error) => {
                return Err(format!("无法规范化路径 `{}`：{error}", path.display()));
            }
        }
    }
}

fn protected_destructive_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        std::env::temp_dir(),
        desktop_control_plane_root(),
        hook_effective_app_data_dir(),
    ];
    roots.extend(
        ["USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA"]
            .into_iter()
            .filter_map(std::env::var_os)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
    );
    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir);
    }
    roots
}

#[cfg(unix)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(windows)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(unix, windows)))]
fn configure_no_follow(_options: &mut OpenOptions) {}

fn metadata_is_windows_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}
