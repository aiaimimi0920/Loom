//! Workflow graph and executor contracts for Loom.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use loom_core::{ActorId, LoomError, LoomEvent, RunId, RunStatus, SessionId, WorkflowId};
use loom_durable::EventStore;
use loom_hooks::{HookDelivery, HookDispatcher, HookError, HookEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

/// Version of the workflow crate.
pub const LOOM_WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Workflow model and executor errors.
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("workflow `{workflow_id}` is missing entry node `{entry_node}`")]
    MissingEntryNode {
        workflow_id: WorkflowId,
        entry_node: String,
    },
    #[error("workflow `{workflow_id}` references missing node `{node_id}`")]
    MissingNode {
        workflow_id: WorkflowId,
        node_id: String,
    },
    #[error("workflow `{workflow_id}` contains a cycle involving node `{node_id}`")]
    Cycle {
        workflow_id: WorkflowId,
        node_id: String,
    },
    #[error("workflow `{workflow_id}` contains unreachable node `{node_id}`")]
    UnreachableNode {
        workflow_id: WorkflowId,
        node_id: String,
    },
    #[error("workflow node `{node_id}` references missing actor `{actor_id}`")]
    MissingActor { node_id: String, actor_id: ActorId },
    #[error("durable runtime error: {0}")]
    Durable(#[from] LoomError),
    #[error("hook dispatch error: {0}")]
    Hook(#[from] HookError),
}

/// Result alias for workflow operations.
pub type WorkflowResult<T> = Result<T, WorkflowError>;

/// A directed workflow graph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowGraph {
    pub id: WorkflowId,
    pub entry_node: String,
    pub nodes: BTreeMap<String, WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

impl WorkflowGraph {
    #[must_use]
    pub fn new(id: WorkflowId, entry_node: impl Into<String>) -> Self {
        Self {
            id,
            entry_node: entry_node.into(),
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_node(mut self, node: WorkflowNode) -> Self {
        self.nodes.insert(node.id.clone(), node);
        self
    }

    #[must_use]
    pub fn with_edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push(WorkflowEdge {
            from: from.into(),
            to: to.into(),
        });
        self
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        if !self.nodes.contains_key(&self.entry_node) {
            return Err(WorkflowError::MissingEntryNode {
                workflow_id: self.id.clone(),
                entry_node: self.entry_node.clone(),
            });
        }

        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) {
                return Err(WorkflowError::MissingNode {
                    workflow_id: self.id.clone(),
                    node_id: edge.from.clone(),
                });
            }
            if !self.nodes.contains_key(&edge.to) {
                return Err(WorkflowError::MissingNode {
                    workflow_id: self.id.clone(),
                    node_id: edge.to.clone(),
                });
            }
        }

        self.validate_acyclic()?;
        self.validate_reachable()?;
        Ok(())
    }

    fn children_by_node(&self) -> BTreeMap<&str, Vec<&str>> {
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for edge in &self.edges {
            children
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }
        children
    }

    fn validate_acyclic(&self) -> WorkflowResult<()> {
        #[derive(Clone, Copy, Eq, PartialEq)]
        enum VisitState {
            Visiting,
            Visited,
        }

        fn visit(
            graph: &WorkflowGraph,
            node_id: &str,
            children: &BTreeMap<&str, Vec<&str>>,
            states: &mut BTreeMap<String, VisitState>,
        ) -> WorkflowResult<()> {
            match states.get(node_id).copied() {
                Some(VisitState::Visiting) => {
                    return Err(WorkflowError::Cycle {
                        workflow_id: graph.id.clone(),
                        node_id: node_id.to_owned(),
                    });
                }
                Some(VisitState::Visited) => return Ok(()),
                None => {}
            }

            states.insert(node_id.to_owned(), VisitState::Visiting);
            for child in children.get(node_id).into_iter().flatten() {
                visit(graph, child, children, states)?;
            }
            states.insert(node_id.to_owned(), VisitState::Visited);
            Ok(())
        }

        let children = self.children_by_node();
        let mut states = BTreeMap::new();
        for node_id in self.nodes.keys() {
            visit(self, node_id, &children, &mut states)?;
        }
        Ok(())
    }

    fn validate_reachable(&self) -> WorkflowResult<()> {
        let children = self.children_by_node();
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([self.entry_node.as_str()]);

        while let Some(node_id) = queue.pop_front() {
            if !seen.insert(node_id.to_owned()) {
                continue;
            }
            for child in children.get(node_id).into_iter().flatten() {
                queue.push_back(child);
            }
        }

        for node_id in self.nodes.keys() {
            if !seen.contains(node_id) {
                return Err(WorkflowError::UnreachableNode {
                    workflow_id: self.id.clone(),
                    node_id: node_id.clone(),
                });
            }
        }

        Ok(())
    }

    fn execution_order(&self) -> Vec<&WorkflowNode> {
        let children = self.children_by_node();
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([self.entry_node.as_str()]);

        while let Some(node_id) = queue.pop_front() {
            if !seen.insert(node_id.to_owned()) {
                continue;
            }

            if let Some(node) = self.nodes.get(node_id) {
                ordered.push(node);
            }

            for child in children.get(node_id).into_iter().flatten() {
                queue.push_back(child);
            }
        }

        ordered
    }
}

/// Workflow node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowNode {
    pub id: String,
    pub action: WorkflowAction,
}

impl WorkflowNode {
    #[must_use]
    pub fn agent(id: impl Into<String>, actor_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            action: WorkflowAction::Agent {
                actor_id: ActorId::new(actor_id),
            },
        }
    }
}

/// Supported v1 workflow action kinds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum WorkflowAction {
    Agent { actor_id: ActorId },
}

/// Directed edge between two workflow nodes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
}

/// Deterministic test/runtime step outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepOutcome {
    pub status: StepStatus,
    pub message: String,
}

impl StepOutcome {
    #[must_use]
    pub fn succeed(message: impl Into<String>) -> Self {
        Self {
            status: StepStatus::Succeeded,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            status: StepStatus::Failed,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepStatus {
    Succeeded,
    Failed,
}

/// Summary returned after a workflow run finishes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowRunSummary {
    pub run_id: RunId,
    pub status: RunStatus,
    pub completed_nodes: Vec<String>,
    pub failed_nodes: Vec<String>,
    pub hook_deliveries: Vec<HookDelivery>,
}

/// Minimal workflow executor that records durable run and node events.
pub struct WorkflowExecutor<'a, S>
where
    S: EventStore,
{
    store: &'a S,
    actors: BTreeMap<ActorId, StepOutcome>,
    hooks: HookDispatcher,
}

impl<'a, S> WorkflowExecutor<'a, S>
where
    S: EventStore,
{
    #[must_use]
    pub fn new(store: &'a S) -> Self {
        Self {
            store,
            actors: BTreeMap::new(),
            hooks: HookDispatcher::default(),
        }
    }

    #[must_use]
    pub fn with_actor(mut self, actor_id: ActorId, outcome: StepOutcome) -> Self {
        self.actors.insert(actor_id, outcome);
        self
    }

    #[must_use]
    pub fn with_hooks(mut self, hooks: HookDispatcher) -> Self {
        self.hooks = hooks;
        self
    }

    pub async fn run(
        &self,
        session_id: SessionId,
        graph: &WorkflowGraph,
    ) -> WorkflowResult<WorkflowRunSummary> {
        graph.validate()?;

        let run_id = RunId::new();
        let mut sequence = 1_u64;
        self.store
            .append(LoomEvent::run_started(
                session_id,
                run_id,
                graph.id.clone(),
                sequence,
            ))
            .await?;
        let mut hook_deliveries = self
            .hooks
            .dispatch(&HookEvent::run_started(session_id, run_id))?;

        let mut completed_nodes = Vec::new();
        let mut failed_nodes = Vec::new();
        let mut status = RunStatus::Succeeded;

        for node in graph.execution_order() {
            let WorkflowAction::Agent { actor_id } = &node.action;
            let outcome = self
                .actors
                .get(actor_id)
                .ok_or_else(|| WorkflowError::MissingActor {
                    node_id: node.id.clone(),
                    actor_id: actor_id.clone(),
                })?;

            sequence += 1;
            self.store
                .append(LoomEvent::actor_message(
                    session_id,
                    run_id,
                    actor_id.clone(),
                    json!({
                        "node_id": node.id,
                        "status": match outcome.status {
                            StepStatus::Succeeded => "succeeded",
                            StepStatus::Failed => "failed",
                        },
                        "message": outcome.message,
                    }),
                    sequence,
                ))
                .await?;
            hook_deliveries.extend(
                self.hooks
                    .dispatch(&HookEvent::agent_stopped(session_id, actor_id.as_str()))?,
            );

            match outcome.status {
                StepStatus::Succeeded => completed_nodes.push(node.id.clone()),
                StepStatus::Failed => {
                    failed_nodes.push(node.id.clone());
                    status = RunStatus::Failed;
                    break;
                }
            }
        }

        sequence += 1;
        self.store
            .append(LoomEvent::run_finished(
                session_id, run_id, status, sequence,
            ))
            .await?;
        hook_deliveries.extend(
            self.hooks
                .dispatch(&HookEvent::run_stopped(session_id, run_id, status))?,
        );

        Ok(WorkflowRunSummary {
            run_id,
            status,
            completed_nodes,
            failed_nodes,
            hook_deliveries,
        })
    }
}

/// System 1 facade: deterministic/direct workflow execution.
pub struct SystemOneRuntime<'a, S>
where
    S: EventStore,
{
    executor: WorkflowExecutor<'a, S>,
}

impl<'a, S> SystemOneRuntime<'a, S>
where
    S: EventStore,
{
    #[must_use]
    pub fn new(store: &'a S) -> Self {
        Self {
            executor: WorkflowExecutor::new(store),
        }
    }

    #[must_use]
    pub fn with_actor(mut self, actor_id: ActorId, outcome: StepOutcome) -> Self {
        self.executor = self.executor.with_actor(actor_id, outcome);
        self
    }

    #[must_use]
    pub fn with_hooks(mut self, hooks: HookDispatcher) -> Self {
        self.executor = self.executor.with_hooks(hooks);
        self
    }

    pub async fn execute(
        &self,
        session_id: SessionId,
        graph: &WorkflowGraph,
    ) -> WorkflowResult<WorkflowRunSummary> {
        self.executor.run(session_id, graph).await
    }
}

/// Request supplied to System 2 planning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanningRequest {
    pub goal: String,
}

impl PlanningRequest {
    #[must_use]
    pub fn new(goal: impl Into<String>) -> Self {
        Self { goal: goal.into() }
    }
}

/// System 2 planning facade. Implementations produce a validated workflow graph.
pub trait SystemTwoPlanner {
    fn plan(&self, request: PlanningRequest) -> WorkflowResult<WorkflowGraph>;
}

#[cfg(test)]
mod tests;
