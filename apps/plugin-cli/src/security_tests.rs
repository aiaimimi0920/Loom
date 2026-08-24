// Filesystem, resource-bound and diagnostic-redaction regression coverage.
#[cfg(test)]
mod security_tests {
    use super::*;

    fn complete_art(root: &Path, id: &str) {
        init_art(
            root,
            id,
            "publisher.example/process",
            "publisher.example",
        )
        .expect("initialize Art fixture");
        fs::write(root.join("runtime/main.exe"), b"MZ-art-fixture")
            .expect("write Art payload");
    }

    #[cfg(unix)]
    fn create_file_link(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    fn create_file_link(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }

    fn failing_conformance_fixture(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let script = root.join("failing-conformance.ps1");
            fs::write(
                &script,
                "$null = [Console]::In.ReadToEnd()\n[Console]::Error.Write('TOP_SECRET_DIAGNOSTIC')\nexit 7\n",
            )
            .expect("write failing PowerShell fixture");
            let wrapper = root.join("failing-conformance.cmd");
            fs::write(
                &wrapper,
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0failing-conformance.ps1\"\r\n",
            )
            .expect("write failing command fixture");
            wrapper
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let wrapper = root.join("failing-conformance.sh");
            fs::write(
                &wrapper,
                "#!/bin/sh\ncat >/dev/null\nprintf '%s' 'TOP_SECRET_DIAGNOSTIC' >&2\nexit 7\n",
            )
            .expect("write failing shell fixture");
            let mut permissions = fs::metadata(&wrapper).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&wrapper, permissions).unwrap();
            wrapper
        }
    }

    #[test]
    fn manifest_reads_are_bounded() {
        let root = tests::temp_root("bounded-manifest");
        let path = root.join("manifest.json");
        fs::write(&path, vec![b' '; MAX_MANIFEST_BYTES as usize + 1])
            .expect("write oversized manifest");

        let error = read_json::<Value>(&path).expect_err("oversized manifest must fail");
        assert!(format!("{error:#}").contains("byte limit"), "{error:#}");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn package_output_inside_source_is_rejected() {
        let root = tests::temp_root("output-inside-source");
        complete_art(&root, "inside-output-art");
        let output = root.join("package.zip");

        let error = pack_directory(&root, &output).expect_err("self-including output must fail");
        assert!(error.to_string().contains("outside the source"), "{error:#}");
        assert!(!output.exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn streaming_package_bytes_remain_deterministic() {
        let root = tests::temp_root("deterministic-pack");
        let source = root.join("art");
        complete_art(&source, "deterministic-art");
        let first = root.join("first.zip");
        let second = root.join("second.zip");

        pack_directory(&source, &first).expect("first package");
        pack_directory(&source, &second).expect("second package");
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn existing_archive_and_digest_are_replaced_together() {
        let root = tests::temp_root("replace-existing-pack");
        let source = root.join("art");
        complete_art(&source, "replace-existing-art");
        let output = root.join("package.zip");
        let digest = package_digest_path(&output);
        fs::write(&output, b"old archive").unwrap();
        fs::write(&digest, b"old digest").unwrap();

        pack_directory(&source, &output).expect("replace package outputs");
        assert_ne!(fs::read(&output).unwrap(), b"old archive");
        let sidecar = fs::read_to_string(&digest).unwrap();
        assert!(!sidecar.contains("old digest"));
        assert!(sidecar.ends_with("  package.zip\n"), "{sidecar}");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn package_root_and_payload_links_are_rejected() {
        let root = tests::temp_root("package-links");
        let source = root.join("art");
        complete_art(&source, "linked-art");
        let linked_root = root.join("linked-root");
        if !create_directory_link(&source, &linked_root) {
            fs::remove_dir_all(root).ok();
            return;
        }
        assert!(validate_path_with_payload(&linked_root, true, &TrustStore::default()).is_err());

        fs::remove_file(source.join("runtime/main.exe")).unwrap();
        let external = root.join("external.exe");
        fs::write(&external, b"external payload").unwrap();
        assert!(create_file_link(&external, &source.join("runtime/main.exe")));
        let error = validate_path_with_payload(&source, true, &TrustStore::default())
            .expect_err("linked payload must fail");
        assert!(error.to_string().contains("links are not allowed"), "{error:#}");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn atomic_output_rejects_links_without_touching_target() {
        let root = tests::temp_root("atomic-output-link");
        let target = root.join("target.txt");
        let output = root.join("output.txt");
        fs::write(&target, b"sentinel").unwrap();
        if !create_file_link(&target, &output) {
            fs::remove_dir_all(root).ok();
            return;
        }

        assert!(write_bytes_atomic(&output, b"replacement").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"sentinel");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn digest_and_output_parent_links_are_rejected() {
        let root = tests::temp_root("linked-package-outputs");
        let source = root.join("art");
        complete_art(&source, "linked-output-art");
        let output = root.join("package.zip");
        let digest = package_digest_path(&output);
        let sentinel = root.join("sentinel.txt");
        fs::write(&sentinel, b"sentinel").unwrap();
        if !create_file_link(&sentinel, &digest) {
            fs::remove_dir_all(root).ok();
            return;
        }
        assert!(pack_directory(&source, &output).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read(&sentinel).unwrap(), b"sentinel");
        fs::remove_file(&digest).ok();

        let real_parent = root.join("real-output");
        fs::create_dir(&real_parent).unwrap();
        let linked_parent = root.join("linked-output");
        assert!(create_directory_link(&real_parent, &linked_parent));
        assert!(pack_directory(&source, &linked_parent.join("package.zip")).is_err());
        assert!(!real_parent.join("package.zip").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn signing_key_links_are_rejected_without_overwrite() {
        let root = tests::temp_root("linked-signing-key");
        let target = root.join("target-key.json");
        let linked = root.join("linked-key.json");
        let key = generate_signing_key("safe-key");
        write_signing_key_document(&target, &key).expect("write target key");
        if !create_file_link(&target, &linked) {
            fs::remove_dir_all(root).ok();
            return;
        }

        assert!(read_signing_key_document(&linked).is_err());
        let before = fs::read(&target).unwrap();
        assert!(write_signing_key_document(&linked, &generate_signing_key("replacement")).is_err());
        assert_eq!(fs::read(&target).unwrap(), before);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn conformance_failure_does_not_echo_child_output() {
        let root = tests::temp_root("redacted-conformance");
        let art = root.join("art");
        complete_art(&art, "redacted-conformance-art");
        let executable = failing_conformance_fixture(&root);

        let linked_executable = root.join(if cfg!(windows) {
            "linked-conformance.cmd"
        } else {
            "linked-conformance.sh"
        });
        if create_file_link(&executable, &linked_executable) {
            assert!(run_conformance(&linked_executable, "publisher.example/process", &art).is_err());
        }

        let error = run_conformance(&executable, "publisher.example/process", &art)
            .expect_err("failing framework must fail conformance");
        let message = format!("{error:#}");
        assert!(message.contains("stderrBytes="), "{message}");
        assert!(!message.contains("TOP_SECRET_DIAGNOSTIC"), "{message}");
        fs::remove_dir_all(root).ok();
    }
}
