use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn collect_workflow_uses_dedupes_and_skips_stickers() {
    let yaml = r#"
name: wf
nodes:
  - id: a
    uses: __sticker__
  - id: b
    uses: neuro.official/resize
  - id: c
    uses: neuro.official/ocr
    needs: [b]
  - id: d
    uses: neuro.official/resize
    needs: [c]
"#;
    let uses = super::collect_workflow_uses(yaml).expect("collect uses");
    assert_eq!(
        uses,
        vec![
            "neuro.official/resize".to_owned(),
            "neuro.official/ocr".to_owned()
        ]
    );
    assert!(matches!(
        super::collect_workflow_uses("nodes:\n  - id: bad\n    uses: unqualified\n"),
        Err(WorkflowStoreError::InvalidWorkflowYaml(_))
    ));
}

use super::*;

fn temp_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "loom-workflow-store-{name}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create temp workflow root");
    root
}

#[test]
fn hook_live_alias_uses_latest_yaml() {
    let root = temp_root("hook-live-alias");
    let store = WorkflowStore::new(&root);
    let yaml = "name: Live\nnodes: []\n";

    assert_eq!(workflow_file_name("hook-live"), "latest.yaml");

    store
        .save_workflow("hook-live", yaml)
        .expect("save hook live workflow");

    assert!(root.join("latest.yaml").exists());
    assert!(!root.join("hook-live.yaml").exists());
    assert_eq!(
        store
            .load_workflow("hook-live")
            .expect("load live workflow"),
        yaml
    );

    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}

#[test]
fn list_workflows_includes_hook_live_alias_when_latest_yaml_exists() {
    let root = temp_root("hook-live-list");
    let store = WorkflowStore::new(&root);
    let yaml = "name: Hook 实时工作流\nnodes:\n  - id: screenshot\n    uses: __sticker__\n";

    store
        .save_workflow("hook-live", yaml)
        .expect("save hook live workflow");

    let listed = store.list_workflows().expect("list workflows");
    let live = listed
        .iter()
        .find(|workflow| workflow.id == "hook-live")
        .expect("hook live workflow should be listed");

    assert_eq!(live.name, "Hook 实时工作流");
    assert_eq!(live.node_count, 1);
    assert!(!root.join("workflow_index.json").exists());

    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}

#[test]
fn save_load_list_and_delete_workflow_roundtrip() {
    let root = temp_root("roundtrip");
    let store = WorkflowStore::new(&root);
    let yaml = r#"name: Paint Flow
description: demo
nodes:
  - id: prompt
    uses: neuro.official/text-prompt
  - id: image
    uses: neuro.official/image-generate
    needs: [prompt]
"#;

    let metadata = store
        .save_workflow("paint-flow", yaml)
        .expect("save workflow");

    assert_eq!(metadata.id, "paint-flow");
    assert_eq!(metadata.name, "Paint Flow");
    assert_eq!(metadata.node_count, 2);
    assert_eq!(
        store.load_workflow("paint-flow").expect("load workflow"),
        yaml
    );
    assert!(root.join("workflow_index.json").exists());

    let listed = store.list_workflows().expect("list workflows");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "paint-flow");
    assert_eq!(listed[0].node_count, 2);

    store
        .delete_workflow("paint-flow")
        .expect("delete workflow");
    assert!(!root.join("paint-flow.yaml").exists());
    assert!(store.load_workflow("paint-flow").is_err());
    assert!(store
        .list_workflows()
        .expect("list after delete")
        .is_empty());

    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}

#[test]
fn graph_json_roundtrips_to_yaml_and_back() {
    let graph = serde_json::json!({
        "nodes": [
            {
                "id": "prompt",
                "type": "artNode",
                "position": { "x": 10, "y": 20 },
                "data": {
                    "artId": "neuro.official/text-prompt",
                    "label": "Prompt",
                    "params": { "prompt": "castle", "strength": 0.75 }
                }
            },
            {
                "id": "image",
                "type": "artNode",
                "position": { "x": 300, "y": 20 },
                "data": {
                    "artId": "neuro.official/image-generate",
                    "label": "Generate",
                    "params": { "steps": 20 }
                }
            }
        ],
        "edges": [
            {
                "source": "prompt",
                "target": "image",
                "sourceHandle": "text",
                "targetHandle": "prompt"
            }
        ]
    });

    let yaml = graph_json_to_workflow_yaml(&graph, Some("Roundtrip"), Some("demo"))
        .expect("graph to yaml");
    assert!(yaml.contains("name: Roundtrip"));
    assert!(yaml.contains("description: demo"));
    assert!(yaml.contains("uses: neuro.official/text-prompt"));
    assert!(yaml.contains("uses: neuro.official/image-generate"));

    let parsed = workflow_yaml_to_graph_json(&yaml).expect("yaml to graph");
    let nodes = parsed["nodes"].as_array().expect("nodes array");
    assert_eq!(nodes.len(), 2);

    let prompt = nodes
        .iter()
        .find(|node| node["id"] == "prompt")
        .expect("prompt node");
    assert_eq!(prompt["data"]["params"]["prompt"], "castle");
    assert_eq!(prompt["data"]["params"]["strength"], 0.75);

    let image = nodes
        .iter()
        .find(|node| node["id"] == "image")
        .expect("image node");
    assert_eq!(image["data"]["params"]["steps"], 20);

    let edges = parsed["edges"].as_array().expect("edges array");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["source"], "prompt");
    assert_eq!(edges[0]["target"], "image");
    assert_eq!(edges[0]["sourceHandle"], "text");
    assert_eq!(edges[0]["targetHandle"], "prompt");
}

#[test]
fn graph_codec_rejects_noncanonical_node_types_and_unqualified_art_ids() {
    for graph in [
        serde_json::json!({ "nodes": [{ "type": "artNode", "data": { "artId": "neuro.official/demo" } }], "edges": [] }),
        serde_json::json!({ "nodes": [{ "id": "missing", "data": {} }], "edges": [] }),
        serde_json::json!({ "nodes": [{ "id": "old", "type": "art", "data": { "artId": "neuro.official/demo" } }], "edges": [] }),
        serde_json::json!({ "nodes": [{ "id": "bare", "type": "artNode", "data": { "artId": "demo" } }], "edges": [] }),
        serde_json::json!({ "nodes": [{ "id": "empty", "type": "artNode", "data": {} }], "edges": [] }),
    ] {
        assert!(matches!(
            graph_json_to_workflow_yaml(&graph, None, None),
            Err(WorkflowStoreError::InvalidWorkflowGraph(_))
        ));
    }
}

#[test]
fn workflow_yaml_codec_rejects_missing_or_unqualified_uses() {
    for yaml in [
        "name: invalid\nnodes:\n  - id: missing\n",
        "name: invalid\nnodes:\n  - id: bare\n    uses: demo\n",
    ] {
        assert!(matches!(
            workflow_yaml_to_graph_json(yaml),
            Err(WorkflowStoreError::InvalidWorkflowYaml(_))
        ));
    }
}

#[test]
fn store_rejects_unsafe_workflow_ids_and_malformed_index() {
    let root = temp_root("invalid-inputs");
    let store = WorkflowStore::new(&root);
    let yaml = "name: Valid\nnodes: []\n";

    for id in [
        "",
        "..",
        "../escape",
        "folder/name",
        "folder\\name",
        "drive:name",
        "CON",
        "trailing.",
        "control\nname",
    ] {
        assert!(matches!(
            store.save_workflow(id, yaml),
            Err(WorkflowStoreError::InvalidWorkflowId(_))
        ));
    }

    fs::write(root.join("workflow_index.json"), b"{").expect("write malformed index");
    assert!(matches!(
        store.list_workflows(),
        Err(WorkflowStoreError::Json(_))
    ));
    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}

#[test]
fn list_rejects_malformed_workflow_yaml() {
    let root = temp_root("malformed-yaml");
    fs::write(root.join("broken.yaml"), "name: [unterminated\n").expect("write malformed workflow");
    let store = WorkflowStore::new(&root);
    assert!(matches!(
        store.list_workflows(),
        Err(WorkflowStoreError::Yaml(_))
    ));
    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}

#[test]
fn workflow_json_and_yaml_budgets_reject_hostile_documents() {
    let oversized = format!(
        "name: oversized\nnodes: []\npadding: {}\n",
        "x".repeat(super::validation::MAX_WORKFLOW_YAML_BYTES)
    );
    assert!(matches!(
        workflow_yaml_to_graph_json(&oversized),
        Err(WorkflowStoreError::InvalidWorkflowYaml(_))
    ));

    let mut nested = serde_json::json!(true);
    for _ in 0..=super::validation::MAX_WORKFLOW_DEPTH {
        nested = serde_json::json!({ "nested": nested });
    }
    let graph = serde_json::json!({
        "nodes": [{
            "id": "deep",
            "type": "artNode",
            "data": {
                "artId": "neuro.official/deep",
                "params": { "payload": nested }
            }
        }],
        "edges": []
    });
    assert!(matches!(
        graph_json_to_workflow_yaml(&graph, None, None),
        Err(WorkflowStoreError::InvalidWorkflowGraph(_))
    ));
}

#[test]
fn concurrent_saves_preserve_every_index_entry_and_leave_no_temporaries() {
    use std::sync::{Arc, Barrier};

    const WRITERS: usize = 12;
    let root = temp_root("concurrent-save");
    let store = Arc::new(WorkflowStore::new(&root));
    let barrier = Arc::new(Barrier::new(WRITERS));
    let mut writers = Vec::new();
    for index in 0..WRITERS {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        writers.push(std::thread::spawn(move || {
            barrier.wait();
            store
                .save_workflow(
                    &format!("workflow-{index:02}"),
                    &format!("name: Workflow {index:02}\nnodes: []\n"),
                )
                .expect("save workflow");
        }));
    }
    for writer in writers {
        writer.join().expect("writer thread");
    }

    let listed = store.list_workflows().expect("list workflows");
    assert_eq!(listed.len(), WRITERS);
    for index in 0..WRITERS {
        let id = format!("workflow-{index:02}");
        assert!(listed.iter().any(|workflow| workflow.id == id));
        assert!(store.load_workflow(&id).is_ok());
    }
    assert!(fs::read_dir(&root)
        .expect("read workflow root")
        .all(|entry| !entry
            .expect("directory entry")
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")));
    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}

#[cfg(unix)]
#[test]
fn store_rejects_symlinked_destinations_and_applies_private_modes() {
    use std::os::unix::fs::{symlink, PermissionsExt as _};

    let root = temp_root("symlink");
    let outside = root.with_extension("outside.yaml");
    fs::write(&outside, "outside\n").expect("write outside file");
    symlink(&outside, root.join("linked.yaml")).expect("create symlink");
    let store = WorkflowStore::new(&root);
    assert!(matches!(
        store.save_workflow("linked", "name: Linked\nnodes: []\n"),
        Err(WorkflowStoreError::Io(_))
    ));
    assert_eq!(
        fs::read_to_string(&outside).expect("outside content"),
        "outside\n"
    );

    store
        .save_workflow("private", "name: Private\nnodes: []\n")
        .expect("save private workflow");
    assert_eq!(
        fs::metadata(&root)
            .expect("root metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for path in [root.join("private.yaml"), root.join("workflow_index.json")] {
        assert_eq!(
            fs::metadata(path)
                .expect("file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    fs::remove_file(outside).expect("remove outside file");
    fs::remove_dir_all(root).expect("cleanup temp workflow root");
}
