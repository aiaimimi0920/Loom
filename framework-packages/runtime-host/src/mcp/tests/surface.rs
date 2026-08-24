// Surface action selection, binding, disablement, and path isolation.
#[test]
fn surface_action_maps_only_declared_values_into_multiple_calls() {
    let mut request = request();
    request.inputs = json!({
        "surfaceAction": {
            "actionId": "stock_symbol_commit",
            "payload": { "value": "SZ000034", "ignored": "do-not-forward" },
            "authoritativeState": { "code": "SH600000" }
        },
        "untrusted": "do-not-forward"
    });
    request.params = json!({ "alsoUntrusted": true });

    let calls = resolve_calls(&request, &multi_call_config()).unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "quote");
    assert_eq!(
        calls[0].arguments,
        json!({ "source": "auto", "code": "SZ000034" })
    );
    assert_eq!(calls[1].id, "history");
    assert_eq!(
        calls[1].arguments,
        json!({
            "source": "auto",
            "period": "day",
            "count": 60,
            "code": "SZ000034"
        })
    );
}

#[test]
fn surface_action_can_explicitly_skip_mcp_calls() {
    let mut request = request();
    request.inputs = json!({
        "surfaceAction": {
            "actionId": "stock_interval_commit",
            "payload": { "value": 120 },
            "authoritativeState": { "code": "SZ000034" }
        }
    });
    request.params = json!({});

    let calls = resolve_calls(&request, &multi_call_config()).unwrap();
    assert!(calls.is_empty());
}

#[test]
fn surface_action_binding_cannot_target_a_disabled_parameter() {
    let mut request = request();
    request.inputs = json!({
        "surfaceAction": {
            "actionId": "stock_symbol_commit",
            "payload": { "value": "SZ000034" }
        }
    });
    request.params = json!({});
    request.disabled_params = vec!["code".to_owned()];

    let error = resolve_calls(&request, &multi_call_config()).unwrap_err();
    assert!(error.contains("both bound"), "{error}");
}

#[test]
fn surface_binding_distinguishes_null_from_a_missing_path() {
    let invocation = json!({
        "payload": {
            "nullValue": null,
            "falseValue": false,
            "zeroValue": 0,
            "emptyValue": ""
        }
    });
    assert_eq!(
        value_at_binding_path(&invocation, "payload.nullValue"),
        Some(&Value::Null)
    );
    assert_eq!(
        value_at_binding_path(&invocation, "payload.falseValue"),
        Some(&json!(false))
    );
    assert_eq!(
        value_at_binding_path(&invocation, "payload.zeroValue"),
        Some(&json!(0))
    );
    assert_eq!(
        value_at_binding_path(&invocation, "payload.emptyValue"),
        Some(&json!(""))
    );
    assert_eq!(value_at_binding_path(&invocation, "payload.missing"), None);
}

#[test]
fn surface_bindings_reject_context_and_credential_paths() {
    let mut config = multi_call_config();
    config.surface_actions.insert(
        "stock_unsafe".to_owned(),
        McpSurfaceActionConfig {
            calls: None,
            arguments: BTreeMap::from([(
                "code".to_owned(),
                McpArgumentBinding {
                    from: vec!["context.credentials.0.value".to_owned()],
                },
            )]),
        },
    );
    assert!(validate_surface_actions(&config)
        .unwrap_err()
        .contains("payload or authoritativeState"));
}
