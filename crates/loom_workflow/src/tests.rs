//! Workflow graph, execution, and planning contract coverage.

use super::*;
use loom_core::{ActorId, RunStatus, SessionId, WorkflowId};
use loom_durable::{EventStore, InMemoryEventStore};

fn three_node_success_graph() -> WorkflowGraph {
    WorkflowGraph::new(WorkflowId::new("sample.three_node"), "start")
        .with_node(WorkflowNode::agent("start", "planner"))
        .with_node(WorkflowNode::agent("draft", "writer"))
        .with_node(WorkflowNode::agent("review", "reviewer"))
        .with_edge("start", "draft")
        .with_edge("draft", "review")
}

#[test]
fn validation_rejects_missing_entry_missing_nodes_cycles_and_orphans() {
    let missing_entry = WorkflowGraph::new(WorkflowId::new("invalid.missing_entry"), "start")
        .with_node(WorkflowNode::agent("other", "planner"));
    assert!(matches!(
        missing_entry.validate(),
        Err(WorkflowError::MissingEntryNode { .. })
    ));

    let missing_node = WorkflowGraph::new(WorkflowId::new("invalid.missing_node"), "start")
        .with_node(WorkflowNode::agent("start", "planner"))
        .with_edge("start", "missing");
    assert!(matches!(
        missing_node.validate(),
        Err(WorkflowError::MissingNode { .. })
    ));

    let cycle = WorkflowGraph::new(WorkflowId::new("invalid.cycle"), "a")
        .with_node(WorkflowNode::agent("a", "planner"))
        .with_node(WorkflowNode::agent("b", "writer"))
        .with_edge("a", "b")
        .with_edge("b", "a");
    assert!(matches!(cycle.validate(), Err(WorkflowError::Cycle { .. })));

    let orphan = WorkflowGraph::new(WorkflowId::new("invalid.orphan"), "start")
        .with_node(WorkflowNode::agent("start", "planner"))
        .with_node(WorkflowNode::agent("orphan", "reviewer"));
    assert!(matches!(
        orphan.validate(),
        Err(WorkflowError::UnreachableNode { .. })
    ));
}

#[tokio::test]
async fn executor_runs_three_node_success_dag_and_records_durable_events() {
    let store = InMemoryEventStore::default();
    let graph = three_node_success_graph();
    let executor = WorkflowExecutor::new(&store)
        .with_actor(ActorId::new("planner"), StepOutcome::succeed("planned"))
        .with_actor(ActorId::new("writer"), StepOutcome::succeed("drafted"))
        .with_actor(ActorId::new("reviewer"), StepOutcome::succeed("approved"));

    let summary = executor
        .run(SessionId::new(), &graph)
        .await
        .expect("execute success dag");

    assert_eq!(summary.status, RunStatus::Succeeded);
    assert_eq!(
        summary.completed_nodes,
        vec!["start".to_owned(), "draft".to_owned(), "review".to_owned()]
    );
    assert!(summary.failed_nodes.is_empty());

    let events = store.events_for_run(summary.run_id).await.expect("events");
    assert_eq!(
        events.len(),
        5,
        "run start + 3 actor dispatch events + run finish"
    );
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[4].sequence, 5);
}

#[tokio::test]
async fn executor_records_mixed_success_failure_dag() {
    let store = InMemoryEventStore::default();
    let graph = three_node_success_graph();
    let executor = WorkflowExecutor::new(&store)
        .with_actor(ActorId::new("planner"), StepOutcome::succeed("planned"))
        .with_actor(ActorId::new("writer"), StepOutcome::fail("draft failed"))
        .with_actor(ActorId::new("reviewer"), StepOutcome::succeed("approved"));

    let summary = executor
        .run(SessionId::new(), &graph)
        .await
        .expect("execute mixed dag");

    assert_eq!(summary.status, RunStatus::Failed);
    assert_eq!(summary.completed_nodes, vec!["start".to_owned()]);
    assert_eq!(summary.failed_nodes, vec!["draft".to_owned()]);

    let events = store.events_for_run(summary.run_id).await.expect("events");
    assert_eq!(
        events.len(),
        4,
        "run start + 2 actor dispatches + run finish"
    );
    assert_eq!(events[3].sequence, 4);
}

#[tokio::test]
async fn executor_dispatches_configured_run_and_agent_hooks() {
    let store = InMemoryEventStore::default();
    let graph = three_node_success_graph();
    let hooks = loom_hooks::HookDispatcher::from_settings(loom_hooks::HookSettings::enabled(vec![
        loom_hooks::HookRule::new(loom_hooks::HookEventKind::RunStarted)
            .with_target(loom_hooks::HookTarget::memory("runs")),
        loom_hooks::HookRule::new(loom_hooks::HookEventKind::AgentStopped)
            .with_matcher(loom_hooks::HookMatcher::AgentIdExact("writer".to_owned()))
            .with_target(loom_hooks::HookTarget::memory("writer-agent")),
        loom_hooks::HookRule::new(loom_hooks::HookEventKind::RunStopped)
            .with_matcher(loom_hooks::HookMatcher::RunStatus(RunStatus::Succeeded))
            .with_target(loom_hooks::HookTarget::memory("run-finished")),
    ]));
    let executor = WorkflowExecutor::new(&store)
        .with_hooks(hooks)
        .with_actor(ActorId::new("planner"), StepOutcome::succeed("planned"))
        .with_actor(ActorId::new("writer"), StepOutcome::succeed("drafted"))
        .with_actor(ActorId::new("reviewer"), StepOutcome::succeed("approved"));

    let summary = executor
        .run(SessionId::new(), &graph)
        .await
        .expect("execute success dag with hooks");

    assert_eq!(summary.status, RunStatus::Succeeded);
    assert_eq!(summary.hook_deliveries.len(), 3);
    assert_eq!(summary.hook_deliveries[0].target.name, "runs");
    assert!(summary.hook_deliveries[0]
        .payload
        .contains("\"kind\":\"run_started\""));
    assert_eq!(summary.hook_deliveries[1].target.name, "writer-agent");
    assert!(summary.hook_deliveries[1].payload.contains("writer"));
    assert_eq!(summary.hook_deliveries[2].target.name, "run-finished");
    assert!(summary.hook_deliveries[2]
        .payload
        .contains("\"status\":\"succeeded\""));
}

struct LinearPlanner;

impl SystemTwoPlanner for LinearPlanner {
    fn plan(&self, request: PlanningRequest) -> WorkflowResult<WorkflowGraph> {
        Ok(WorkflowGraph::new(WorkflowId::new(request.goal), "start")
            .with_node(WorkflowNode::agent("start", "planner")))
    }
}

#[tokio::test]
async fn system_one_facade_runs_workflow_execution() {
    let store = InMemoryEventStore::default();
    let graph = three_node_success_graph();
    let runtime = SystemOneRuntime::new(&store)
        .with_actor(ActorId::new("planner"), StepOutcome::succeed("planned"))
        .with_actor(ActorId::new("writer"), StepOutcome::succeed("drafted"))
        .with_actor(ActorId::new("reviewer"), StepOutcome::succeed("approved"));

    let summary = runtime
        .execute(SessionId::new(), &graph)
        .await
        .expect("system one execute");

    assert_eq!(summary.status, RunStatus::Succeeded);
    assert_eq!(summary.completed_nodes.len(), 3);
}

#[test]
fn system_two_planning_trait_can_return_a_workflow_graph() {
    let planner = LinearPlanner;
    let graph = planner
        .plan(PlanningRequest::new("planned.workflow"))
        .expect("plan workflow");

    assert_eq!(graph.id.as_str(), "planned.workflow");
    assert!(graph.validate().is_ok());
}
