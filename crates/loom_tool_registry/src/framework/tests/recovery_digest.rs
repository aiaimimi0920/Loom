//! Crash recovery treats the persisted installation record as the commit point.
use super::*;

const ID: &str = "publisher.test/process";

fn replacement_package() -> Vec<u8> {
    use std::io::{Read, Write};
    let bytes = fake_framework_package_zip_with_version("process", "1.0.0");
    let mut input = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for index in 0..input.len() {
        let mut file = input.by_index(index).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        if file.name().starts_with("runtime/") {
            bytes.extend_from_slice(b"-replacement");
        }
        output
            .start_file(file.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn recovery_pairs_same_version_activation_and_pin_at_every_commit_boundary() {
    for rollback in [false, true] {
        for activation_written in [false, true] {
            for state_written in [false, true] {
                if state_written && !activation_written {
                    continue;
                }
                let root = temp_root();
                let registry = FrameworkRegistry::new(&root);
                registry
                    .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
                        "process", "1.0.0",
                    ))
                    .unwrap();
                let first_activation = registry.activation(ID).unwrap();
                let first_states = registry.installation_states().unwrap();
                registry
                    .install_framework_package_from_zip(&replacement_package())
                    .unwrap();
                let second_activation = registry.activation(ID).unwrap();
                let second_states = registry.installation_states().unwrap();
                assert_ne!(
                    first_states[ID].package_digest,
                    second_states[ID].package_digest
                );
                let (old, next, old_states, next_states) = if rollback {
                    (
                        second_activation.clone(),
                        FrameworkActivationState {
                            active: first_activation.active.clone(),
                            previous: Some(second_activation.active.clone()),
                        },
                        second_states,
                        first_states,
                    )
                } else {
                    (
                        first_activation,
                        second_activation,
                        first_states,
                        second_states,
                    )
                };
                registry
                    .write_installed(if state_written {
                        &next_states
                    } else {
                        &old_states
                    })
                    .unwrap();
                registry
                    .write_activation(ID, if activation_written { &next } else { &old })
                    .unwrap();
                registry
                    .write_lifecycle_journal(
                        ID,
                        &FrameworkLifecycleJournal {
                            old_activation: Some(old.clone()),
                            next_activation: next.clone(),
                            next_installation: Some(next_states[ID].clone()),
                            target: next.active.clone(),
                            // Both versions already exist in this simulated interrupted operation.
                            created_target: false,
                        },
                    )
                    .unwrap();
                let expected = if state_written { next } else { old };
                let expected_states = if state_written {
                    next_states
                } else {
                    old_states
                };
                for _ in 0..2 {
                    let recovered = FrameworkRegistry::new(&root);
                    assert_eq!(recovered.activation(ID), Some(expected.clone()));
                    assert_eq!(recovered.installation_states().unwrap(), expected_states);
                    let (ready, detail) = framework_ready_in(ID, Some(&root.join("frameworks")));
                    assert!(ready, "rollback={rollback} activation={activation_written} state={state_written}: {detail}");
                    assert!(!recovered.lifecycle_path(ID).exists());
                }
                set_framework_tree_readonly(&root, false).unwrap();
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
}
