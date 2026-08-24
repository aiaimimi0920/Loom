impl HookCanvasDocument {
    pub(crate) fn read(session_path: &Path) -> Result<Self, HookCanvasError> {
        let Some((bytes, root)) = read_session_value(session_path)? else {
            return Ok(Self::missing());
        };
        Ok(Self::from_serialized_root(
            session_path,
            bytes,
            root,
            modified_at_millis(session_path),
        ))
    }

    pub(crate) fn from_serialized_root(
        source_path: &Path,
        bytes: Vec<u8>,
        root: Value,
        updated_at: Option<String>,
    ) -> Self {
        let session_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
        let preview_roots = canonical_preview_roots(session_dir);
        let mut warnings = Vec::new();
        let mut preview_sources = HashMap::new();
        let mut preview_versions = Vec::new();
        let mut node_ids = HashSet::new();
        let mut raw_nodes = HashMap::new();
        let mut nodes = Vec::new();

        let canvas_source = hook_canvas_source(&root);
        if matches!(canvas_source, HookCanvasSource::Invalid) {
            warnings.push(
                "Hook canvas must contain exactly one canonical shape: stickers/links or nodes/edges."
                    .to_owned(),
            );
        }
        for raw_node in canvas_nodes(&root, canvas_source) {
            let Some(id) = non_empty_string(raw_node.get("id")) else {
                warnings.push("已跳过缺少有效 ID 的 Hook 节点。".to_owned());
                continue;
            };
            if !node_ids.insert(id.clone()) {
                warnings.push(format!("已跳过重复的 Hook 节点 `{id}`。"));
                continue;
            }

            let (x, x_degraded) =
                normalized_coordinate(node_coordinate(raw_node, "x", canvas_source));
            let (y, y_degraded) =
                normalized_coordinate(node_coordinate(raw_node, "y", canvas_source));
            let (width, width_degraded) =
                normalized_size(node_size(raw_node, "w", "width", canvas_source));
            let (height, height_degraded) =
                normalized_size(node_size(raw_node, "h", "height", canvas_source));
            if x_degraded || y_degraded || width_degraded || height_degraded {
                warnings.push(format!("Hook 节点 `{id}` 的几何信息已归一化。"));
            }

            let raw_art_id = node_string(raw_node, "artId");
            let node_type = node_type(raw_node);
            let kind = classify_node(node_type.as_deref(), raw_art_id.as_deref());
            let art_id = matches!(kind, HookCanvasNodeKind::Art)
                .then_some(raw_art_id)
                .flatten();
            let label = match kind {
                HookCanvasNodeKind::Screenshot => "截图节点",
                HookCanvasNodeKind::Art => "Art 节点",
                HookCanvasNodeKind::Unknown => "未知节点",
            }
            .to_owned();
            let status =
                normalized_status(node_string(raw_node, "status").as_deref(), &kind).to_owned();
            let error_message =
                node_string(raw_node, "errorMessage").or_else(|| node_string(raw_node, "error"));

            let minified = node_value(raw_node, "minified")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let crop = if minified {
                extract_crop(raw_node, width, height)
            } else {
                None
            };
            // Hook applies opacity at render time (not baked into the image):
            // opacityMini when minified, opacityNormal otherwise.
            let opacity_key = if minified {
                "opacityMini"
            } else {
                "opacityNormal"
            };
            let opacity = value_as_f64(node_value(raw_node, opacity_key))
                .map(|value| value.clamp(0.0, 1.0))
                .unwrap_or(1.0);
            let params = node_value(raw_node, "params")
                .cloned()
                .unwrap_or(Value::Null);
            let result_candidates = node_result_candidates(raw_node);
            let selected_result_index = node_selected_result_index(raw_node, &params);

            raw_nodes.insert(id.clone(), raw_node.clone());

            nodes.push(HookCanvasNode {
                id,
                component_id: String::new(),
                workflow_node_id: String::new(),
                upstream_workflow_node_ids: Vec::new(),
                kind,
                label,
                art_id,
                x,
                y,
                width,
                height,
                preview_available: false,
                preview_url: None,
                status,
                error_message,
                minified,
                crop,
                opacity,
                params,
                result_candidates,
                selected_result_index,
            });
        }
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        let node_lookup = nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<HashMap<_, _>>();

        let mut edges = Vec::new();
        let mut session_links = Vec::new();
        for (index, raw_edge) in canvas_edges(&root, canvas_source).iter().enumerate() {
            let source_node_id = edge_endpoint(raw_edge, canvas_source, EdgeEnd::Source);
            let target_node_id = edge_endpoint(raw_edge, canvas_source, EdgeEnd::Target);
            let (Some(source_node_id), Some(target_node_id)) = (source_node_id, target_node_id)
            else {
                warnings.push("已跳过缺少端点的 Hook 连线。".to_owned());
                continue;
            };
            let (Some(source_node), Some(target_node)) = (
                node_lookup.get(source_node_id.as_str()),
                node_lookup.get(target_node_id.as_str()),
            ) else {
                warnings.push(format!(
                    "已跳过端点不存在的 Hook 连线 `{source_node_id}` -> `{target_node_id}`。"
                ));
                continue;
            };
            let id =
                non_empty_string(raw_edge.get("id")).unwrap_or_else(|| format!("edge-{index:04}"));
            let target_port_id = edge_port(raw_edge, canvas_source, EdgeEnd::Target);
            session_links.push(HookCanvasSessionLink {
                from_unit_id: source_node_id.clone(),
                to_unit_id: target_node_id.clone(),
                to_port_id: target_port_id.clone(),
            });
            let (source_point, target_point) = edge_port_points(source_node, target_node);
            edges.push(HookCanvasEdge {
                id,
                source_node_id,
                source_port_id: edge_port(raw_edge, canvas_source, EdgeEnd::Source),
                source_point,
                target_node_id,
                target_port_id,
                target_point,
            });
        }
        edges.sort_by(|left, right| left.id.cmp(&right.id));

        let mut preview_cache = HashMap::new();
        let image_inputs = connected_image_inputs(&session_links);
        for node in &mut nodes {
            let resolved = resolve_effective_preview_source(
                node.id.as_str(),
                &raw_nodes,
                &image_inputs,
                session_dir,
                &preview_roots,
                &mut preview_cache,
                &mut HashSet::new(),
                0,
            );
            if resolved.source.is_none() && resolved.had_candidates {
                warnings.push(format!("Hook 节点 `{}` 的预览不可用。", node.id));
            }
            let preview_version = resolved.source.as_ref().map(preview_source_version);
            if let Some(source) = resolved.source {
                preview_sources.insert(node.id.clone(), source);
                node.preview_available = true;
            }
            if let Some(version) = preview_version.as_deref() {
                preview_versions.push(format!("{}:{version}", node.id));
            }
            if node.preview_available {
                let base = format!(
                    "/v1/hook-bridge/canvas/nodes/{}/preview",
                    encode_path_segment(&node.id)
                );
                node.preview_url = Some(match preview_version.as_deref() {
                    Some(version) => format!("{base}?v={version}"),
                    None => base,
                });
            }
        }

        let component_ids = component_ids_for(&nodes, &edges);
        let (workflow_node_ids, mut upstream_workflow_node_ids) =
            workflow_export_metadata_for(&nodes, &edges);
        for node in &mut nodes {
            node.component_id = component_ids
                .get(node.id.as_str())
                .cloned()
                .unwrap_or_else(|| node.id.clone());
            node.workflow_node_id = workflow_node_ids
                .get(node.id.as_str())
                .cloned()
                .unwrap_or_else(|| sanitize_workflow_node_id(&node.id));
            node.upstream_workflow_node_ids = upstream_workflow_node_ids
                .remove(node.id.as_str())
                .unwrap_or_default();
        }

        let snapshot = HookCanvasSnapshot {
            available: true,
            revision: revision_for(&bytes, &preview_versions),
            updated_at,
            workflow_id: non_empty_string(root.get("workflowId")),
            bounds: canvas_bounds(&nodes),
            nodes,
            edges,
            warnings,
        };
        Self {
            snapshot,
            preview_sources,
            preview_roots,
        }
    }

    #[cfg(test)]
    pub(crate) fn preview_path(&self, node_id: &str) -> Option<&Path> {
        match self.preview_sources.get(node_id) {
            Some(HookCanvasPreviewSource::File(path)) => Some(path.as_path()),
            _ => None,
        }
    }

    pub(crate) fn preview_source(&self, node_id: &str) -> Option<&HookCanvasPreviewSource> {
        self.preview_sources.get(node_id)
    }

    pub(crate) fn override_preview_source(
        &mut self,
        node_id: &str,
        source: HookCanvasPreviewSource,
        cache_token: Option<&str>,
    ) {
        if !preview_source_is_within_limit(&source) {
            return;
        }
        self.preview_sources.insert(node_id.to_owned(), source);
        let Some(node) = self
            .snapshot
            .nodes
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        node.preview_available = true;
        let base = format!(
            "/v1/hook-bridge/canvas/nodes/{}/preview",
            encode_path_segment(node_id)
        );
        node.preview_url = Some(match cache_token {
            Some(token) if !token.trim().is_empty() => format!("{base}?v={token}"),
            _ => base,
        });
    }

    pub(crate) fn preview_roots(&self) -> &[PathBuf] {
        &self.preview_roots
    }

    pub(crate) fn export_workflow_yaml_for_selected_node(
        &self,
        selected_node_id: &str,
        workflow_name: &str,
    ) -> Result<String, HookCanvasWorkflowExportError> {
        let Some(selected_node) = self
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == selected_node_id)
        else {
            return Err(HookCanvasWorkflowExportError::NodeNotFound(
                selected_node_id.to_owned(),
            ));
        };

        let members = self
            .snapshot
            .nodes
            .iter()
            .filter(|node| node.component_id == selected_node.component_id)
            .collect::<Vec<_>>();
        let safe_name = if workflow_name.trim().is_empty() {
            "hook-pipeline"
        } else {
            workflow_name.trim()
        };
        let mut lines = vec![
            format!("name: {}", yaml_single_quoted(safe_name)),
            "nodes:".to_owned(),
        ];
        if members.is_empty() {
            lines.push("  []".to_owned());
            return Ok(format!("{}\n", lines.join("\n")));
        }

        let selected_workflow_ids = members
            .iter()
            .map(|node| node.workflow_node_id.clone())
            .collect::<HashSet<_>>();
        let workflow_ids_by_raw_node = members
            .iter()
            .map(|node| (node.id.as_str(), node.workflow_node_id.as_str()))
            .collect::<HashMap<_, _>>();
        for node in members {
            lines.push(format!("  - id: {}", node.workflow_node_id));
            let uses = match &node.kind {
                HookCanvasNodeKind::Screenshot => STICKER_WORKFLOW_USES,
                HookCanvasNodeKind::Art => node
                    .art_id
                    .as_deref()
                    .ok_or_else(|| HookCanvasWorkflowExportError::InvalidNode(node.id.clone()))?,
                HookCanvasNodeKind::Unknown => {
                    return Err(HookCanvasWorkflowExportError::InvalidNode(node.id.clone()));
                }
            };
            lines.push(format!("    uses: {}", yaml_single_quoted(uses)));
            let needs = node
                .upstream_workflow_node_ids
                .iter()
                .filter(|id| selected_workflow_ids.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            if !needs.is_empty() {
                lines.push(format!("    needs: [{}]", needs.join(", ")));
            }
            let mut seen_target_ports = HashSet::new();
            let incoming_edges = self
                .snapshot
                .edges
                .iter()
                .filter(|edge| edge.target_node_id == node.id)
                .filter_map(|edge| {
                    let source_node_id = workflow_ids_by_raw_node
                        .get(edge.source_node_id.as_str())?
                        .to_string();
                    let source_port_id = edge
                        .source_port_id
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("output_image")
                        .to_owned();
                    let target_port_id = edge
                        .target_port_id
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("image")
                        .to_owned();
                    if !seen_target_ports.insert(target_port_id.clone()) {
                        return None;
                    }
                    Some((source_node_id, source_port_id, target_port_id))
                })
                .collect::<Vec<_>>();
            if !incoming_edges.is_empty() {
                lines.push("    with:".to_owned());
                for (source_node_id, source_port_id, target_port_id) in incoming_edges {
                    let reference =
                        format!("${{{{ nodes.{source_node_id}.outputs.{source_port_id} }}}}");
                    lines.push(format!(
                        "      {}: {}",
                        yaml_mapping_key(&target_port_id),
                        yaml_single_quoted(&reference)
                    ));
                }
            }
        }

        Ok(format!("{}\n", lines.join("\n")))
    }

    // Build a frozen snapshot scoped to the selected node's connected component,
    // plus each member node's current preview source. The snapshot keeps node
    // geometry/crop and the in-component edges, so it renders identically to the
    // live canvas. The caller persists the images and rewrites each node's
    // `preview_url` to point at the saved-workflow preview route.
    pub(crate) fn component_snapshot_for_selected_node(
        &self,
        selected_node_id: &str,
    ) -> Result<HookCanvasComponentSnapshot, HookCanvasWorkflowExportError> {
        let Some(selected_node) = self
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == selected_node_id)
        else {
            return Err(HookCanvasWorkflowExportError::NodeNotFound(
                selected_node_id.to_owned(),
            ));
        };
        let component_id = selected_node.component_id.clone();

        let nodes: Vec<HookCanvasNode> = self
            .snapshot
            .nodes
            .iter()
            .filter(|node| node.component_id == component_id)
            .cloned()
            .collect();
        let member_ids: HashSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
        let edges: Vec<HookCanvasEdge> = self
            .snapshot
            .edges
            .iter()
            .filter(|edge| {
                member_ids.contains(&edge.source_node_id)
                    && member_ids.contains(&edge.target_node_id)
            })
            .cloned()
            .collect();

        let previews: Vec<(String, HookCanvasPreviewSource)> = nodes
            .iter()
            .filter_map(|node| {
                self.preview_sources
                    .get(&node.id)
                    .map(|source| (node.id.clone(), source.clone()))
            })
            .collect();

        let snapshot = HookCanvasSnapshot {
            available: true,
            revision: self.snapshot.revision.clone(),
            updated_at: self.snapshot.updated_at.clone(),
            workflow_id: self.snapshot.workflow_id.clone(),
            bounds: canvas_bounds(&nodes),
            nodes,
            edges,
            warnings: Vec::new(),
        };
        Ok(HookCanvasComponentSnapshot { snapshot, previews })
    }

    fn missing() -> Self {
        Self {
            snapshot: HookCanvasSnapshot {
                available: false,
                revision: "missing".to_owned(),
                updated_at: None,
                workflow_id: None,
                bounds: HookCanvasBounds::default(),
                nodes: Vec::new(),
                edges: Vec::new(),
                warnings: Vec::new(),
            },
            preview_sources: HashMap::new(),
            preview_roots: Vec::new(),
        }
    }
}
