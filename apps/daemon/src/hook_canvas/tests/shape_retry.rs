    #[test]
    fn accepts_current_hook_workflow_sync_shape() {
        let root = test_root("nested");
        let session = write_session(
            &root,
            r#"{
              "workflowId": "hook-live",
              "nodes": [
                {
                  "id":"nested",
                  "type":"artNode",
                  "position":{"x":12,"y":24},
                  "measured":{"width":320,"height":180},
                  "data":{"artId":"neuro.official/ocr","previewSrc":"images/nested.png","status":"processing"}
                }
              ],
              "edges": [
                {"id":"self","source":"nested","target":"nested","sourceHandle":"out","targetHandle":"in"}
              ]
            }"#,
        );
        fs::write(
            session
                .parent()
                .expect("session parent")
                .join("images")
                .join("nested.png"),
            b"nested",
        )
        .expect("write nested preview");

        let bytes = fs::read(&session).expect("read workflow sync fixture");
        let root_value = serde_json::from_slice(&bytes).expect("parse workflow sync fixture");
        let document = HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);
        let node = &document.snapshot.nodes[0];

        assert_eq!(document.snapshot.workflow_id.as_deref(), Some("hook-live"));
        assert_eq!(node.x, 12.0);
        assert_eq!(node.y, 24.0);
        assert_eq!(node.width, 320.0);
        assert_eq!(node.height, 180.0);
        assert_eq!(node.kind, HookCanvasNodeKind::Art);
        assert_eq!(node.status, "processing");
        assert_eq!(document.snapshot.edges.len(), 1);
        assert_eq!(
            document.snapshot.edges[0].source_port_id.as_deref(),
            Some("out")
        );
        assert_eq!(
            document.snapshot.edges[0].target_port_id.as_deref(),
            Some("in")
        );
    }

    #[test]
    fn hybrid_canvas_shape_is_rejected() {
        let root = test_root("hybrid-session");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"session-node","type":"sticker","x":4,"y":8,"w":320,"h":180}
              ],
              "nodes": [
                {"id":"wire-node","type":"artNode","position":{"x":99,"y":99},"measured":{"width":1,"height":1}}
              ],
              "links": [
                {"id":"alias","source":"session-node","target":"session-node","sourceHandle":"out","targetHandle":"in"}
              ],
              "edges": [
                {"id":"wire","source":"wire-node","target":"wire-node"}
              ]
            }"#,
        );
        let bytes = fs::read(&session).expect("read hybrid session fixture");
        let root_value = serde_json::from_slice(&bytes).expect("parse hybrid session fixture");
        let document = HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);

        assert!(document.snapshot.nodes.is_empty());
        assert!(document.snapshot.edges.is_empty());
        assert!(document
            .snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("exactly one canonical shape")));
    }

    #[test]
    fn incomplete_or_non_array_canvas_containers_are_rejected() {
        for (name, contents) in [
            ("missing-links", r#"{"stickers":[]}"#),
            ("null-links", r#"{"stickers":[],"links":null}"#),
            ("missing-edges", r#"{"nodes":[]}"#),
            ("object-edges", r#"{"nodes":[],"edges":{}}"#),
        ] {
            let root = test_root(name);
            let session = write_session(&root, contents);
            let bytes = fs::read(&session).expect("read malformed canvas fixture");
            let root_value = serde_json::from_slice(&bytes).expect("parse canvas fixture");
            let document =
                HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);
            assert!(document.snapshot.nodes.is_empty(), "{name}");
            assert!(document.snapshot.edges.is_empty(), "{name}");
            assert!(document
                .snapshot
                .warnings
                .iter()
                .any(|warning| warning.contains("exactly one canonical shape")));
        }
    }

    #[test]
    fn workflow_shape_ignores_session_endpoint_aliases() {
        let root = test_root("hybrid-workflow");
        let session = write_session(
            &root,
            r#"{
              "nodes": [
                {"id":"wire-node","type":"artNode","position":{"x":12,"y":24},"measured":{"width":32,"height":48}}
              ],
              "edges": [
                {"id":"alias","fromUnitId":"wire-node","toUnitId":"wire-node","fromPortId":"out","toPortId":"in"}
              ]
            }"#,
        );
        let bytes = fs::read(&session).expect("read hybrid workflow fixture");
        let root_value = serde_json::from_slice(&bytes).expect("parse hybrid workflow fixture");
        let document = HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);

        assert_eq!(document.snapshot.nodes.len(), 1);
        assert_eq!(document.snapshot.nodes[0].id, "wire-node");
        assert_eq!(document.snapshot.nodes[0].x, 12.0);
        assert_eq!(document.snapshot.edges.len(), 0);
    }

    #[test]
    fn retries_a_transient_partial_session_write() {
        let mut reads = VecDeque::from([
            b"{\"stickers\":[".to_vec(),
            br#"{"stickers":[{"id":"ready","type":"sticker"}],"links":[]}"#.to_vec(),
        ]);
        let mut waits = 0;

        let (_, root) = read_session_value_with(
            || Ok(reads.pop_front().expect("session read fixture")),
            || waits += 1,
        )
        .expect("retry partial Hook session")
        .expect("session remains available");

        assert_eq!(waits, 1);
        let source = hook_canvas_source(&root);
        assert_eq!(canvas_nodes(&root, source).len(), 1);
        assert_eq!(canvas_nodes(&root, source)[0]["id"], "ready");
    }

    #[test]
    fn malformed_session_is_reported_as_json_error() {
        let root = test_root("malformed");
        let session = write_session(&root, "{not-json");

        let error = HookCanvasDocument::read(&session).expect_err("malformed session must fail");

        assert!(matches!(error, HookCanvasError::Json(_)));
    }

    #[test]
    fn oversized_session_is_rejected_before_parse_or_retry() {
        let mut reads = 0;
        let mut waits = 0;

        let error = read_session_value_with_limits(
            || {
                reads += 1;
                Ok(br#"{}"#.to_vec())
            },
            || waits += 1,
            1,
            MAX_HOOK_SESSION_DEPTH,
        )
        .expect_err("oversized Hook session must fail");

        assert!(matches!(error, HookCanvasError::Limit(_)));
        assert_eq!(reads, 1);
        assert_eq!(waits, 0);
    }

    #[test]
    fn deeply_nested_session_is_rejected_without_retry() {
        let mut waits = 0;

        let error = read_session_value_with_limits(
            || Ok(br#"{"a":{"b":1}}"#.to_vec()),
            || waits += 1,
            1024,
            1,
        )
        .expect_err("deep Hook session must fail");

        assert!(matches!(error, HookCanvasError::Limit(_)));
        assert_eq!(waits, 0);
    }
