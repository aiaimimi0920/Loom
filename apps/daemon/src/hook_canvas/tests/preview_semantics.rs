    #[test]
    fn error_art_preview_prefers_local_src_over_upstream_input() {
        let root = test_root("error-art-local-preview");
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
                  "id":"failed-art",
                  "type":"art",
                  "artId":"neuro.official/cloud-upscale",
                  "status":"error",
                  "src":"images/failed-art-error.png",
                  "x":200,"y":0,"w":200,"h":100
                }
              ],
              "links": [
                {
                  "id":"upstream-image",
                  "fromUnitId":"upstream",
                  "fromPortId":"output_image",
                  "toUnitId":"failed-art",
                  "toPortId":"input_image"
                }
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("failed-art-error.png"), b"art-error").expect("write art error");
        let expected_art =
            fs::canonicalize(images.join("failed-art-error.png")).expect("canonical art error");

        let document = HookCanvasDocument::read(&session).expect("normalize error art preview");

        assert_eq!(
            document.preview_path("failed-art"),
            Some(expected_art.as_path()),
            "when Hook stores a failed Art node's own local preview/error image, Loom must keep that preview instead of falling back to the upstream input image",
        );
    }

    #[test]
    fn error_art_preview_prefers_realistic_src_only_shape_over_upstream_input() {
        let root = test_root("error-art-realistic-shape");
        let session_dir = root.join("com.yamiyu.hook");
        let image_dir = session_dir.join("images");
        fs::create_dir_all(&image_dir).expect("create image dir");
        let upstream_path = fs::canonicalize({
            let path = image_dir.join("upstream.png");
            fs::write(&path, b"upstream").expect("write upstream");
            path
        })
        .expect("canonical upstream");
        let failed_art_path = fs::canonicalize({
            let path = image_dir.join("failed-art.png");
            fs::write(&path, b"art-error").expect("write art error");
            path
        })
        .expect("canonical art error");

        let session = write_session(
            &root,
            &format!(
                r#"{{
                  "workflowId":"hook-error-preview",
                  "stickers":[
                    {{
                      "id":"upstream",
                      "type":"sticker",
                      "src":"{upstream_src}",
                      "x":120,"y":80,"w":360,"h":210
                    }},
                    {{
                      "id":"failed-art",
                      "type":"art",
                      "artId":"neuro.official/custom-1770131241684",
                      "status":"error",
                      "src":"{failed_art_src}",
                      "x":600,"y":190,"w":190,"h":150,
                      "minified":true,
                      "opacityMini":0.9,
                      "opacityNormal":1.0,
                      "savedRect":{{"x":1508.0,"y":7.0,"w":500.0,"h":750.0}},
                      "cropOffset":{{"x":269.33333333333326,"y":384.33333333333326}},
                      "params":{{"reference":"upstream","strength":61}}
                    }}
                  ],
                  "links":[
                    {{
                      "id":"upstream-to-failed-art",
                      "fromUnitId":"upstream",
                      "fromPortId":"output",
                      "toUnitId":"failed-art",
                      "toPortId":"input"
                    }}
                  ]
                }}"#,
                upstream_src = upstream_path.to_string_lossy().replace('\\', "\\\\"),
                failed_art_src = failed_art_path.to_string_lossy().replace('\\', "\\\\"),
            ),
        );

        let document =
            HookCanvasDocument::read(&session).expect("normalize realistic error art preview");
        let failed_art = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "failed-art")
            .expect("failed-art node");

        assert_eq!(failed_art.status, "error");
        assert!(failed_art.minified);
        assert_eq!(
            document.preview_path("failed-art"),
            Some(failed_art_path.as_path()),
            "a realistic Hook Art-node shape that only carries local src must still keep its own failed preview instead of falling back to the upstream input image",
        );
    }

    #[test]
    fn sticker_preview_prefers_local_baked_preview_when_annotation_state_exists() {
        let root = test_root("sticker-local-baked-preview");
        let local_baked_preview = "data:image/png;base64,LOCAL_BAKED_PREVIEW";
        let session = write_session(
            &root,
            &format!(
                r##"{{
                  "stickers": [
                    {{
                      "id":"upstream",
                      "type":"sticker",
                      "src":"images/upstream-square.png",
                      "x":0,"y":0,"w":100,"h":100
                    }},
                    {{
                      "id":"target",
                      "type":"sticker",
                      "src":"images/target-rect.png",
                      "previewSrc":"{local_baked_preview}",
                      "annotationState": {{
                        "serialCounter": 1,
                        "elements": [
                          {{
                            "id":"line-b",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":20,"y":50}},{{"x":180,"y":50}}],
                            "style": {{"color":"#ffffff","width":2}}
                          }}
                        ]
                      }},
                      "x":200,"y":0,"w":200,"h":100
                    }}
                  ],
                  "links": [
                    {{
                      "id":"upstream-image",
                      "fromUnitId":"upstream",
                      "fromPortId":"output",
                      "toUnitId":"target",
                      "toPortId":"image"
                    }}
                  ]
                }}"##
            ),
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("target-rect.png"), b"target").expect("write target");

        let document = HookCanvasDocument::read(&session).expect("normalize local baked preview");

        assert_eq!(
            document.preview_source("target"),
            Some(&HookCanvasPreviewSource::DataUrl(local_baked_preview.to_owned())),
            "a sticker with persisted annotation state must prefer its own baked preview over the raw upstream image input",
        );
    }

    #[test]
    fn sticker_preview_prefers_local_baked_preview_through_detached_chain() {
        let root = test_root("sticker-detached-chain-preview");
        let baked_preview_b = "data:image/png;base64,LOCAL_B";
        let baked_preview_c = "data:image/png;base64,LOCAL_C";
        let session = write_session(
            &root,
            &format!(
                r##"{{
                  "stickers": [
                    {{
                      "id":"a",
                      "type":"sticker",
                      "src":"images/a.png",
                      "annotationState": {{
                        "serialCounter": 1,
                        "elements": [
                          {{
                            "id":"line-a",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":50,"y":0}},{{"x":50,"y":100}}],
                            "style": {{"color":"#ffffff","width":2}}
                          }}
                        ]
                      }},
                      "x":0,"y":0,"w":100,"h":100
                    }},
                    {{
                      "id":"b",
                      "type":"sticker",
                      "src":"images/b.png",
                      "previewSrc":"{baked_preview_b}",
                      "annotationState": {{
                        "serialCounter": 2,
                        "elements": [
                          {{
                            "id":"line-b",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":20,"y":50}},{{"x":180,"y":50}}],
                            "style": {{"color":"#ffffff","width":2}}
                          }}
                        ]
                      }},
                      "x":120,"y":0,"w":200,"h":100
                    }},
                    {{
                      "id":"c",
                      "type":"sticker",
                      "src":"images/c.png",
                      "previewSrc":"{baked_preview_c}",
                      "annotationState": {{
                        "serialCounter": 2,
                        "elements": [
                          {{
                            "id":"line-b",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":40,"y":100}},{{"x":360,"y":100}}],
                            "style": {{"color":"#ffffff","width":4}}
                          }}
                        ]
                      }},
                      "x":360,"y":0,"w":400,"h":200
                    }}
                  ],
                  "links": [
                    {{
                      "id":"a-b",
                      "fromUnitId":"a",
                      "fromPortId":"output",
                      "toUnitId":"b",
                      "toPortId":"image"
                    }},
                    {{
                      "id":"b-c",
                      "fromUnitId":"b",
                      "fromPortId":"output",
                      "toUnitId":"c",
                      "toPortId":"image"
                    }}
                  ]
                }}"##
            ),
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("a.png"), b"a").expect("write a");
        fs::write(images.join("b.png"), b"b").expect("write b");
        fs::write(images.join("c.png"), b"c").expect("write c");

        let document =
            HookCanvasDocument::read(&session).expect("normalize detached chain preview");

        assert_eq!(
            document.preview_source("c"),
            Some(&HookCanvasPreviewSource::DataUrl(baked_preview_c.to_owned())),
            "when a downstream sticker already carries its own baked propagated preview, Loom must not recurse all the way back to ancestor raw inputs",
        );
    }

    #[test]
    fn resolves_tauri_asset_url_previews_from_the_clipboard_cache_root() {
        let root = test_root("asset-url-clipboard-cache");
        let cache = root.join("clipboard_cache");
        fs::create_dir_all(&cache).expect("create clipboard cache");
        let image_path = cache.join("Hook_capture_1.png");
        fs::write(&image_path, b"capture-bytes").expect("write cache image");

        // Point the daemon's clipboard-cache root at the isolated fixture dir.
        let previous = std::env::var_os("LOOM_HOOK_IMAGE_ROOT");
        std::env::set_var("LOOM_HOOK_IMAGE_ROOT", &cache);

        // Hook writes the image as a percent-encoded Tauri asset URL in `src`
        // and the clean absolute path in `filePath`.
        let canonical_cache = fs::canonicalize(&cache).expect("canonicalize cache");
        let canonical_image = canonical_cache.join("Hook_capture_1.png");
        let encoded = encode_asset_url_path(&canonical_image.to_string_lossy());
        let session = write_session(
            &root,
            &format!(
                r#"{{
                  "stickers": [
                    {{
                      "id":"capture",
                      "type":"sticker",
                      "src":"http://asset.localhost/{encoded}",
                      "filePath":"{file_path}",
                      "x":10,"y":20,"w":320,"h":180
                    }}
                  ],
                  "links": []
                }}"#,
                file_path = canonical_image.to_string_lossy().replace('\\', "\\\\"),
            ),
        );

        let document = HookCanvasDocument::read(&session).expect("normalize asset url preview");
        let node = &document.snapshot.nodes[0];

        assert!(
            node.preview_available,
            "asset-url preview from clipboard_cache should resolve"
        );
        assert!(node
            .preview_url
            .as_deref()
            .expect("preview url")
            .starts_with("/v1/hook-bridge/canvas/nodes/capture/preview?v="));
        assert_eq!(
            document.preview_path("capture"),
            Some(canonical_image.as_path())
        );

        if let Some(previous) = previous {
            std::env::set_var("LOOM_HOOK_IMAGE_ROOT", previous);
        } else {
            std::env::remove_var("LOOM_HOOK_IMAGE_ROOT");
        }
    }

    #[test]
    fn data_url_limit_is_checked_from_encoded_length_before_decode() {
        assert!(preview_data_url_is_within_limit_for(
            "data:image/png;base64,AAAA",
            3,
        ));
        assert!(!preview_data_url_is_within_limit_for(
            "data:image/png;base64,AAAA",
            2,
        ));
        assert!(preview_data_url_is_within_limit_for(
            "data:image/png;base64,AAA=",
            2,
        ));
        assert!(!preview_data_url_is_within_limit_for(
            "data:image/png;base64,AAAAA",
            20,
        ));

        let oversized_header = format!("data:image/{};base64,AA", "x".repeat(128));
        assert!(!preview_data_url_is_within_limit_for(
            &oversized_header,
            3,
        ));
    }

    #[test]
    fn connected_image_input_index_preserves_first_edge_precedence() {
        let links = vec![
            HookCanvasSessionLink {
                from_unit_id: "first".to_owned(),
                to_unit_id: "target".to_owned(),
                to_port_id: Some("image".to_owned()),
            },
            HookCanvasSessionLink {
                from_unit_id: "second".to_owned(),
                to_unit_id: "target".to_owned(),
                to_port_id: Some("input_image".to_owned()),
            },
        ];

        let index = connected_image_inputs(&links);

        assert_eq!(index.get("target").copied(), Some("first"));
    }

    #[test]
    fn preview_chain_depth_is_bounded_without_poisoning_shallower_cache_entries() {
        let mut raw_nodes = HashMap::new();
        raw_nodes.insert(
            "node-0".to_owned(),
            serde_json::json!({
                "id": "node-0",
                "previewSrc": "data:image/png;base64,AAAA"
            }),
        );
        let mut links = Vec::new();
        for index in 1..=MAX_PREVIEW_CHAIN_DEPTH + 1 {
            let node_id = format!("node-{index}");
            raw_nodes.insert(node_id.clone(), serde_json::json!({ "id": node_id }));
            links.push(HookCanvasSessionLink {
                from_unit_id: format!("node-{}", index - 1),
                to_unit_id: format!("node-{index}"),
                to_port_id: Some("image".to_owned()),
            });
        }
        let image_inputs = connected_image_inputs(&links);
        let mut cache = HashMap::new();

        let limited = resolve_effective_preview_source(
            &format!("node-{}", MAX_PREVIEW_CHAIN_DEPTH + 1),
            &raw_nodes,
            &image_inputs,
            Path::new("."),
            &[],
            &mut cache,
            &mut HashSet::new(),
            0,
        );
        assert!(limited.source.is_none());
        assert!(limited.depth_limited);

        let within_limit = resolve_effective_preview_source(
            &format!("node-{MAX_PREVIEW_CHAIN_DEPTH}"),
            &raw_nodes,
            &image_inputs,
            Path::new("."),
            &[],
            &mut cache,
            &mut HashSet::new(),
            0,
        );
        assert!(within_limit.source.is_some());
        assert!(!within_limit.depth_limited);
    }
