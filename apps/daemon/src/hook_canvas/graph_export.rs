fn canvas_bounds(nodes: &[HookCanvasNode]) -> HookCanvasBounds {
    let Some(first) = nodes.first() else {
        return HookCanvasBounds::default();
    };
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width;
    let mut max_y = first.y + first.height;
    for node in &nodes[1..] {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x + node.width);
        max_y = max_y.max(node.y + node.height);
    }
    HookCanvasBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn edge_port_points(
    source: &HookCanvasNode,
    target: &HookCanvasNode,
) -> (HookCanvasPoint, HookCanvasPoint) {
    let source_gap = if source.minified {
        MINIFIED_EDGE_PORT_GAP
    } else {
        DEFAULT_EDGE_PORT_GAP
    };
    let target_gap = if target.minified {
        MINIFIED_EDGE_PORT_GAP
    } else {
        DEFAULT_EDGE_PORT_GAP
    };
    (
        HookCanvasPoint {
            x: source.x + source.width + source_gap,
            y: source.y + source.height / 2.0,
        },
        HookCanvasPoint {
            x: target.x - target_gap,
            y: target.y + target.height / 2.0,
        },
    )
}

fn component_ids_for(
    nodes: &[HookCanvasNode],
    edges: &[HookCanvasEdge],
) -> HashMap<String, String> {
    let mut adjacency = nodes
        .iter()
        .map(|node| (node.id.clone(), Vec::<String>::new()))
        .collect::<HashMap<_, _>>();
    for edge in edges {
        if let Some(neighbors) = adjacency.get_mut(&edge.source_node_id) {
            neighbors.push(edge.target_node_id.clone());
        }
        if let Some(neighbors) = adjacency.get_mut(&edge.target_node_id) {
            neighbors.push(edge.source_node_id.clone());
        }
    }

    let mut visited = HashSet::new();
    let mut component_ids = HashMap::new();

    for node in nodes {
        if visited.contains(&node.id) {
            continue;
        }

        let mut queue = std::collections::VecDeque::from([node.id.clone()]);
        let mut members = Vec::new();
        visited.insert(node.id.clone());

        while let Some(current) = queue.pop_front() {
            members.push(current.clone());
            for neighbor in adjacency.get(&current).into_iter().flatten() {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }

        members.sort();
        let component_id = members.first().cloned().unwrap_or_else(|| node.id.clone());
        for member in members {
            component_ids.insert(member, component_id.clone());
        }
    }

    component_ids
}

fn workflow_export_metadata_for(
    nodes: &[HookCanvasNode],
    edges: &[HookCanvasEdge],
) -> (HashMap<String, String>, HashMap<String, Vec<String>>) {
    let mut ordered_nodes = nodes.iter().collect::<Vec<_>>();
    ordered_nodes.sort_by(|left, right| left.id.cmp(&right.id));

    let mut used_ids = HashSet::new();
    let mut workflow_node_ids = HashMap::new();

    for node in ordered_nodes {
        let base = workflow_node_id_base(node);
        let mut candidate = base.clone();
        let mut suffix = 2;
        while used_ids.contains(&candidate) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        used_ids.insert(candidate.clone());
        workflow_node_ids.insert(node.id.clone(), candidate);
    }

    let mut upstream_workflow_node_ids = nodes
        .iter()
        .map(|node| (node.id.clone(), Vec::<String>::new()))
        .collect::<HashMap<_, _>>();

    for edge in edges {
        let Some(source_workflow_node_id) = workflow_node_ids.get(&edge.source_node_id).cloned()
        else {
            continue;
        };
        let Some(target_upstreams) = upstream_workflow_node_ids.get_mut(&edge.target_node_id)
        else {
            continue;
        };
        if !target_upstreams.contains(&source_workflow_node_id) {
            target_upstreams.push(source_workflow_node_id);
        }
    }

    for upstreams in upstream_workflow_node_ids.values_mut() {
        upstreams.sort();
    }

    (workflow_node_ids, upstream_workflow_node_ids)
}

fn workflow_node_id_base(node: &HookCanvasNode) -> String {
    let base = node
        .art_id
        .as_deref()
        .and_then(|art_id| art_id.rsplit('/').next())
        .unwrap_or(node.id.as_str());
    sanitize_workflow_node_id(base)
}

fn yaml_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn yaml_mapping_key(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        value.to_owned()
    } else {
        yaml_single_quoted(value)
    }
}

fn sanitize_workflow_node_id(value: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            sanitized.push(ch);
            previous_was_dash = false;
        } else if !previous_was_dash {
            sanitized.push('-');
            previous_was_dash = true;
        }
    }

    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "node".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
