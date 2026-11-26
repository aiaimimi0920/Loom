//! Build script generating embedded Web UI asset table from `apps/desktop/dist` without external crates.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

fn content_type_for(relative: &str) -> &'static str {
    let lower = relative.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".json") || lower.ends_with(".map") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".ttf") {
        "font/ttf"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    let mut sorted = entries.filter_map(Result::ok).collect::<Vec<_>>();
    sorted.sort_by_key(|entry| entry.file_name());
    for entry in sorted {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            panic!(
                "refusing symlink inside embedded web UI directory: {}",
                entry.path().display()
            );
        }
        let path = entry.path();
        if file_type.is_dir() {
            let name = entry.file_name();
            if name == "web" && current == root {
                // Skip nested `dist/web` when scanning `dist` directly.
                continue;
            }
            collect_files(root, &path, out);
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("relative path inside dist root");
            if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
                continue;
            }
            let normalized = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if normalized.starts_with('.') || normalized.contains("/.") {
                continue;
            }
            out.push((normalized, path));
        }
    }
}

fn resolve_dist_dir(repo_root: &Path) -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("LOOM_EMBEDDED_UI_DIST_DIR").filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(explicit);
        watch_candidate(&dir);
        if dir.join("index.html").is_file() {
            return Some(dir);
        }
        return None;
    }
    let candidate = repo_root.join("apps/desktop/dist/web");
    watch_candidate(&candidate);
    candidate.join("index.html").is_file().then_some(candidate)
}

fn watch_candidate(path: &Path) {
    // Watching the nearest existing parent detects the first frontend build too.
    if let Some(existing) = path.ancestors().find(|candidate| candidate.is_dir()) {
        println!("cargo:rerun-if-changed={}", existing.display());
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=LOOM_EMBEDDED_UI_DIST_DIR");
    println!("cargo:rerun-if-env-changed=LOOM_REQUIRE_PREBUILT_WEB_UI");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("resolve repository root");
    let require_prebuilt = matches!(
        env::var("LOOM_REQUIRE_PREBUILT_WEB_UI").as_deref(),
        Ok("1" | "true" | "TRUE")
    );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let generated_path = out_dir.join("loom_embedded_ui_assets.rs");
    let mut file = fs::File::create(&generated_path).expect("create generated embedded UI source");

    if let Some(dist_dir) = resolve_dist_dir(repo_root) {
        let canonical_dist = fs::canonicalize(&dist_dir).unwrap_or(dist_dir);
        println!("cargo:rerun-if-changed={}", canonical_dist.display());
        let mut assets = Vec::new();
        collect_files(&canonical_dist, &canonical_dist, &mut assets);
        assert!(
            assets.iter().any(|(rel, _)| rel == "index.html"),
            "embedded Web UI dist directory is missing index.html"
        );
        writeln!(file, "pub const PREBUILT_BUNDLE: bool = true;").expect("write const");
        writeln!(file, "pub static ASSETS: &[(&str, &str, bool, &[u8])] = &[")
            .expect("write header");
        for (rel, abs_path) in &assets {
            let escaped_abs = abs_path
                .to_string_lossy()
                .replace('\\', "/")
                .replace('"', "\\\"");
            let mime = content_type_for(rel);
            let immutable = rel.starts_with("static/") && !rel.ends_with(".html");
            writeln!(
                file,
                "    ({rel:?}, {mime:?}, {immutable}, include_bytes!(\"{escaped_abs}\")),"
            )
            .expect("write asset row");
        }
        writeln!(file, "];").expect("write footer");
    } else {
        if require_prebuilt {
            panic!(
                "LOOM_REQUIRE_PREBUILT_WEB_UI=1 requires compiled Web assets in apps/desktop/dist/web (npm run build:web)"
            );
        }
        let fallback_html = concat!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
            "<title>Loom Web Management Console</title></head>",
            "<body style=\"font-family:system-ui,sans-serif;padding:2rem;background:#f5f7fa;color:#1f2937\">",
            "<h1>Loom Web Management Console</h1>",
            "<p>Loom Server has enabled the Web management mount point (<code>/ui/</code>). Static frontend assets have not been bundled in this development build; ",
            "run <code>npm run build:web</code> in <code>apps/desktop</code> and rebuild <code>loom-daemon</code>.</p>",
            "</body></html>"
        );
        writeln!(file, "pub const PREBUILT_BUNDLE: bool = false;").expect("write const");
        writeln!(
            file,
            "pub static ASSETS: &[(&str, &str, bool, &[u8])] = &[(\"index.html\", \"text/html; charset=utf-8\", false, {:?}.as_bytes())];",
            fallback_html
        )
        .expect("write fallback");
    }
}
