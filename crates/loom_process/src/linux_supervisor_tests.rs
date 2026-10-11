use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::Command;

use crate::{run_with_input, ProcessError, ProcessSpec};

#[test]
fn nested_runtime_uses_pinned_files_despite_overflow_uid_mounts() {
    let mut spec = ProcessSpec::new(std::env::current_exe().unwrap());
    spec.args = vec![
        "--exact".into(),
        "linux_supervisor_tests::remount_worker".into(),
        "--nocapture".into(),
    ];
    spec.env
        .insert("LOOM_TEST_REMOUNT_STAGE".into(), "mount".into());
    // A package must not replace the reserved launch capability through ProcessSpec.
    spec.env
        .insert(crate::linux_bootstrap::ENV_KEY.into(), "0".into());
    let output = run_with_input(&spec, b"").unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PINNED_SUPERVISORS_OK"));
}

#[test]
fn remount_worker() {
    match std::env::var("LOOM_TEST_REMOUNT_STAGE").as_deref() {
        Ok("mount") => {
            let fake =
                std::env::temp_dir().join(format!("loom-fake-supervisor-{}", std::process::id()));
            std::fs::copy("/bin/false", &fake).unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o555)).unwrap();
            let mut command = Command::new("/usr/bin/bwrap");
            command.args([
                "--unshare-user",
                "--uid",
                "65534",
                "--gid",
                "65534",
                "--bind",
                "/",
                "/",
                "--bind",
            ]);
            command.arg(&fake).arg("/usr/bin/bwrap").arg("--bind");
            command
                .arg(&fake)
                .arg("/usr/bin/perl")
                .args(["--dev-bind", "/dev", "/dev", "--"]);
            command.arg(std::env::current_exe().unwrap());
            command.args([
                "--exact",
                "linux_supervisor_tests::remount_worker",
                "--nocapture",
            ]);
            let output = command
                .env("LOOM_TEST_REMOUNT_STAGE", "verify")
                .output()
                .unwrap();
            std::fs::remove_file(fake).unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("PINNED_SUPERVISORS_OK"));
            println!("PINNED_SUPERVISORS_OK");
        }
        Ok("verify") => {
            assert_eq!(std::fs::metadata("/usr/bin/bwrap").unwrap().uid(), 65534);
            // Both pathnames now point at an attacker-owned false executable. The
            // same owner number on unmapped ancestors must not establish trust.
            assert!(!Command::new("/usr/bin/bwrap").status().unwrap().success());
            let mut spec = ProcessSpec::new("/bin/echo");
            spec.args = vec!["PINNED_SUPERVISORS_OK".into()];
            let output = run_with_input(&spec, b"").unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "PINNED_SUPERVISORS_OK"
            );
            // A fresh runtime without the ancestor capability cannot bootstrap
            // from virtual root/overflow ownership or the overwritten paths.
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "linux_supervisor_tests::remount_worker",
                    "--nocapture",
                ])
                .env_remove(crate::linux_bootstrap::ENV_KEY)
                .env("LOOM_TEST_REMOUNT_STAGE", "reject")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            println!("PINNED_SUPERVISORS_OK");
        }
        Ok("reject") => {
            let result = run_with_input(&ProcessSpec::new("/bin/true"), b"");
            assert!(matches!(result, Err(ProcessError::Isolation(_))));
        }
        _ => {}
    }
}
