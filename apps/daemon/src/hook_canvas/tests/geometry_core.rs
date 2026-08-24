    #[test]
    fn extracts_minified_crop_window_from_saved_rect_and_offset() {
        let root = test_root("minified-crop");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"mini",
                  "type":"sticker",
                  "src":"images/mini.png",
                  "x":2000.0,"y":-4.0,"w":100.0,"h":100.0,
                  "minified":true,
                  "savedRect":{"x":614.0,"y":1177.0,"w":461.0,"h":421.0},
                  "cropOffset":{"x":185.0,"y":72.0}
                },
                {
                  "id":"full",
                  "type":"sticker",
                  "src":"images/full.png",
                  "x":100.0,"y":100.0,"w":300.0,"h":200.0
                }
              ],
              "links": []
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("mini.png"), b"mini").expect("write mini preview");
        fs::write(images.join("full.png"), b"full").expect("write full preview");

        let document = HookCanvasDocument::read(&session).expect("normalize crop canvas");
        let mini = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "mini")
            .expect("mini node");
        assert!(mini.minified);
        let crop = mini.crop.as_ref().expect("crop window");
        // window is 100x100, savedRect 461x421, offset 185/72 → ratios to the box.
        assert_eq!(crop.image_width_ratio, 4.61);
        assert_eq!(crop.image_height_ratio, 4.21);
        assert_eq!(crop.offset_x_ratio, 1.85);
        assert_eq!(crop.offset_y_ratio, 0.72);

        let full = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "full")
            .expect("full node");
        assert!(!full.minified);
        assert!(full.crop.is_none());
    }

    #[test]
    fn invalid_geometry_and_dangling_edges_degrade_locally() {
        let root = test_root("invalid");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"capture","type":"sticker","x":"bad","y":-20,"w":0,"h":-1},
                {"type":"art","x":5,"y":5,"w":40,"h":40}
              ],
              "links": [
                {"id":"dangling","fromUnitId":"missing","toUnitId":"capture"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("degraded Hook canvas");

        assert_eq!(document.snapshot.nodes.len(), 1);
        assert_eq!(document.snapshot.nodes[0].x, 0.0);
        assert_eq!(document.snapshot.nodes[0].y, -20.0);
        assert_eq!(document.snapshot.nodes[0].width, DEFAULT_NODE_SIZE);
        assert_eq!(document.snapshot.nodes[0].height, DEFAULT_NODE_SIZE);
        assert!(document.snapshot.edges.is_empty());
        assert!(!document.snapshot.warnings.is_empty());
    }

    #[test]
    fn missing_session_returns_a_valid_empty_snapshot() {
        let root = test_root("missing");
        let document = HookCanvasDocument::read(&root.join("session.json"))
            .expect("missing session is a valid empty state");

        assert!(!document.snapshot.available);
        assert!(document.snapshot.nodes.is_empty());
        assert!(document.snapshot.edges.is_empty());
        assert_eq!(document.snapshot.revision, "missing");
        assert!(document.preview_roots().is_empty());
    }

    #[test]
    fn revision_changes_when_session_content_changes() {
        let root = test_root("revision");
        let session = write_session(&root, r#"{"stickers":[],"links":[]}"#);
        let first = HookCanvasDocument::read(&session).expect("first snapshot");
        fs::write(
            &session,
            r#"{"stickers":[{"id":"one","type":"sticker"}],"links":[]}"#,
        )
        .expect("rewrite session");
        let second = HookCanvasDocument::read(&session).expect("second snapshot");

        assert_eq!(first.snapshot.revision.len(), 16);
        assert_ne!(first.snapshot.revision, second.snapshot.revision);
    }

    #[test]
    fn revision_and_preview_url_change_when_image_is_updated_in_place() {
        let root = test_root("preview-version");
        let session = write_session(
            &root,
            r#"{"stickers":[{"id":"capture","type":"sticker","src":"images/capture.png"}],"links":[]}"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("capture.png"), b"first").expect("write first preview");
        let first = HookCanvasDocument::read(&session).expect("first snapshot");

        // Overwrite the same node's image in place. The session JSON, node id, and
        // file path are all unchanged; only the image bytes differ.
        fs::write(images.join("capture.png"), b"second image bytes")
            .expect("overwrite preview in place");
        let second = HookCanvasDocument::read(&session).expect("second snapshot");

        assert_ne!(
            first.snapshot.revision, second.snapshot.revision,
            "in-place image update must produce a new revision"
        );
        let first_url = first.snapshot.nodes[0]
            .preview_url
            .as_deref()
            .expect("first preview url");
        let second_url = second.snapshot.nodes[0]
            .preview_url
            .as_deref()
            .expect("second preview url");
        assert!(first_url.starts_with("/v1/hook-bridge/canvas/nodes/capture/preview?v="));
        assert_ne!(
            first_url, second_url,
            "in-place image update must bust the preview URL cache token"
        );
    }

    #[test]
    fn negative_coordinates_are_preserved_in_bounds() {
        let root = test_root("negative-bounds");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"left","type":"sticker","x":-120,"y":-40,"w":20,"h":30},
                {"id":"right","type":"sticker","x":80,"y":60,"w":40,"h":50}
              ],
              "links": []
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("negative bounds");

        assert_eq!(document.snapshot.bounds.x, -120.0);
        assert_eq!(document.snapshot.bounds.y, -40.0);
        assert_eq!(document.snapshot.bounds.width, 240.0);
        assert_eq!(document.snapshot.bounds.height, 150.0);
        assert_eq!(document.snapshot.nodes[0].width, MIN_NODE_SIZE);
    }

    #[test]
    fn classifies_only_canonical_art_and_sticker_nodes() {
        let root = test_root("classification");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"art-by-id","artId":"neuro.official/resize"},
                {"id":"capture","type":"capture"},
                {"id":"art","type":"art","artId":"neuro.official/resize"},
                {"id":"sticker","type":"sticker"},
                {"id":"unknown","type":"custom"}
              ],
              "links": []
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("classify nodes");
        let kinds = document
            .snapshot
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), &node.kind, node.label.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            vec![
                ("art", &HookCanvasNodeKind::Art, "Art 节点"),
                ("art-by-id", &HookCanvasNodeKind::Unknown, "未知节点"),
                ("capture", &HookCanvasNodeKind::Unknown, "未知节点"),
                ("sticker", &HookCanvasNodeKind::Screenshot, "截图节点"),
                ("unknown", &HookCanvasNodeKind::Unknown, "未知节点"),
            ]
        );
    }

    #[test]
    fn passes_through_node_params_for_parameter_exposure() {
        let root = test_root("node-params");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"art","type":"art","artId":"neuro.official/resize","params":{"width":512,"mode":"fit"}},
                {"id":"plain","type":"sticker"}
              ],
              "links": []
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("read params");
        let art = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "art")
            .expect("art node");
        assert_eq!(art.params["width"], 512);
        assert_eq!(art.params["mode"], "fit");
        let plain = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "plain")
            .expect("plain node");
        assert!(plain.params.is_null());
    }

    #[test]
    fn preview_paths_outside_the_session_image_root_are_not_registered() {
        let root = test_root("preview-boundary");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"escape","type":"sticker","src":"../outside.png"}
              ],
              "links": []
            }"#,
        );
        fs::write(root.join("outside.png"), b"outside").expect("write outside image");

        let document = HookCanvasDocument::read(&session).expect("normalize outside preview");
        let node = &document.snapshot.nodes[0];

        assert!(!node.preview_available);
        assert!(node.preview_url.is_none());
        assert!(document.preview_path("escape").is_none());
    }

    #[test]
    fn sticker_preview_uses_upstream_image_input_before_local_src() {
        let root = test_root("sticker-upstream-preview");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"upstream",
                  "type":"sticker",
                  "src":"images/upstream-square.png",
                  "x":0,"y":0,"w":100,"h":100
                },
                {
                  "id":"target",
                  "type":"sticker",
                  "src":"images/target-rect.png",
                  "x":200,"y":0,"w":200,"h":100
                }
              ],
              "links": [
                {
                  "id":"upstream-image",
                  "fromUnitId":"upstream",
                  "fromPortId":"output",
                  "toUnitId":"target",
                  "toPortId":"image"
                }
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("target-rect.png"), b"target").expect("write target");
        let expected_upstream =
            fs::canonicalize(images.join("upstream-square.png")).expect("canonical upstream");

        let document = HookCanvasDocument::read(&session).expect("normalize upstream preview");

        assert_eq!(
            document.preview_path("target"),
            Some(expected_upstream.as_path()),
            "sticker preview should mirror Hook and display the upstream image input instead of stretching the target's own src",
        );
    }

    #[test]
    fn disabled_sticker_image_input_falls_back_to_local_src() {
        let root = test_root("sticker-disabled-input-preview");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"upstream",
                  "type":"sticker",
                  "src":"images/upstream-square.png",
                  "x":0,"y":0,"w":100,"h":100
                },
                {
                  "id":"target",
                  "type":"sticker",
                  "src":"images/target-rect.png",
                  "x":200,"y":0,"w":200,"h":100,
                  "params":{"image":"__DISABLED__"}
                }
              ],
              "links": [
                {
                  "id":"upstream-image",
                  "fromUnitId":"upstream",
                  "fromPortId":"output",
                  "toUnitId":"target",
                  "toPortId":"image"
                }
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("target-rect.png"), b"target").expect("write target");
        let expected_target =
            fs::canonicalize(images.join("target-rect.png")).expect("canonical target");

        let document = HookCanvasDocument::read(&session).expect("normalize disabled preview");

        assert_eq!(
            document.preview_path("target"),
            Some(expected_target.as_path()),
            "when Hook disables the sticker image input, Loom must keep the node's own local preview",
        );
    }
