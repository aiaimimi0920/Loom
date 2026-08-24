use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{
    helpers::{
        is_live_workflow_id, now_string, sort_metadata, workflow_details_from_graph,
        workflow_details_from_yaml,
    },
    storage::{
        ensure_private_root, ensure_regular_directory_entry, lock_store, read_bounded_utf8,
        remove_regular_file, write_atomic,
    },
    validation::{
        validate_workflow_id, MAX_STORED_WORKFLOWS, MAX_WORKFLOW_INDEX_BYTES,
        MAX_WORKFLOW_YAML_BYTES,
    },
    workflow_file_name, workflow_yaml_to_graph_json, WorkflowMetadata, WorkflowStoreError,
    WorkflowStoreResult, WORKFLOW_INDEX_FILE,
};

#[derive(Clone, Debug)]
pub struct WorkflowStore {
    root: PathBuf,
}

impl WorkflowStore {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save_workflow(&self, id: &str, yaml: &str) -> WorkflowStoreResult<WorkflowMetadata> {
        let graph = workflow_yaml_to_graph_json(yaml)?;
        let path = self.workflow_path(id)?;
        let _lock = lock_store(&self.root)?;
        write_atomic(&path, yaml.as_bytes())?;
        let (name, node_count) = workflow_details_from_graph(&graph);

        let metadata = WorkflowMetadata {
            id: id.to_owned(),
            name: name.unwrap_or_else(|| id.to_owned()),
            node_count,
            updated_at: now_string(),
        };

        if !is_live_workflow_id(id) {
            let mut workflows = self.read_index()?;
            if let Some(existing) = workflows.iter_mut().find(|workflow| workflow.id == id) {
                *existing = metadata.clone();
            } else {
                workflows.push(metadata.clone());
            }
            sort_metadata(&mut workflows);
            self.write_index(&workflows)?;
        }

        Ok(metadata)
    }

    pub fn load_workflow(&self, id: &str) -> WorkflowStoreResult<String> {
        let path = self.workflow_path(id)?;
        ensure_private_root(&self.root)?;
        let yaml = match read_bounded_utf8(&path, MAX_WORKFLOW_YAML_BYTES) {
            Ok(yaml) => yaml,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(WorkflowStoreError::NotFound(id.to_owned()));
            }
            Err(error) => return Err(error.into()),
        };
        workflow_yaml_to_graph_json(&yaml)?;
        Ok(yaml)
    }

    pub fn list_workflows(&self) -> WorkflowStoreResult<Vec<WorkflowMetadata>> {
        let _lock = lock_store(&self.root)?;

        let indexed = self.read_index()?;
        let mut by_id = BTreeMap::new();
        let mut live_workflow = None;
        let mut changed = false;

        for workflow in indexed {
            if workflow.id.trim().is_empty() || is_live_workflow_id(&workflow.id) {
                changed = true;
                continue;
            }

            let path = self.workflow_path(&workflow.id)?;
            match ensure_regular_directory_entry(&path) {
                Ok(()) => {
                    by_id.insert(workflow.id.clone(), workflow);
                }
                Err(error) if error.kind() == ErrorKind::NotFound => changed = true,
                Err(error) => return Err(error.into()),
            }
        }

        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
                continue;
            }
            ensure_regular_directory_entry(&path)?;

            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if stem == "latest" {
                let yaml = read_bounded_utf8(&path, MAX_WORKFLOW_YAML_BYTES)?;
                let (name, node_count) = workflow_details_from_yaml(&yaml)?;
                live_workflow = Some(WorkflowMetadata {
                    id: "hook-live".to_owned(),
                    name: name.unwrap_or_else(|| "Hook 实时工作流".to_owned()),
                    node_count,
                    updated_at: now_string(),
                });
                continue;
            }

            let yaml = read_bounded_utf8(&path, MAX_WORKFLOW_YAML_BYTES)?;
            let id = stem.to_owned();
            validate_workflow_id(&id)?;
            let (name, node_count) = workflow_details_from_yaml(&yaml)?;
            let name = name.unwrap_or_else(|| id.clone());

            match by_id.get_mut(&id) {
                Some(existing) if existing.name == name && existing.node_count == node_count => {}
                Some(existing) => {
                    existing.name = name;
                    existing.node_count = node_count;
                    changed = true;
                }
                None => {
                    by_id.insert(
                        id.clone(),
                        WorkflowMetadata {
                            id,
                            name,
                            node_count,
                            updated_at: now_string(),
                        },
                    );
                    changed = true;
                }
            }
        }

        let mut workflows: Vec<_> = by_id.into_values().collect();
        sort_metadata(&mut workflows);
        if changed {
            self.write_index(&workflows)?;
        }
        if let Some(workflow) = live_workflow {
            workflows.push(workflow);
            sort_metadata(&mut workflows);
        }
        Ok(workflows)
    }

    pub fn delete_workflow(&self, id: &str) -> WorkflowStoreResult<()> {
        let path = self.workflow_path(id)?;
        let _lock = lock_store(&self.root)?;
        remove_regular_file(&path)?;

        if !is_live_workflow_id(id) {
            let mut workflows = self.read_index()?;
            let before = workflows.len();
            workflows.retain(|workflow| workflow.id != id);
            if workflows.len() != before {
                self.write_index(&workflows)?;
            }
        }

        Ok(())
    }

    fn workflow_path(&self, id: &str) -> WorkflowStoreResult<PathBuf> {
        validate_workflow_id(id)?;
        Ok(self.root.join(workflow_file_name(id)))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(WORKFLOW_INDEX_FILE)
    }

    fn read_index(&self) -> WorkflowStoreResult<Vec<WorkflowMetadata>> {
        let path = self.index_path();
        let content = match read_bounded_utf8(&path, MAX_WORKFLOW_INDEX_BYTES) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let value: serde_json::Value = serde_json::from_str(&content)?;
        if !loom_security::json::value_is_within_depth(
            &value,
            crate::validation::MAX_WORKFLOW_DEPTH,
        ) {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "workflow index exceeds the nesting limit of {} levels",
                    crate::validation::MAX_WORKFLOW_DEPTH
                ),
            )
            .into());
        }
        let workflows: Vec<WorkflowMetadata> = serde_json::from_value(value)?;
        if workflows.len() > MAX_STORED_WORKFLOWS {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("workflow index exceeds {MAX_STORED_WORKFLOWS} entries"),
            )
            .into());
        }
        Ok(workflows)
    }

    fn write_index(&self, workflows: &[WorkflowMetadata]) -> WorkflowStoreResult<()> {
        if workflows.len() > MAX_STORED_WORKFLOWS {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("workflow index exceeds {MAX_STORED_WORKFLOWS} entries"),
            )
            .into());
        }
        let content = serde_json::to_string_pretty(workflows)?;
        if content.len() > MAX_WORKFLOW_INDEX_BYTES {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                format!("workflow index exceeds {MAX_WORKFLOW_INDEX_BYTES} bytes"),
            )
            .into());
        }
        write_atomic(&self.index_path(), content.as_bytes())?;
        Ok(())
    }
}
