// JSON-pointer array/object mutation and malformed-index regression coverage.
#[test]
fn json_pointer_set_addresses_array_elements_instead_of_destroying_the_array() {
    let mut target = serde_json::json!({"props": {"items": ["a", "b", "c"]}});
    set_json_pointer(&mut target, "/props/items/1", serde_json::json!("B"))
        .expect("replace an array element");
    assert_eq!(target["props"]["items"], serde_json::json!(["a", "B", "c"]));

    set_json_pointer(&mut target, "/props/items/-", serde_json::json!("d"))
        .expect("append with the dash token");
    assert_eq!(
        target["props"]["items"],
        serde_json::json!(["a", "B", "c", "d"])
    );

    set_json_pointer(&mut target, "/props/items/4", serde_json::json!("e"))
        .expect("append at the end index");
    assert_eq!(
        target["props"]["items"],
        serde_json::json!(["a", "B", "c", "d", "e"])
    );

    let mut nested = serde_json::json!({"props": {"rows": [{"label": "one"}]}});
    set_json_pointer(&mut nested, "/props/rows/0/label", serde_json::json!("two"))
        .expect("traverse through an array element");
    assert_eq!(
        nested["props"]["rows"],
        serde_json::json!([{"label": "two"}])
    );
}

#[test]
fn json_pointer_set_rejects_bad_indexes_and_scalar_containers() {
    let mut target = serde_json::json!({"props": {"items": ["a"], "title": "hi"}});
    for path in [
        "/props/items/7",
        "/props/items/01",
        "/props/items/last",
        "/props/items/7/label",
        "/props/items/x/label",
        "/props/title/bold",
    ] {
        let error = set_json_pointer(&mut target, path, serde_json::json!(1))
            .expect_err("bad pointer must be rejected");
        assert!(
            matches!(error, SurfaceStoreError::Invalid(_)),
            "{path} produced {error:?}"
        );
    }
    assert_eq!(target["props"]["items"], serde_json::json!(["a"]));
    assert_eq!(target["props"]["title"], serde_json::json!("hi"));
}

#[test]
fn json_pointer_remove_deletes_array_elements_and_stays_a_no_op_when_absent() {
    let mut target = serde_json::json!({"props": {"items": ["a", "b", "c"]}});
    remove_json_pointer(&mut target, "/props/items/1").expect("remove an array element");
    assert_eq!(target["props"]["items"], serde_json::json!(["a", "c"]));

    remove_json_pointer(&mut target, "/props/items/9").expect("out of range is a no-op");
    remove_json_pointer(&mut target, "/props/missing/0").expect("missing branch is a no-op");
    remove_json_pointer(&mut target, "/props/items/5/label")
        .expect("missing array element on the way down is a no-op");
    assert_eq!(target["props"]["items"], serde_json::json!(["a", "c"]));

    let error = remove_json_pointer(&mut target, "/props/items/last")
        .expect_err("a non-index token on an array must be rejected");
    assert!(matches!(error, SurfaceStoreError::Invalid(_)));
    let error = remove_json_pointer(&mut target, "/props/items/0/label")
        .expect_err("traversing through a string must be rejected");
    assert!(matches!(error, SurfaceStoreError::Invalid(_)));
    assert_eq!(target["props"]["items"], serde_json::json!(["a", "c"]));
}

#[test]
fn node_patch_keeps_sibling_array_elements() {
    let mut root = SurfaceNode {
        id: "root".to_owned(),
        node_type: "column".to_owned(),
        props: serde_json::json!({"items": ["a", "b", "c"]}),
        ..SurfaceNode::default()
    };
    mutate_node_json(
        &mut root,
        "root",
        "/props/items/2",
        Some(serde_json::json!("C")),
    )
    .expect("patch an array element through the node encoder");
    assert_eq!(root.props["items"], serde_json::json!(["a", "b", "C"]));

    mutate_node_json(&mut root, "root", "/props/items/0", None)
        .expect("remove an array element through the node encoder");
    assert_eq!(root.props["items"], serde_json::json!(["b", "C"]));
}
