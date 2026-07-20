use loom_core::{ActorId, RunStatus, SessionId};
use loom_durable::InMemoryEventStore;
use loom_workflow::artloom::convert_artloom_yaml;
use loom_workflow::{StepOutcome, WorkflowAction, WorkflowExecutor};

const ARTLOOM_SUCCESS_YAML: &str = include_str!("../../../examples/artloom/success-dag.yaml");
const ARTLOOM_MIXED_FAILURE_YAML: &str =
    include_str!("../../../examples/artloom/mixed-failure-dag.yaml");

#[test]
fn artloom_yaml_converts_to_valid_loom_workflow_graph() {
    let converted = convert_artloom_yaml("artloom.success", ARTLOOM_SUCCESS_YAML)
        .expect("convert ArtLoom YAML");

    assert_eq!(converted.name.as_deref(), Some("ArtLoom Success DAG"));
    assert_eq!(
        converted.description.as_deref(),
        Some("Selected ArtLoom-style workflow fixture for Loom conversion.")
    );
    assert_eq!(converted.graph.id.as_str(), "artloom.success");
    assert_eq!(converted.graph.entry_node, "root");
    assert_eq!(converted.graph.nodes.len(), 3);
    assert_eq!(converted.graph.edges.len(), 2);
    assert!(converted.graph.validate().is_ok());

    let draft = converted.graph.nodes.get("draft").expect("draft node");
    let WorkflowAction::Agent { actor_id } = &draft.action;
    assert_eq!(actor_id.as_str(), "writer");
}

#[tokio::test]
async fn converted_artloom_success_fixture_validates_and_runs() {
    let converted = convert_artloom_yaml("artloom.success", ARTLOOM_SUCCESS_YAML)
        .expect("convert ArtLoom YAML");
    let store = InMemoryEventStore::default();
    let executor = WorkflowExecutor::new(&store)
        .with_actor(ActorId::new("planner"), StepOutcome::succeed("planned"))
        .with_actor(ActorId::new("writer"), StepOutcome::succeed("drafted"))
        .with_actor(ActorId::new("reviewer"), StepOutcome::succeed("approved"));

    let summary = executor
        .run(SessionId::new(), &converted.graph)
        .await
        .expect("run converted ArtLoom fixture");

    assert_eq!(summary.status, RunStatus::Succeeded);
    assert_eq!(summary.completed_nodes, vec!["root", "draft", "review"]);
    assert!(summary.failed_nodes.is_empty());
}

#[tokio::test]
async fn converted_artloom_mixed_failure_fixture_records_failure_without_desktop_ui() {
    let converted = convert_artloom_yaml("artloom.mixed_failure", ARTLOOM_MIXED_FAILURE_YAML)
        .expect("convert ArtLoom YAML");
    let store = InMemoryEventStore::default();
    let executor = WorkflowExecutor::new(&store)
        .with_actor(ActorId::new("planner"), StepOutcome::succeed("planned"))
        .with_actor(ActorId::new("writer"), StepOutcome::succeed("drafted"))
        .with_actor(
            ActorId::new("failing-art"),
            StepOutcome::fail("image input missing"),
        );

    let summary = executor
        .run(SessionId::new(), &converted.graph)
        .await
        .expect("run converted mixed fixture");

    assert_eq!(summary.status, RunStatus::Failed);
    assert_eq!(summary.completed_nodes, vec!["root", "success_sibling"]);
    assert_eq!(summary.failed_nodes, vec!["failing_child"]);
}
