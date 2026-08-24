// Declared dependency, version, normalization, and call validation contracts.
fn declared_dependency(id: &str, version: &str) -> ArtDependencies {
    ArtDependencies {
        mcp_servers: vec![ArtMcpServerDependency {
            id: id.to_owned(),
            version: version.to_owned(),
        }],
    }
}

#[test]
fn declared_dependency_must_match_the_mcp_package_and_version() {
    let config = multi_call_config();
    validate_declared_dependency(
        &config,
        &declared_dependency("neuro.official/stock-api", "=2.9.0"),
    )
    .unwrap();

    assert!(
        validate_declared_dependency(&config, &ArtDependencies::default())
            .unwrap_err()
            .contains("found 0")
    );
    assert!(validate_declared_dependency(
        &config,
        &declared_dependency("neuro.official/stock-api", "=2.9.1")
    )
    .unwrap_err()
    .contains("disagrees with the declared dependency version"));
}

#[test]
fn requirement_bounds_cover_the_comparator_forms_art_manifests_use() {
    for (requirement, lower, upper) in [
        ("=2.9.0", [2, 9, 0], [2, 9, 1]),
        ("=2.9", [2, 9, 0], [2, 10, 0]),
        ("^0.1", [0, 1, 0], [0, 2, 0]),
        ("^0.0.3", [0, 0, 3], [0, 0, 4]),
        ("^0", [0, 0, 0], [1, 0, 0]),
        ("1.2.3", [1, 2, 3], [2, 0, 0]),
        ("~1.2", [1, 2, 0], [1, 3, 0]),
        ("~1.2.3", [1, 2, 3], [1, 3, 0]),
        ("~1", [1, 0, 0], [2, 0, 0]),
    ] {
        assert_eq!(
            requirement_bounds(requirement),
            Some(VersionBounds { lower, upper }),
            "{requirement}"
        );
    }
}

#[test]
fn undecidable_requirements_are_left_to_the_authoritative_checker() {
    // `None` means "this framework will not judge it", which is why nothing downstream may read
    // it as "satisfied".
    for requirement in [
        ">=1.0.0",
        "<2",
        "*",
        ">=1.0.0, <2.0.0",
        "1.x",
        "^1.0.0-rc.1",
        "",
    ] {
        assert_eq!(requirement_bounds(requirement), None, "{requirement}");
    }
}

#[test]
fn resolved_version_outside_the_declared_range_is_rejected() {
    let mut config = multi_call_config();
    config.server_id = "fixture".to_owned();
    config.package_id = "publisher.test/fixture".to_owned();
    config.version = "^0.1".to_owned();

    let mut resolved = resolved_server();
    resolved.version = "0.1.9".to_owned();
    validate_resolved_server(&config, &resolved).unwrap();

    resolved.version = "0.2.0".to_owned();
    let error = validate_resolved_server(&config, &resolved).unwrap_err();
    assert!(
        error.contains("does not satisfy Art dependency `^0.1`"),
        "{error}"
    );
    assert!(error.contains("needs >= 0.1.0 and < 0.2.0"), "{error}");

    resolved.version = "   ".to_owned();
    assert!(validate_resolved_server(&config, &resolved)
        .unwrap_err()
        .contains("reported no version"));

    // Build metadata is ignored, pre-release ordering is not judged, and a requirement this
    // framework cannot parse leaves the decision to the host.
    resolved.version = "0.1.4+build.7".to_owned();
    validate_resolved_server(&config, &resolved).unwrap();
    resolved.version = "0.2.0-rc.1".to_owned();
    validate_resolved_server(&config, &resolved).unwrap();
    config.version = ">=0.1.0".to_owned();
    resolved.version = "9.9.9".to_owned();
    validate_resolved_server(&config, &resolved).unwrap();
}

#[test]
fn config_identifiers_are_stored_trimmed_so_selections_match() {
    let mut config = multi_call_config();
    config.version = " =2.9.0 ".to_owned();
    config.calls[0].id = " quote ".to_owned();
    config.calls[0].tool_name = " get_stock ".to_owned();
    config.surface_actions.insert(
        " stock_trimmed ".to_owned(),
        McpSurfaceActionConfig {
            calls: Some(vec![" quote ".to_owned()]),
            arguments: BTreeMap::new(),
        },
    );

    normalize_config(&mut config).unwrap();
    assert_eq!(config.version, "=2.9.0");
    assert_eq!(config.calls[0].id, "quote");
    assert_eq!(config.calls[0].tool_name, "get_stock");
    assert_eq!(
        config.surface_actions["stock_trimmed"].calls.as_deref(),
        Some(["quote".to_owned()].as_slice())
    );
    validate_call_config(&config).unwrap();
    validate_surface_actions(&config).unwrap();
}

#[test]
fn surface_action_ids_that_collide_after_trimming_are_rejected() {
    let mut config = multi_call_config();
    config.surface_actions.insert(
        " stock_refresh".to_owned(),
        McpSurfaceActionConfig::default(),
    );
    assert!(normalize_config(&mut config)
        .unwrap_err()
        .contains("duplicate MCP Surface action id"));
}

#[test]
fn legacy_tool_name_is_held_to_the_multi_call_rules() {
    let mut config = multi_call_config();
    config.calls.clear();
    config.surface_actions.clear();

    config.tool_name = Some("get\u{7}stock".to_owned());
    assert!(validate_call_config(&config)
        .unwrap_err()
        .contains("invalid toolName"));
    config.tool_name = Some("x".repeat(257));
    assert!(validate_call_config(&config)
        .unwrap_err()
        .contains("non-empty toolName"));
    config.tool_name = Some("get_stock".to_owned());
    validate_call_config(&config).unwrap();
}
