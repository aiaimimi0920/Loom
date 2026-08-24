// Ordered multi-call success and failure retention.
#[test]
fn multi_call_outcomes_keep_successes_and_continue_after_a_failure() {
    let calls = vec![
        ResolvedCall {
            id: "first".to_owned(),
            tool_name: "first".to_owned(),
            arguments: json!({}),
        },
        ResolvedCall {
            id: "second".to_owned(),
            tool_name: "second".to_owned(),
            arguments: json!({}),
        },
        ResolvedCall {
            id: "third".to_owned(),
            tool_name: "third".to_owned(),
            arguments: json!({}),
        },
    ];
    let mut visited = Vec::new();

    let outcomes = collect_call_outcomes(&calls, |call| {
        visited.push(call.id.clone());
        if call.id == "second" {
            Err("second failed".to_owned())
        } else {
            Ok(json!({ "id": call.id }))
        }
    });

    assert_eq!(visited, vec!["first", "second", "third"]);
    assert!(matches!(outcomes[0], McpCallOutcome::Success(_)));
    assert!(matches!(outcomes[1], McpCallOutcome::Failure(ref error) if error == "second failed"));
    assert!(matches!(outcomes[2], McpCallOutcome::Success(_)));
}
