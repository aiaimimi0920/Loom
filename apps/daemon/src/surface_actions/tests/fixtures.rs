// Shared Surface action test fixtures.
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use loom_protocol::{
        SurfaceEventClass, SurfaceHostCapabilities, SurfaceInstancePersistence, SurfaceNode,
        SurfaceRuntimeKind, SurfaceSnapshot,
    };
    use loom_tool_registry::{ToolDefinition, ToolExecution};

    use super::*;
    use crate::surface_resources::SurfaceResourceStore;
    use crate::surface_store::SurfaceInstanceStore;
    use crate::{register_hook_bridge_subscription, HookBridgeRuntime};

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-surface-actions-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn surface_tool(digest: &str) -> ToolDefinition {
        surface_tool_at("1.0.0", digest)
    }

    fn surface_tool_at(version: &str, digest: &str) -> ToolDefinition {
        ToolDefinition {
            id: "surface-action-test".to_owned(),
            name: "Surface Action Test".to_owned(),
            description: "Surface action executor fixture".to_owned(),
            enabled: true,
            execution: ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
            metadata: Some(json!({
                "dependencies": { "framework": "process" },
                "packageSecurity": { "version": version },
                "artPackage": {
                    "version": version,
                    "digest": digest,
                    "dir": "unused"
                },
                "capabilities": {
                    "surface": {
                        "protocolVersion": SURFACE_PROTOCOL_VERSION,
                        "apiVersion": "1.0",
                        "variants": [{
                            "runtime": "declarative",
                            "entry": "surface/main.json"
                        }],
                        "requiredNodes": ["column", "text", "button"],
                        "actions": [{
                            "id": "refresh_price",
                            "risk": "low",
                            "offlinePolicy": "reject",
                            "concurrency": "serial",
                            "idempotent": true,
                            "confirmation": false,
                            "cancelable": false,
                            "timeoutMs": 5000,
                            "progress": true
                        }]
                    }
                }
            })),
        }
    }

    fn host_capabilities() -> SurfaceHostCapabilities {
        SurfaceHostCapabilities {
            api_version: "1.0".to_owned(),
            runtimes: vec![SurfaceRuntimeKind::Declarative],
            nodes: vec!["column".to_owned(), "text".to_owned(), "button".to_owned()],
            transports: Vec::new(),
            capabilities: Vec::new(),
            input: Default::default(),
        }
    }

    fn setup_action_fixture(
        root: &Path,
        tool: ToolDefinition,
        hook_node_id: &str,
    ) -> (
        ToolRegistry,
        SharedSurfaceInstanceStore,
        SharedSurfaceResourceStore,
        SharedHookBridgeRuntime,
        String,
        String,
    ) {
        let digest = tool
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artPackage/digest"))
            .and_then(Value::as_str)
            .expect("fixture package digest")
            .to_owned();
        let tool_registry = ToolRegistry::new(root.join("tools"));
        tool_registry
            .save_tool(tool)
            .expect("save Surface fixture tool");
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_id) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    loom_protocol::SurfaceInstanceMode::Independent,
                )
                .expect("create instance");
            let attachment = store
                .attach(
                    &instance.descriptor.instance_id,
                    hook_node_id,
                    "device-000-local",
                    Some(host_capabilities()),
                )
                .expect("attach Surface instance");
            store
                .put_snapshot(
                    &instance.descriptor.instance_id,
                    SurfaceSnapshot {
                        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                        instance_id: instance.descriptor.instance_id.clone(),
                        attachment_id: attachment.descriptor.attachment_id.clone(),
                        art_id: instance.descriptor.art_id,
                        art_version: instance.descriptor.art_version,
                        revision: 1,
                        runtime: SurfaceRuntimeKind::Declarative,
                        entry_resource_id: None,
                        view_id: None,
                        scene: SurfaceNode {
                            id: "root".to_owned(),
                            node_type: "column".to_owned(),
                            children: vec![SurfaceNode {
                                id: "refresh".to_owned(),
                                node_type: "button".to_owned(),
                                events: BTreeMap::from([(
                                    "click".to_owned(),
                                    "refresh_price".to_owned(),
                                )]),
                                ..SurfaceNode::default()
                            }],
                            ..SurfaceNode::default()
                        },
                        authoritative_state: json!({"value": 0}),
                        resources: Vec::new(),
                        resource_leases: Vec::new(),
                    },
                )
                .expect("mount Surface snapshot");
            (
                instance.descriptor.instance_id,
                attachment.descriptor.attachment_id,
            )
        };
        let resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new(root.join("surface-resources"))
                .expect("open Surface resource store"),
        ));
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        (
            tool_registry,
            instances,
            resources,
            hook_bridge,
            instance_id,
            attachment_id,
        )
    }

    fn fixture_event(
        instance_id: &str,
        attachment_id: &str,
        event_id: &str,
        payload: Value,
    ) -> SurfaceEvent {
        SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.to_owned(),
            attachment_id: attachment_id.to_owned(),
            event_id: event_id.to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload,
        }
    }
