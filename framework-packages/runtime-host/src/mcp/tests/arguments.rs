// Default/caller argument merge and multi-call configuration fixtures.
#[test]
fn arguments_merge_defaults_inputs_and_params() {
    let arguments = build_arguments(
        &request(),
        &json!({ "query": "default", "safesearch": "strict" }),
    )
    .unwrap();
    assert_eq!(
        arguments,
        json!({
            "input": "from-input",
            "query": "loom",
            "count": "2",
            "safesearch": "strict"
        })
    );
}

fn multi_call_config() -> McpArtConfig {
    McpArtConfig {
        server_id: "stock-api".to_owned(),
        package_id: "neuro.official/stock-api".to_owned(),
        version: "=2.9.0".to_owned(),
        tool_name: None,
        arguments: json!({ "source": "auto" }),
        calls: vec![
            McpCallConfig {
                id: "quote".to_owned(),
                tool_name: "get_stock".to_owned(),
                arguments: Value::Null,
            },
            McpCallConfig {
                id: "history".to_owned(),
                tool_name: "get_klines".to_owned(),
                arguments: json!({ "period": "day", "count": 60 }),
            },
        ],
        surface_actions: BTreeMap::from([
            (
                "stock_refresh".to_owned(),
                McpSurfaceActionConfig {
                    calls: None,
                    arguments: BTreeMap::from([(
                        "code".to_owned(),
                        McpArgumentBinding {
                            from: vec!["authoritativeState.code".to_owned()],
                        },
                    )]),
                },
            ),
            (
                "stock_symbol_commit".to_owned(),
                McpSurfaceActionConfig {
                    calls: None,
                    arguments: BTreeMap::from([(
                        "code".to_owned(),
                        McpArgumentBinding {
                            from: vec![
                                "payload.value".to_owned(),
                                "authoritativeState.code".to_owned(),
                            ],
                        },
                    )]),
                },
            ),
            (
                "stock_interval_commit".to_owned(),
                McpSurfaceActionConfig {
                    calls: Some(Vec::new()),
                    arguments: BTreeMap::new(),
                },
            ),
        ]),
        argument_aliases: BTreeMap::new(),
    }
}
