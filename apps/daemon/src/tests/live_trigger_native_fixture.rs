// Real binaries must be supplied explicitly and match digests; never spoof execution success with legacy MZ text fixtures or runner injection.
fn live_trigger_executable(name: &str) -> Vec<u8> {
    let path = PathBuf::from(std::env::var_os(name).unwrap());
    let expected = std::env::var(format!("{name}_SHA256")).unwrap();
    let canonical = fs::canonicalize(&path).unwrap();
    let repo_tmp = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tmp");
    let allowed = [
        repo_tmp.as_path(),
        std::path::Path::new("C:/Users/Public/BuildStaging"),
        std::path::Path::new("C:/Users/Public/Project/Release"),
        std::path::Path::new("C:/Users/Public/nas_home/AI/GameEditor/linshi"),
    ];
    assert!(
        path.is_absolute()
            && allowed
                .iter()
                .filter_map(|candidate| fs::canonicalize(candidate).ok())
                .any(|base| canonical != base && canonical.starts_with(&base))
    );
    assert!(fs::metadata(&canonical).unwrap().len() <= 64 * 1024 * 1024);
    let bytes = fs::read(canonical).unwrap();
    assert_eq!(
        sha256_bytes(&bytes).to_ascii_lowercase(),
        expected.to_ascii_lowercase()
    );
    assert!(bytes.starts_with(b"MZ"));
    bytes
}
fn live_trigger_replace_zip(package: Vec<u8>, name: &str, replacement: &[u8]) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(package)).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut replaced = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let path = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if path == name {
            bytes = replacement.to_vec();
            replaced = true;
        }
        writer
            .start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    assert!(replaced);
    writer.finish().unwrap().into_inner()
}
fn live_trigger_executions(root: &std::path::Path) -> Vec<PathBuf> {
    fn visit(root: &std::path::Path, paths: &mut Vec<PathBuf>, remaining: &mut usize) {
        for entry in fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            *remaining = remaining
                .checked_sub(1)
                .expect("bounded isolated fixture tree");
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink());
            if kind.is_dir() {
                visit(&entry.path(), paths, remaining);
            } else if entry.file_name() == "executions.txt" {
                paths.push(entry.path());
            }
        }
    }
    let mut paths = Vec::new();
    visit(root, &mut paths, &mut 4096);
    paths
}
