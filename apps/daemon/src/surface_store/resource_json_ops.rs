// Resource projection, scene lookup, and JSON merge helpers shared by store mutations.
fn surface_request_id(event_id: &str) -> String {
    format!(
        "request:{}",
        event_id.strip_prefix("event:").unwrap_or(event_id)
    )
}

fn merge_resources(
    target: &mut Vec<loom_protocol::SurfaceResourceDescriptor>,
    additions: &[loom_protocol::SurfaceResourceDescriptor],
) {
    for resource in additions {
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.resource_id == resource.resource_id)
        {
            *existing = resource.clone();
        } else {
            target.push(resource.clone());
        }
    }
}

fn merge_resource_leases(
    target: &mut Vec<loom_protocol::SurfaceResourceLease>,
    additions: &[loom_protocol::SurfaceResourceLease],
) {
    for lease in additions {
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.lease_id == lease.lease_id)
        {
            *existing = lease.clone();
        } else {
            target.push(lease.clone());
        }
    }
}

fn merge_json(target: &mut Value, patch: &Value) {
    match patch {
        Value::Object(patch) => {
            if !target.is_object() {
                *target = Value::Object(Default::default());
            }
            let target = target.as_object_mut().expect("object initialized");
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(key);
                } else {
                    merge_json(target.entry(key.clone()).or_insert(Value::Null), value);
                }
            }
        }
        replacement => *target = replacement.clone(),
    }
}

fn apply_operation(
    root: &mut SurfaceNode,
    operation: &SurfacePatchOperation,
) -> Result<(), SurfaceStoreError> {
    match operation {
        SurfacePatchOperation::Set {
            node_id,
            path,
            value,
        } => mutate_node_json(root, node_id, path, Some(value.clone())),
        SurfacePatchOperation::Remove { node_id, path } => {
            mutate_node_json(root, node_id, path, None)
        }
        SurfacePatchOperation::InsertNode {
            parent_id,
            index,
            node,
        } => {
            let parent = find_node_mut(root, parent_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(parent_id.clone()))?;
            if *index > parent.children.len() {
                return Err(SurfaceStoreError::Invalid(format!(
                    "insert index {index} is out of range"
                )));
            }
            parent.children.insert(*index, node.clone());
            Ok(())
        }
        SurfacePatchOperation::RemoveNode { node_id } => {
            if root.id == *node_id {
                return Err(SurfaceStoreError::Invalid(
                    "the Surface root node cannot be removed".to_owned(),
                ));
            }
            remove_node(root, node_id)
                .map(|_| ())
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))
        }
        SurfacePatchOperation::MoveNode {
            node_id,
            parent_id,
            index,
        } => {
            if root.id == *node_id {
                return Err(SurfaceStoreError::Invalid(
                    "the Surface root node cannot be moved".to_owned(),
                ));
            }
            let node = remove_node(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            let parent = find_node_mut(root, parent_id).ok_or_else(|| {
                SurfaceStoreError::Invalid("a node cannot move into itself".into())
            })?;
            if *index > parent.children.len() {
                return Err(SurfaceStoreError::Invalid(format!(
                    "move index {index} is out of range"
                )));
            }
            parent.children.insert(*index, node);
            Ok(())
        }
        SurfacePatchOperation::ReplaceNode { node_id, node } => {
            if node.id != *node_id {
                return Err(SurfaceStoreError::Invalid(
                    "replacement node must preserve its stable id".to_owned(),
                ));
            }
            let target = find_node_mut(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            *target = node.clone();
            Ok(())
        }
        SurfacePatchOperation::SetVisibility { node_id, visible } => {
            let node = find_node_mut(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            if !node.props.is_object() {
                node.props = Value::Object(Default::default());
            }
            node.props
                .as_object_mut()
                .expect("object initialized")
                .insert("visible".to_owned(), Value::Bool(*visible));
            Ok(())
        }
        SurfacePatchOperation::SetBinding {
            node_id,
            path,
            binding,
        } => {
            let node = find_node_mut(root, node_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(node_id.clone()))?;
            if !node.props.is_object() {
                node.props = Value::Object(Default::default());
            }
            let props = node.props.as_object_mut().expect("object initialized");
            let bindings = props
                .entry("bindings")
                .or_insert_with(|| Value::Object(Default::default()));
            if !bindings.is_object() {
                *bindings = Value::Object(Default::default());
            }
            bindings
                .as_object_mut()
                .expect("object initialized")
                .insert(path.clone(), Value::String(binding.clone()));
            Ok(())
        }
    }
}

fn find_node_mut<'a>(root: &'a mut SurfaceNode, id: &str) -> Option<&'a mut SurfaceNode> {
    if root.id == id {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| find_node_mut(child, id))
}

fn find_node<'a>(root: &'a SurfaceNode, id: &str) -> Option<&'a SurfaceNode> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter().find_map(|child| find_node(child, id))
}

fn remove_node(root: &mut SurfaceNode, id: &str) -> Option<SurfaceNode> {
    if let Some(index) = root.children.iter().position(|child| child.id == id) {
        return Some(root.children.remove(index));
    }
    root.children
        .iter_mut()
        .find_map(|child| remove_node(child, id))
}

fn mutate_node_json(
    root: &mut SurfaceNode,
    node_id: &str,
    path: &str,
    value: Option<Value>,
) -> Result<(), SurfaceStoreError> {
    let allowed = ["/props", "/layout", "/style", "/accessibility", "/events"];
    if !allowed
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
    {
        return Err(SurfaceStoreError::Invalid(
            "node patch path must target props, layout, style, accessibility, or events".to_owned(),
        ));
    }
    let node = find_node_mut(root, node_id)
        .ok_or_else(|| SurfaceStoreError::NotFound(node_id.to_owned()))?;
    let stable_id = node.id.clone();
    let mut encoded = serde_json::to_value(&*node)?;
    match value {
        Some(value) => set_json_pointer(&mut encoded, path, value)?,
        None => remove_json_pointer(&mut encoded, path)?,
    }
    let replacement = serde_json::from_value::<SurfaceNode>(encoded)?;
    if replacement.id != stable_id {
        return Err(SurfaceStoreError::Invalid(
            "node patch changed a stable id".to_owned(),
        ));
    }
    *node = replacement;
    Ok(())
}
