    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("loom-hook-canvas-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create Hook fixture root");
        root
    }

    fn write_session(root: &Path, json: &str) -> PathBuf {
        let session_dir = root.join("com.yamiyu.hook");
        fs::create_dir_all(session_dir.join("images")).expect("create Hook fixture dirs");
        let path = session_dir.join("session.json");
        fs::write(&path, json).expect("write Hook session fixture");
        path
    }

    // Percent-encode a filesystem path the way Tauri's asset protocol does so the
    // fixture matches the real `http://asset.localhost/<encoded>` shape Hook writes.
    fn encode_asset_url_path(path: &str) -> String {
        let mut encoded = String::new();
        for byte in path.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                    encoded.push(char::from(byte));
                }
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        encoded
    }

    #[test]
    fn normalizes_realistic_hook_session_into_canvas_snapshot() {
        let root = test_root("realistic");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"capture","type":"sticker","src":"images/capture.png","x":1816.0,"y":201.0,"w":500.0,"h":750.0},
                {"id":"small","type":"sticker","src":"images/missing.png","x":1792.0,"y":346.0,"w":60.0,"h":60.0},
                {"id":"art","type":"art","artId":"neuro.official/custom-image","src":"images/art.png","x":1576.0,"y":499.0,"w":60.0,"h":60.0}
              ],
              "links": [
                {"id":"edge-1","fromUnitId":"capture","fromPortId":"output_image","toUnitId":"art","toPortId":"input_image"}
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("capture.png"), b"capture").expect("write capture preview");
        fs::write(images.join("art.png"), b"art").expect("write art preview");

        let document = HookCanvasDocument::read(&session).expect("normalize Hook canvas");

        assert!(document.snapshot.available);
        assert_eq!(document.snapshot.nodes.len(), 3);
        assert_eq!(document.snapshot.edges.len(), 1);
        assert_eq!(document.snapshot.bounds.x, 1576.0);
        assert_eq!(document.snapshot.bounds.y, 201.0);
        assert_eq!(document.snapshot.bounds.width, 740.0);
        assert_eq!(document.snapshot.bounds.height, 750.0);
        let art = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "art")
            .expect("art node");
        assert_eq!(art.kind, HookCanvasNodeKind::Art);
        assert_eq!(art.art_id.as_deref(), Some("neuro.official/custom-image"));
        assert!(art.preview_available);
        assert!(art
            .preview_url
            .as_deref()
            .expect("art preview url")
            .starts_with("/v1/hook-bridge/canvas/nodes/art/preview?v="));
        let small = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "small")
            .expect("small node");
        assert!(!small.preview_available);
        assert!(small.preview_url.is_none());
        assert_eq!(document.snapshot.edges[0].source_node_id, "capture");
        assert_eq!(document.snapshot.edges[0].target_node_id, "art");
    }

    #[test]
    fn precomputes_world_edge_points_and_connected_component_ids() {
        let root = test_root("geometry");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"neuro.official/geometry-b","x":200,"y":0,"w":80,"h":80},
                {"id":"c","type":"art","artId":"neuro.official/geometry-c","x":400,"y":0,"w":80,"h":80},
                {"id":"mini","type":"sticker","x":0,"y":200,"w":80,"h":80,"minified":true}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"},
                {"id":"e2","fromUnitId":"b","toUnitId":"c"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize geometry");
        let node = |id: &str| {
            document
                .snapshot
                .nodes
                .iter()
                .find(|node| node.id == id)
                .expect("node present")
        };
        let edge = |id: &str| {
            document
                .snapshot
                .edges
                .iter()
                .find(|edge| edge.id == id)
                .expect("edge present")
        };

        let component = node("a").component_id.clone();
        assert_eq!(node("b").component_id, component);
        assert_eq!(node("c").component_id, component);
        assert_eq!(node("mini").component_id, "mini");

        assert_eq!(
            edge("e1").source_point,
            HookCanvasPoint { x: 86.0, y: 40.0 }
        );
        assert_eq!(
            edge("e1").target_point,
            HookCanvasPoint { x: 194.0, y: 40.0 }
        );
        assert_eq!(
            edge_port_points(node("mini"), node("b")).0,
            HookCanvasPoint { x: 84.0, y: 240.0 }
        );
    }

    #[test]
    fn precomputes_unique_workflow_export_metadata() {
        let root = test_root("workflow-export");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"capture","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"resize-a","type":"art","artId":"neuro.official/resize","x":200,"y":0,"w":80,"h":80},
                {"id":"resize-b","type":"art","artId":"neuro.official/resize","x":400,"y":0,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"capture","toUnitId":"resize-a"},
                {"id":"e2","fromUnitId":"resize-a","toUnitId":"resize-b"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize workflow export");
        let capture = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "capture")
            .expect("capture node");
        let resize_a = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "resize-a")
            .expect("resize-a node");
        let resize_b = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "resize-b")
            .expect("resize-b node");

        assert_eq!(capture.workflow_node_id, "capture");
        assert_eq!(resize_a.workflow_node_id, "resize");
        assert_eq!(resize_b.workflow_node_id, "resize-2");
        assert_eq!(capture.upstream_workflow_node_ids, Vec::<String>::new());
        assert_eq!(
            resize_a.upstream_workflow_node_ids,
            vec!["capture".to_string()]
        );
        assert_eq!(
            resize_b.upstream_workflow_node_ids,
            vec!["resize".to_string()]
        );
    }

    #[test]
    fn exports_selected_component_as_workflow_yaml() {
        let root = test_root("workflow-yaml");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"neuro.official/resize","x":200,"y":0,"w":80,"h":80},
                {"id":"c","type":"art","artId":"neuro.official/resize","x":400,"y":0,"w":80,"h":80},
                {"id":"lonely","type":"sticker","x":0,"y":200,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"},
                {"id":"e2","fromUnitId":"b","toUnitId":"c"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize workflow yaml");
        let yaml = document
            .export_workflow_yaml_for_selected_node("a", "hook-export")
            .expect("export workflow yaml");

        assert!(yaml.contains("name: 'hook-export'"));
        assert!(yaml.contains("- id: a"));
        assert!(yaml.contains("uses: '__sticker__'"));
        assert!(yaml.contains("uses: 'neuro.official/resize'"));
        assert!(yaml.contains("- id: resize"));
        assert!(yaml.contains("- id: resize-2"));
        assert!(yaml.contains("needs: [a]"));
        assert!(yaml.contains("needs: [resize]"));
        assert!(yaml.contains("image: '${{ nodes.a.outputs.output_image }}'"));
        assert!(yaml.contains("image: '${{ nodes.resize.outputs.output_image }}'"));
        assert!(!yaml.contains("lonely"));
    }

    #[test]
    fn exports_multi_image_edge_target_ports_into_workflow_yaml() {
        let root = test_root("workflow-yaml-multi-image");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"input","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"reference","type":"sticker","x":0,"y":200,"w":80,"h":80},
                {"id":"color","type":"art","artId":"neuro.official/color-transfer","x":200,"y":100,"w":80,"h":80},
                {"id":"compress","type":"art","artId":"neuro.official/compress","x":400,"y":100,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"input","fromPortId":"output","toUnitId":"color","toPortId":"input"},
                {"id":"e2","fromUnitId":"reference","fromPortId":"output_image","toUnitId":"color","toPortId":"reference"},
                {"id":"e3","fromUnitId":"color","fromPortId":"output","toUnitId":"compress","toPortId":"input"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize multi-image workflow");
        let yaml = document
            .export_workflow_yaml_for_selected_node("input", "color-compress")
            .expect("export multi-image workflow yaml");

        assert!(yaml.contains("needs: [input, reference]"));
        assert!(yaml.contains("input: '${{ nodes.input.outputs.output }}'"));
        assert!(yaml.contains("reference: '${{ nodes.reference.outputs.output_image }}'"));
        assert!(yaml.contains("input: '${{ nodes.color-transfer.outputs.output }}'"));
    }

    #[test]
    fn rejects_workflow_export_with_noncanonical_art_identity() {
        let root = test_root("workflow-yaml-quoting");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"resize:smart's","x":200,"y":0,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize workflow yaml");
        let error = document
            .export_workflow_yaml_for_selected_node("a", "Hook: Export's")
            .expect_err("unsafe Art identity must fail closed");

        assert!(matches!(
            error,
            HookCanvasWorkflowExportError::InvalidNode(node_id) if node_id == "b"
        ));
    }
