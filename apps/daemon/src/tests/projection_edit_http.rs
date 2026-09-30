fn edit_change(id: &str, x: i32) -> Value {
    json!({"objectId":id,"value":{"id":id,"type":"rect","x":x,"y":0,"w":1,"h":1,"zIndex":0,
        "style":{"color":"#ffffff","width":1}}})
}
fn edit_session_id() -> String { format!("edit:{}", "1".repeat(32)) }

fn edit_apply(envelope: &ProjectionEnvelope, op: &str, base: u64, mode: u64, changes: Vec<Value>) -> Value {
    json!({"operation":"apply","projectionId":envelope.projection_id,"sessionId":edit_session_id(),"opId":op,
        "baseRevision":base,"modeRevision":mode,"changes":changes})
}

#[test]
fn projection_edit_shared_session_merges_objects_and_enforces_mode_epochs() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let mut source = Identity::pair(port, "Edit source");
    let mut first = Identity::pair(port, "Edit receiver one");
    let mut second = Identity::pair(port, "Edit receiver two");
    let outsider = Identity::pair(port, "Edit outsider");
    let snapshot = png(20);
    let a = source.invitation(&snapshot, unix_time_millis() + 270_000);
    let b = source.invitation(&snapshot, unix_time_millis() + 270_000);
    for (envelope, receiver) in [(&a, &first), (&b, &second)] {
        assert_eq!(source.post(port, "create", json!({"envelope":envelope,"snapshot":snapshot})).0, 200);
        assert_eq!(receiver.post(port, "accept", acceptance(envelope, "receiver-unit")).0, 200);
    }
    let session_id = edit_session_id();
    let init = json!({"operation":"initialize","projectionId":a.projection_id,"sessionId":session_id,
        "expectedDigest":a.content.digest,"objects":{"a":edit_change("a",0)["value"]}});
    assert_eq!(admin(port, "/v1/projections/edit", init.clone()).0, 403);
    assert_eq!(first.post(port, "edit", init.clone()).0, 403);
    assert_eq!(source.post(port, "edit", init.clone()).1["revision"], 1);
    let attach = json!({"operation":"attach","projectionId":b.projection_id,"sessionId":session_id});
    assert_eq!(source.post(port, "edit", attach).0, 200);
    let foreign = outsider.invitation(&snapshot, unix_time_millis() + 270_000);
    assert_eq!(outsider.post(port,"create",json!({"envelope":foreign,"snapshot":snapshot})).0,200);
    assert_eq!(outsider.post(port,"edit",json!({"operation":"attach","projectionId":foreign.projection_id,"sessionId":session_id})).0,403);
    let different = png(21); let other = source.invitation(&different,unix_time_millis()+270_000);
    assert_eq!(source.post(port,"create",json!({"envelope":other,"snapshot":different})).0,200);
    assert_eq!(error_code(&source.post(port,"edit",json!({"operation":"attach","projectionId":other.projection_id,"sessionId":session_id}))),"projection_edit_basis_conflict");
    assert_eq!(error_code(&source.post(port,"update",update(&b,1,&png(77)))),"projection_edit_snapshot_locked");
    let read = |envelope: &ProjectionEnvelope| json!({"operation":"read","projectionId":envelope.projection_id});
    assert_eq!(outsider.post(port, "edit", read(&a)).0, 403);
    assert_eq!(error_code(&first.post(port, "edit", edit_apply(&a,"early",1,1,vec![edit_change("a",1)]))), "projection_edit_read_only");
    let mode = |envelope: &ProjectionEnvelope, op: &str, base: u64, value: &str| json!({"operation":"mode",
        "projectionId":envelope.projection_id,"sessionId":edit_session_id(),"opId":op,"baseModeRevision":base,"mode":value});
    assert_eq!(first.post(port, "edit", mode(&a,"bad-mode",1,"two_way")).0, 403);
    assert_eq!(source.post(port, "edit", mode(&a,"enable",1,"two_way")).1["modeRevision"], 2);
    assert_eq!(second.post(port, "edit", read(&b)).1["mode"], "two_way");
    let one = edit_apply(&a,"first-edit",2,2,vec![edit_change("a",10)]);
    let two = edit_apply(&b,"second-edit",2,2,vec![edit_change("b",20)]);
    assert_eq!(first.post(port, "edit", one.clone()).1["revision"], 3);
    assert_eq!(second.post(port, "edit", two.clone()).1["revision"], 4);
    assert_eq!(first.post(port, "edit", one).1["revision"], 4);
    assert_eq!(second.post(port, "edit", two).1["revision"], 4);
    let conflict = first.post(port, "edit", edit_apply(&a,"conflict",2,2,vec![edit_change("c",1),edit_change("a",99)]));
    assert_eq!(error_code(&conflict), "projection_edit_object_conflict");
    let state = source.post(port, "edit", read(&a)).1;
    assert!(state["objects"].get("c").is_none());
    assert_eq!(state["objects"]["a"]["value"]["x"], 10);
    assert_eq!(state["objects"]["b"]["value"]["x"], 20);
    assert!(state.get("bindings").is_none());
    assert!(state.get("receipts").is_none());
    assert_eq!(source.post(port, "edit", mode(&b,"disable",2,"one_way")).1["modeRevision"], 5);
    assert_eq!(first.post(port, "edit", read(&a)).1["mode"], "one_way");
    assert_eq!(error_code(&first.post(port, "edit", edit_apply(&a,"late",4,2,vec![edit_change("a",100)]))), "projection_edit_mode_conflict");
    assert_eq!(error_code(&second.post(port, "edit", edit_apply(&b,"late-new-mode",5,5,vec![edit_change("b",100)]))), "projection_edit_read_only");
    assert_eq!(source.post(port, "edit", edit_apply(&b,"source-edit",4,5,vec![edit_change("b",30)])).1["revision"], 6);
    assert_eq!(error_code(&source.post(port, "edit", edit_apply(&a,"first-edit",6,5,vec![edit_change("a",40)]))), "projection_edit_operation_reused");
    assert_eq!(source.post(port, "edit", init.clone()).1["revision"], 6);
    let mut altered = init; altered["objects"]["a"] = edit_change("a",99)["value"].clone();
    assert_eq!(source.post(port, "edit", altered).0, 409);
    assert_eq!(source.post(port, "unlink", json!({"projectionId":a.projection_id})).0, 200);
    assert_eq!(first.post(port, "edit", read(&a)).0, 410);
    assert_eq!(second.post(port, "edit", read(&b)).1["revision"], 6);
    server.finish().unwrap();
    let mut server = restart_offline(&root.0, port);
    source.session(port); first.session(port); second.session(port);
    let restored = second.post(port, "edit", read(&b));
    assert_eq!(restored.0, 200);
    assert_eq!(restored.1["modeRevision"], 5);
    assert_eq!(restored.1["objects"]["b"]["value"]["x"], 30);
    assert_eq!(source.post(port, "edit", edit_apply(&b,"source-edit",4,5,vec![edit_change("b",30)])).1["revision"], 6);
    // Device disable/re-enable must not resurrect the old receiver epoch.
    for enabled in [false, true] {
        assert_eq!(response(http_request(port,"PUT",&format!("/v1/devices/{}",second.id),Some(&json!({
            "name":"Edit receiver two","kind":"computer","address":"127.0.0.1","enabled":enabled}).to_string()))).0,200);
    }
    second.session(port);
    assert_eq!(second.post(port,"edit",read(&b)).0,403);
    assert_eq!(source.post(port,"edit",read(&b)).0,200);
    server.finish().unwrap();
}

#[test]
fn projection_edit_concurrent_writes_and_tombstones_preserve_conflicts() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new(); let (port, mut server) = start(&root.0);
    let source = Identity::pair(port,"Concurrent source"); let receiver = Identity::pair(port,"Concurrent receiver");
    let snapshot = png(22); let envelope = source.invitation(&snapshot,unix_time_millis()+270_000);
    assert_eq!(source.post(port,"create",json!({"envelope":envelope,"snapshot":snapshot})).0,200);
    assert_eq!(receiver.post(port,"accept",acceptance(&envelope,"unit")).0,200);
    assert_eq!(source.post(port,"edit",json!({"operation":"initialize","projectionId":envelope.projection_id,
        "sessionId":edit_session_id(),"expectedDigest":envelope.content.digest,"objects":{}})).0,200);
    assert_eq!(source.post(port,"edit",json!({"operation":"mode","projectionId":envelope.projection_id,
        "sessionId":edit_session_id(),"opId":"enable","baseModeRevision":1,"mode":"two_way"})).0,200);
    let a=edit_apply(&envelope,"a",2,2,vec![edit_change("shared",1)]);
    let b=edit_apply(&envelope,"b",2,2,vec![edit_change("shared",2)]);
    let (left,right)=thread::scope(|scope| {
        let left=scope.spawn(||source.post(port,"edit",a.clone()));
        let right=scope.spawn(||receiver.post(port,"edit",b.clone()));
        (left.join().unwrap(),right.join().unwrap())
    });
    assert_eq!([left.0,right.0].iter().filter(|status|**status==200).count(),1);
    let retried=if left.0==200 {receiver.post(port,"edit",b)} else {source.post(port,"edit",a)};
    assert_eq!(error_code(&retried),"projection_edit_object_conflict");
    assert_eq!(source.post(port,"edit",edit_apply(&envelope,"delete",3,2,vec![json!({"objectId":"shared","value":null})])).1["revision"],4);
    assert_eq!(error_code(&receiver.post(port,"edit",edit_apply(&envelope,"resurrect",3,2,vec![edit_change("shared",3)]))),"projection_edit_object_conflict");
    let invalid=source.post(port,"edit",edit_apply(&envelope,"duplicate-id",4,2,vec![edit_change("new",1),edit_change("new",2)]));
    assert_eq!(invalid.0,400);
    let invalid=source.post(port,"edit",edit_apply(&envelope,"budget",4,2,vec![json!({"objectId":"large","value":{"id":"large","type":"text","text":"x".repeat(17000)}})]));
    assert_eq!(invalid.0,400);
    let mut stale=edit_apply(&envelope,"old-session",4,2,vec![edit_change("new",1)]);
    stale["sessionId"]=json!(format!("edit:{}","2".repeat(32)));
    assert_eq!(error_code(&source.post(port,"edit",stale)),"projection_edit_session_mismatch");
    server.finish().unwrap();
}
