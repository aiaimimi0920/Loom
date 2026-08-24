use std::path::Path;
use std::process::Command;

fn git_value(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn main() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let hook_repo_root = repo_root
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("Hook");
    let repository = git_value(&repo_root, &["config", "--get", "remote.origin.url"])
        .map(|value| value.trim_end_matches(".git").to_owned())
        .unwrap_or_else(|| "https://github.com/aiaimimi0920/Loom".to_owned());
    let commit = git_value(&repo_root, &["rev-parse", "HEAD"])
        .map(|value| value.chars().take(6).collect())
        .unwrap_or_else(|| "unknown".to_owned());
    let hook_repository = git_value(&hook_repo_root, &["config", "--get", "remote.origin.url"])
        .map(|value| value.trim_end_matches(".git").to_owned())
        .unwrap_or_else(|| "https://github.com/aiaimimi0920/Hook".to_owned());
    let hook_commit = git_value(&hook_repo_root, &["rev-parse", "HEAD"])
        .map(|value| value.chars().take(6).collect())
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=LOOM_BUILD_REPOSITORY={repository}");
    println!("cargo:rustc-env=LOOM_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=HOOK_BUILD_REPOSITORY={hook_repository}");
    println!("cargo:rustc-env=HOOK_BUILD_COMMIT={hook_commit}");
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../.git/refs/heads");
    // Rust caches build-script output; invalidate it when CI checks out a new commit
    // even if the detached .git/HEAD path keeps the same timestamp.
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=../../../../Hook/.git/HEAD");
    println!("cargo:rerun-if-changed=../../../../Hook/.git/refs/heads");
    println!("cargo:rerun-if-changed=icons/loom-icon.svg");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
