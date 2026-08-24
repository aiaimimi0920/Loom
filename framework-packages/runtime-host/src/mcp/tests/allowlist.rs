// Plain-call allowlisting and value-free dropped-argument warnings.
#[test]
fn plain_calls_drop_caller_arguments_the_art_never_declared() {
    let mut request = request();
    request.inputs = json!({});
    request.params = json!({ "code": "SZ000034", "interval_seconds": 120 });
    request.disabled_params = Vec::new();

    let calls = resolve_calls(&request, &multi_call_config()).unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].arguments,
        json!({ "source": "auto", "code": "SZ000034" })
    );
    assert_eq!(
        calls[1].arguments,
        json!({ "source": "auto", "period": "day", "count": 60, "code": "SZ000034" })
    );
}

#[test]
fn surface_invocation_is_rejected_when_the_art_declares_no_actions() {
    let mut config = multi_call_config();
    config.surface_actions.clear();
    let mut request = request();
    request.inputs = json!({
        "surfaceAction": {
            "actionId": "stock_symbol_commit",
            "payload": { "value": "SZ000034" }
        }
    });
    request.params = json!({ "code": "SZ000034", "interval_seconds": 120 });
    request.disabled_params = Vec::new();

    let error = resolve_calls(&request, &config).unwrap_err();
    assert!(error.contains("not declared by this Art"), "{error}");
}

#[test]
fn dropped_undeclared_arguments_are_reported_by_name_without_values() {
    let mut request = request();
    request.inputs = json!({ "code": "SZ000034", "privatePayload": "do-not-record" });
    request.params = json!({ "interval_seconds": 120, "alsoUnknown": true });

    let warnings = dropped_argument_warnings(&request, &multi_call_config()).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].dropped_argument_count, 3);
    assert_eq!(
        warnings[0].dropped_argument_names,
        vec![
            "alsoUnknown".to_owned(),
            "interval_seconds".to_owned(),
            "privatePayload".to_owned()
        ]
    );
    let encoded = serde_json::to_string(&warnings).unwrap();
    assert!(!encoded.contains("do-not-record"));
    assert!(!encoded.contains("SZ000034"));
}
