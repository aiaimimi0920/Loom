// Shared isolated-store, instance, and snapshot fixtures for Surface store tests.
use super::*;
use loom_protocol::{SurfaceResourceDescriptor, SurfaceResourceKind, SURFACE_PROTOCOL_VERSION};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("loom-surface-store-{name}-{}", Uuid::new_v4()))
        .join("instances.json")
}

fn create(store: &mut SurfaceInstanceStore) -> SurfaceInstanceRecord {
    store
        .create(
            "neuro.official/stock-price",
            "1.2.3",
            &"a".repeat(64),
            1,
            SurfaceInstancePersistence::Persistent,
            SurfaceInstanceMode::Independent,
        )
        .expect("create Surface instance")
}

fn snapshot(record: &SurfaceInstanceRecord, attachment_id: &str) -> SurfaceSnapshot {
    SurfaceSnapshot {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: record.descriptor.instance_id.clone(),
        attachment_id: attachment_id.to_owned(),
        art_id: record.descriptor.art_id.clone(),
        art_version: record.descriptor.art_version.clone(),
        revision: 1,
        runtime: loom_protocol::SurfaceRuntimeKind::Declarative,
        entry_resource_id: None,
        view_id: None,
        scene: SurfaceNode {
            id: "root".to_owned(),
            node_type: "column".to_owned(),
            children: vec![SurfaceNode {
                id: "price".to_owned(),
                node_type: "text".to_owned(),
                props: serde_json::json!({"text": "100"}),
                events: BTreeMap::from([("click".to_owned(), "refresh".to_owned())]),
                ..SurfaceNode::default()
            }],
            ..SurfaceNode::default()
        },
        authoritative_state: serde_json::json!({"price": 100}),
        resources: Vec::new(),
        resource_leases: Vec::new(),
    }
}
