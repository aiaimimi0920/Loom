// Tool schema normalization, rejection, and alias-bound contracts.
#[test]
fn schema_normalizes_mcp_argument_types() {
    let aliases = BTreeMap::from([(
        "search_lang".to_owned(),
        BTreeMap::from([
            ("zh".to_owned(), "zh-hans".to_owned()),
            ("zh-cn".to_owned(), "zh-hans".to_owned()),
        ]),
    )]);
    let normalized = normalize_arguments(
        &json!({ "count": "2", "spellcheck": "true", "search_lang": "zh-cn" }),
        &json!({
            "properties": {
                "count": { "type": "integer" },
                "spellcheck": { "type": "boolean" },
                "search_lang": { "type": "string", "enum": ["en", "zh-hans"] }
            }
        }),
        &aliases,
    )
    .unwrap();
    assert_eq!(
        normalized,
        json!({ "count": 2, "spellcheck": true, "search_lang": "zh-hans" })
    );
}

#[test]
fn schema_excludes_arguments_rejected_by_the_mcp_tool() {
    let normalized = normalize_arguments(
        &json!({
            "query": "red panda",
            "count": "2",
            "result_index": 1,
            "__exec_manualTrigger": 123,
            "force_update": 456
        }),
        &json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "count": { "type": "integer" }
            },
            "additionalProperties": false
        }),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(normalized, json!({ "query": "red panda", "count": 2 }));
}

#[test]
fn schema_rejects_pattern_properties_instead_of_claiming_partial_support() {
    let error = normalize_arguments(
        &json!({ "header_x": "visible" }),
        &json!({
            "type": "object",
            "properties": {},
            "patternProperties": { "^header_": { "type": "string" } },
            "additionalProperties": false
        }),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(error.contains("patternProperties"));
}

#[test]
fn schema_enforces_required_and_rejects_nested_or_composed_features() {
    let error = normalize_arguments(
        &json!({ "optional": "present" }),
        &json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        }),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(error.contains("`query` is required"));

    for schema in [
        json!({ "type": "object", "allOf": [] }),
        json!({
            "type": "object",
            "properties": { "nested": { "type": "object" } }
        }),
    ] {
        assert!(normalize_arguments(&json!({}), &schema, &BTreeMap::new()).is_err());
    }
}

#[test]
fn argument_aliases_are_manifest_directed_and_bounded() {
    let aliases = BTreeMap::from([(
        "locale".to_owned(),
        BTreeMap::from([("zh-cn".to_owned(), "zh-Hans".to_owned())]),
    )]);
    validate_argument_aliases(&aliases).unwrap();
    let normalized = normalize_arguments(
        &json!({ "locale": "ZH-CN" }),
        &json!({
            "type": "object",
            "properties": { "locale": { "type": "string", "enum": ["en", "zh-Hans"] } }
        }),
        &aliases,
    )
    .unwrap();
    assert_eq!(normalized, json!({ "locale": "zh-Hans" }));

    let oversized = BTreeMap::from([(
        "locale".to_owned(),
        (0..33)
            .map(|index| (format!("alias-{index}"), "en".to_owned()))
            .collect(),
    )]);
    assert!(validate_argument_aliases(&oversized).is_err());
}
