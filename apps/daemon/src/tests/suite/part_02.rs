// Loom daemon tests fragment 2; included into the shared crate test module.
#[test]
fn platform_global_art_ids_use_the_na_numeric_contract() {
    assert!(is_platform_global_art_id("NA40000000000"));
    assert!(!is_platform_global_art_id("local-art"));
    assert!(!is_platform_global_art_id("NA123"));
    assert!(!is_platform_global_art_id("NB40000000000"));
    assert!(!is_platform_global_art_id("NA4000000000x"));
}

#[test]
fn publisher_ids_accept_the_default_test_contract() {
    assert!(is_platform_publisher_id(DEFAULT_TEST_PUBLISHER_ID));
    assert!(is_platform_publisher_id("NU10000000000"));
    assert!(!is_platform_publisher_id("L000000000"));
    assert!(!is_platform_publisher_id("L000000000x"));
}

#[test]
fn device_registry_bootstraps_one_protected_local_host() {
    let root = unique_temp_dir("local-host-device");
    let path = root.join("settings").join("devices.json");
    let address = "127.0.0.1:18766".parse().expect("local address");
    let store = DeviceRegistryStore::new(path.clone(), address).expect("open device registry");
    let local = store
        .devices
        .get("device-000-local")
        .expect("local host device");
    assert!(local.is_local);
    assert_eq!(local.address, "127.0.0.1:18766");
    assert_eq!(local.approval, "approved");
    drop(store);

    let reloaded = DeviceRegistryStore::new(path, address).expect("reopen device registry");
    assert_eq!(reloaded.devices.len(), 1);
    let registry = Arc::new(Mutex::new(reloaded));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
    let (status, _) = remove_managed_device("device-000-local", &registry, &hook_bridge)
        .expect("protected local device response");
    assert_eq!(status, 400);
    let _ = fs::remove_dir_all(root);
}

/// Every temporary that `create_sensitive_temporary` could have produced for `path`.
fn temporary_siblings(path: &Path) -> Vec<PathBuf> {
    let parent = path.parent().expect("temporary parent");
    let prefix = format!(
        ".{}.tmp-",
        path.file_name()
            .and_then(|name| name.to_str())
            .expect("temporary file name")
    );
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| name.starts_with(&prefix))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect()
}

#[test]
fn device_registry_refuses_to_start_when_the_stored_file_is_unparsable() {
    let root = unique_temp_dir("device-registry-corrupt");
    let path = root.join("settings").join("devices.json");
    fs::create_dir_all(path.parent().expect("settings directory")).expect("settings directory");
    let corrupt = br#"{"devices": [ {"id": "device-001-remote", "sessionEp"#;
    fs::write(&path, corrupt).expect("write a corrupt registry");
    let address = "127.0.0.1:18767".parse().expect("local address");

    // Matched rather than `expect_err` on purpose: the store holds live session material, so
    // it deliberately does not implement `Debug`.
    let error = match DeviceRegistryStore::new(path.clone(), address) {
        Ok(_) => panic!("an unparsable registry must not be read as an empty one"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(message.contains("device registry"), "{message}");
    assert!(message.contains(&path.display().to_string()), "{message}");

    // The paired devices and their revocation counters are still on disk for recovery: a
    // loader that defaulted here would have persisted a registry holding only the local host.
    assert_eq!(
        fs::read(&path).expect("read the registry back"),
        corrupt.to_vec()
    );

    // An absent file is the legitimate first-run case and still bootstraps the local host.
    fs::remove_file(&path).expect("remove the corrupt registry");
    let store = DeviceRegistryStore::new(path, address).expect("an empty registry bootstraps");
    assert_eq!(store.devices.len(), 1);
    assert!(
        store
            .devices
            .get("device-000-local")
            .expect("local host device")
            .is_local
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn publisher_identity_replacement_always_leaves_a_readable_file() {
    let root = unique_temp_dir("publisher-identity-atomic");
    fs::create_dir_all(&root).expect("control plane root");
    let identity = |generation: u32| LocalPublisherIdentity {
        schema_version: publisher_identity_schema_version(),
        user_id: format!("NU100000000{generation:02}"),
        current_key_id: format!("key-{generation}"),
        public_key: format!("public-{generation}"),
    };
    save_publisher_identity(&root, &identity(0)).expect("seed the identity");

    // Read the identity as fast as the filesystem allows while it is being replaced. The
    // previous implementation deleted the live file before renaming the temporary over it, so
    // a reader in that window saw `Ok(None)` — no identity at all — and this loop catches it.
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let reader_root = root.clone();
    let reader_stop = Arc::clone(&stop);
    let reader_observed = Arc::clone(&observed);
    let reader = thread::spawn(move || -> std::result::Result<u32, String> {
        let mut reads = 0_u32;
        // Read once before consulting `stop`: on a loaded machine this thread can be scheduled
        // for the first time after the writer below has already finished, and a reader that
        // never looked at the file would report a vacuous pass.
        loop {
            match load_publisher_identity(&reader_root) {
                Ok(Some(_)) => {
                    reads = reads.saturating_add(1);
                    reader_observed.store(reads, std::sync::atomic::Ordering::Release);
                }
                Ok(None) => {
                    return Err("the identity file was missing during a replacement".to_owned())
                }
                // A parse failure means the reader saw half-written bytes, which is the defect
                // this test exists to catch. A read failure is different: Windows can refuse to
                // open the destination for the instant a replacement holds it, and that refusal
                // says nothing about the file's contents, so it is not counted as a violation.
                Err(error) if error.starts_with("用户签名身份无效") => {
                    return Err(format!("a partial identity was observed: {error}"))
                }
                Err(_) => {}
            }
            if reader_stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            // Sample often but not in a hot spin: a busy loop here would both starve the
            // timing-sensitive tests running alongside and keep the file open so continuously
            // that no replacement could win the race.
            thread::sleep(Duration::from_millis(1));
        }
        Ok(reads)
    });
    // Do not start replacing until the reader is actually running, so the writes below overlap
    // it instead of racing thread startup.
    for _ in 0..2_000 {
        if observed.load(std::sync::atomic::Ordering::Acquire) > 0 {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    let generations = 12_u32;
    let mut contended_replacements = 0_u32;
    for generation in 1..=generations {
        match save_publisher_identity(&root, &identity(generation)) {
            Ok(()) => contended_replacements = contended_replacements.saturating_add(1),
            // Windows refuses a rename-with-replace, and sometimes even the creation of the
            // temporary, while another handle has the directory entry open — and the reader
            // below holds one for most of this loop. Any such refusal is a safe outcome: the
            // previous identity stays whole, which is exactly the property under test, so the
            // loop tolerates it and the reader is what has to stay clean. A serialization
            // failure is not an I/O race, so that one still fails the test.
            Err(error) => assert!(
                !error.contains("serialize JSON"),
                "unexpected identity write failure: {error}"
            ),
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let reads = reader
        .join()
        .expect("reader thread")
        .expect("every read saw a whole identity");
    assert!(
        reads > 0,
        "the reader never observed the identity file \
             (contended replacements: {contended_replacements})"
    );
    // Every contended replacement above is allowed to be refused, so one write with the reader
    // stopped anchors the final-state assertions below without depending on how the race played
    // out. Windows can still hold the directory entry briefly after the reader's last open, so
    // this write gets a bounded retry rather than one chance.
    let final_generation = generations + 1;
    let mut settled = save_publisher_identity(&root, &identity(final_generation));
    for _ in 0..50 {
        if settled.is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
        settled = save_publisher_identity(&root, &identity(final_generation));
    }
    settled.expect("uncontended replacement");

    let stored = load_publisher_identity(&root)
        .expect("load the final identity")
        .expect("the final identity is present");
    assert_eq!(stored.current_key_id, format!("key-{final_generation}"));
    assert!(
        temporary_siblings(&publisher_identity_path(&root)).is_empty(),
        "the replacements left temporaries behind"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn atomic_json_writes_keep_the_previous_file_and_leave_no_temporary() {
    let root = unique_temp_dir("atomic-json-write");
    let path = root.join("nested").join("state.json");

    write_json_atomically(&path, &json!({ "generation": 1 })).expect("first atomic write");
    let succeeded = fs::read(&path).expect("read after a successful write");
    assert_eq!(
        serde_json::from_slice::<Value>(&succeeded).expect("parse after a successful write"),
        json!({ "generation": 1 })
    );
    assert!(
        temporary_siblings(&path).is_empty(),
        "a successful write left a temporary behind"
    );

    // A serialization failure happens before the destination is touched at all.
    let mut unserializable: BTreeMap<(u8, u8), u8> = BTreeMap::new();
    unserializable.insert((1, 2), 3);
    let error = write_json_atomically(&path, &unserializable)
        .expect_err("a map with tuple keys cannot be JSON");
    assert!(format!("{error:#}").contains("serialize JSON"), "{error:#}");

    // A replacement that cannot complete must clean up its temporary and leave the
    // destination exactly as it was. A directory in the destination's place is the cheapest
    // portable way to make `replace_sensitive_file` fail after the temporary exists.
    let blocked = root.join("nested").join("blocked.json");
    fs::create_dir_all(blocked.join("child")).expect("put a directory in the way");
    let error = write_json_atomically(&blocked, &json!({ "generation": 2 }))
        .expect_err("replacing a directory must fail");
    assert!(
        format!("{error:#}").contains("atomically replace"),
        "{error:#}"
    );
    assert!(
        blocked.join("child").is_dir(),
        "the destination was damaged"
    );
    assert!(
        temporary_siblings(&blocked).is_empty(),
        "the failed write left a temporary behind"
    );

    assert_eq!(
        fs::read(&path).expect("read after the failures"),
        succeeded,
        "the previous contents did not survive a failed write"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn device_update_persists_enabled_state_and_removal() {
    let root = unique_temp_dir("device-update");
    let registry = Arc::new(Mutex::new(
        DeviceRegistryStore::new(
            root.join("settings").join("devices.json"),
            "127.0.0.1:18766".parse().expect("local address"),
        )
        .expect("open device registry"),
    ));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
    add_managed_device(
        r#"{"name":"iPad","kind":"tablet","address":"192.168.1.36"}"#,
        "approved",
        &registry,
        &hook_bridge,
    )
    .expect("add remote device");
    let remote_id = registry
        .lock()
        .expect("device registry")
        .devices
        .values()
        .find(|device| !device.is_local)
        .expect("remote device")
        .id
        .clone();
    update_managed_device(
        &remote_id,
        r#"{"name":"Studio iPad","kind":"tablet","address":"192.168.1.37","enabled":false}"#,
        &registry,
        &hook_bridge,
    )
    .expect("disable remote device");
    let updated = registry
        .lock()
        .expect("device registry")
        .devices
        .get(&remote_id)
        .expect("updated remote device")
        .clone();
    assert_eq!(updated.name, "Studio iPad");
    assert_eq!(updated.address, "192.168.1.37");
    assert!(!updated.enabled);
    remove_managed_device(&remote_id, &registry, &hook_bridge).expect("remove remote device");
    assert_eq!(registry.lock().expect("device registry").devices.len(), 1);
    let _ = fs::remove_dir_all(root);
}
