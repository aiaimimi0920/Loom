//! Hook event contracts for Loom automation.

use loom_core::{RunId, RunStatus, SessionId};
use loom_sandbox::{Sandbox, SandboxCommand, SandboxError, SandboxOutput};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version of the hooks crate.
pub const LOOM_HOOKS_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Hook dispatch errors.
#[derive(Debug, Error)]
pub enum HookError {
    #[error("hook payload serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("hook command target failed: {0}")]
    Command(#[from] SandboxError),
    #[error("hook command target `{target}` is missing command")]
    MissingCommand { target: String },
}

/// Result alias for hook dispatch.
pub type HookResult<T> = Result<T, HookError>;

/// Hook lifecycle events supported by Loom v1.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    RunStarted,
    RunStopped,
    BeforeToolCall,
    AfterToolCall,
    AgentStopped,
}

/// Serializable hook event payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HookEvent {
    pub kind: HookEventKind,
    pub session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<RunStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

impl HookEvent {
    #[must_use]
    pub fn run_started(session_id: SessionId, run_id: RunId) -> Self {
        Self::new(HookEventKind::RunStarted, session_id).with_run(run_id)
    }

    #[must_use]
    pub fn run_stopped(session_id: SessionId, run_id: RunId, status: RunStatus) -> Self {
        Self::new(HookEventKind::RunStopped, session_id)
            .with_run(run_id)
            .with_status(status)
    }

    #[must_use]
    pub fn before_tool_call(
        session_id: SessionId,
        run_id: RunId,
        tool_name: impl Into<String>,
    ) -> Self {
        Self::new(HookEventKind::BeforeToolCall, session_id)
            .with_run(run_id)
            .with_tool(tool_name)
    }

    #[must_use]
    pub fn after_tool_call(
        session_id: SessionId,
        run_id: RunId,
        tool_name: impl Into<String>,
        success: bool,
    ) -> Self {
        Self::new(HookEventKind::AfterToolCall, session_id)
            .with_run(run_id)
            .with_tool(tool_name)
            .with_tool_success(success)
    }

    #[must_use]
    pub fn agent_stopped(session_id: SessionId, agent_id: impl Into<String>) -> Self {
        Self::new(HookEventKind::AgentStopped, session_id).with_agent(agent_id)
    }

    fn new(kind: HookEventKind, session_id: SessionId) -> Self {
        Self {
            kind,
            session_id,
            run_id: None,
            status: None,
            tool_name: None,
            tool_success: None,
            agent_id: None,
        }
    }

    fn with_run(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }

    fn with_status(mut self, status: RunStatus) -> Self {
        self.status = Some(status);
        self
    }

    fn with_tool(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }

    fn with_tool_success(mut self, success: bool) -> Self {
        self.tool_success = Some(success);
        self
    }

    fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }
}

/// Hook settings. Defaults disabled so automation cannot affect runtime unless
/// explicitly configured.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HookSettings {
    pub enabled: bool,
    pub rules: Vec<HookRule>,
}

impl HookSettings {
    #[must_use]
    pub fn enabled(rules: Vec<HookRule>) -> Self {
        Self {
            enabled: true,
            rules,
        }
    }

    #[must_use]
    pub fn summary(&self) -> HookSettingsSummary {
        HookSettingsSummary {
            enabled: self.enabled,
            rule_count: self.rules.len(),
            target_count: self.rules.iter().map(|rule| rule.targets.len()).sum(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSettingsSummary {
    pub enabled: bool,
    pub rule_count: usize,
    pub target_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HookRule {
    pub event: HookEventKind,
    #[serde(default)]
    pub matcher: HookMatcher,
    #[serde(default)]
    pub targets: Vec<HookTarget>,
}

impl HookRule {
    #[must_use]
    pub fn new(event: HookEventKind) -> Self {
        Self {
            event,
            matcher: HookMatcher::Any,
            targets: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_matcher(mut self, matcher: HookMatcher) -> Self {
        self.matcher = matcher;
        self
    }

    #[must_use]
    pub fn with_target(mut self, target: HookTarget) -> Self {
        self.targets.push(target);
        self
    }

    #[must_use]
    pub fn with_targets(mut self, targets: Vec<HookTarget>) -> Self {
        self.targets = targets;
        self
    }

    fn matches(&self, event: &HookEvent) -> bool {
        self.event == event.kind && self.matcher.matches(event)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum HookMatcher {
    #[default]
    Any,
    ToolNameExact(String),
    AgentIdExact(String),
    RunStatus(RunStatus),
}

impl HookMatcher {
    fn matches(&self, event: &HookEvent) -> bool {
        match self {
            Self::Any => true,
            Self::ToolNameExact(expected) => event.tool_name.as_deref() == Some(expected.as_str()),
            Self::AgentIdExact(expected) => event.agent_id.as_deref() == Some(expected.as_str()),
            Self::RunStatus(expected) => event.status == Some(*expected),
        }
    }
}

/// Hook target. Memory targets keep dispatch testable without external
/// commands; command targets must be run through the deny-by-default sandbox.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HookTarget {
    pub kind: HookTargetKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<SandboxCommand>,
}

impl HookTarget {
    #[must_use]
    pub fn memory(name: impl Into<String>) -> Self {
        Self {
            kind: HookTargetKind::Memory,
            name: name.into(),
            command: None,
        }
    }

    #[must_use]
    pub fn command(name: impl Into<String>, command: SandboxCommand) -> Self {
        Self {
            kind: HookTargetKind::Command,
            name: name.into(),
            command: Some(command),
        }
    }
}

/// Supported hook target kinds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookTargetKind {
    Memory,
    Command,
}

/// Serialized payload delivered to a configured hook target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookDelivery {
    pub target: HookTarget,
    pub payload: String,
    pub command_output: Option<SandboxOutput>,
}

/// Hook dispatcher. Defaults disabled so hooks cannot affect runtime unless
/// explicitly enabled by configuration.
#[derive(Clone, Debug, Default)]
pub struct HookDispatcher {
    settings: HookSettings,
}

impl HookDispatcher {
    #[must_use]
    pub fn enabled(targets: Vec<HookTarget>) -> Self {
        Self::from_settings(HookSettings::enabled(
            all_hook_event_kinds()
                .into_iter()
                .map(|event| HookRule::new(event).with_targets(targets.clone()))
                .collect(),
        ))
    }

    #[must_use]
    pub fn from_settings(settings: HookSettings) -> Self {
        Self { settings }
    }

    #[must_use]
    pub fn settings(&self) -> &HookSettings {
        &self.settings
    }

    pub fn dispatch(&self, event: &HookEvent) -> HookResult<Vec<HookDelivery>> {
        if !self.settings.enabled {
            return Ok(Vec::new());
        }

        let payload = serde_json::to_string(event)?;
        let mut deliveries = Vec::new();
        for rule in self
            .settings
            .rules
            .iter()
            .filter(|rule| rule.matches(event))
        {
            deliveries.extend(rule.targets.iter().cloned().map(|target| HookDelivery {
                target,
                payload: payload.clone(),
                command_output: None,
            }));
        }
        Ok(deliveries)
    }

    pub fn dispatch_with_sandbox(
        &self,
        event: &HookEvent,
        sandbox: &Sandbox,
    ) -> HookResult<Vec<HookDelivery>> {
        if !self.settings.enabled {
            return Ok(Vec::new());
        }

        let payload = serde_json::to_string(event)?;
        let mut deliveries = Vec::new();
        for rule in self
            .settings
            .rules
            .iter()
            .filter(|rule| rule.matches(event))
        {
            for target in &rule.targets {
                let command_output =
                    match target.kind {
                        HookTargetKind::Memory => None,
                        HookTargetKind::Command => {
                            let command = target.command.as_ref().ok_or_else(|| {
                                HookError::MissingCommand {
                                    target: target.name.clone(),
                                }
                            })?;
                            Some(sandbox.execute_with_stdin(command, &payload)?)
                        }
                    };
                deliveries.push(HookDelivery {
                    target: target.clone(),
                    payload: payload.clone(),
                    command_output,
                });
            }
        }
        Ok(deliveries)
    }
}

fn all_hook_event_kinds() -> [HookEventKind; 5] {
    [
        HookEventKind::RunStarted,
        HookEventKind::RunStopped,
        HookEventKind::BeforeToolCall,
        HookEventKind::AfterToolCall,
        HookEventKind::AgentStopped,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::{RunId, RunStatus, SessionId};

    #[test]
    fn hook_events_serialize_run_tool_and_agent_lifecycle_payloads() {
        let session_id = SessionId::new();
        let run_id = RunId::new();
        let events = vec![
            HookEvent::run_started(session_id, run_id),
            HookEvent::run_stopped(session_id, run_id, RunStatus::Succeeded),
            HookEvent::before_tool_call(session_id, run_id, "sandbox.exec"),
            HookEvent::after_tool_call(session_id, run_id, "sandbox.exec", true),
            HookEvent::agent_stopped(session_id, "planner"),
        ];

        let encoded = serde_json::to_value(&events).expect("serialize hook events");

        assert_eq!(encoded[0]["kind"], "run_started");
        assert_eq!(encoded[1]["kind"], "run_stopped");
        assert_eq!(encoded[2]["kind"], "before_tool_call");
        assert_eq!(encoded[3]["kind"], "after_tool_call");
        assert_eq!(encoded[4]["kind"], "agent_stopped");
    }

    #[test]
    fn dispatcher_is_disabled_by_default_and_emits_no_payloads() {
        let dispatcher = HookDispatcher::default();
        let event = HookEvent::agent_stopped(SessionId::new(), "planner");

        let payloads = dispatcher.dispatch(&event).expect("dispatch disabled hook");

        assert!(payloads.is_empty());
    }

    #[test]
    fn enabled_dispatcher_returns_serialized_payload_for_each_target() {
        let dispatcher = HookDispatcher::enabled(vec![
            HookTarget::memory("audit-log"),
            HookTarget::memory("operator-stream"),
        ]);
        let event = HookEvent::before_tool_call(SessionId::new(), RunId::new(), "sandbox.exec");

        let payloads = dispatcher.dispatch(&event).expect("dispatch enabled hook");

        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].target.name, "audit-log");
        assert!(payloads[0]
            .payload
            .contains("\"kind\":\"before_tool_call\""));
        assert_eq!(payloads[1].target.name, "operator-stream");

        let run_started = HookEvent::run_started(SessionId::new(), RunId::new());
        let run_payloads = dispatcher
            .dispatch(&run_started)
            .expect("enabled dispatcher handles all events");
        assert_eq!(run_payloads.len(), 2);
        assert!(run_payloads[0].payload.contains("\"kind\":\"run_started\""));
    }

    #[test]
    fn settings_default_disabled_and_match_rules_by_event_tool_agent_and_status() {
        let session_id = SessionId::new();
        let run_id = RunId::new();
        let settings = HookSettings::enabled(vec![
            HookRule::new(HookEventKind::BeforeToolCall)
                .with_matcher(HookMatcher::ToolNameExact("sandbox.exec".to_owned()))
                .with_target(HookTarget::memory("tool-audit")),
            HookRule::new(HookEventKind::AgentStopped)
                .with_matcher(HookMatcher::AgentIdExact("planner".to_owned()))
                .with_target(HookTarget::memory("agent-audit")),
            HookRule::new(HookEventKind::RunStopped)
                .with_matcher(HookMatcher::RunStatus(RunStatus::Failed))
                .with_target(HookTarget::memory("failed-runs")),
        ]);
        let dispatcher = HookDispatcher::from_settings(settings);

        assert!(!HookSettings::default().summary().enabled);

        let tool_payloads = dispatcher
            .dispatch(&HookEvent::before_tool_call(
                session_id,
                run_id,
                "sandbox.exec",
            ))
            .expect("dispatch matching tool hook");
        assert_eq!(tool_payloads.len(), 1);
        assert_eq!(tool_payloads[0].target.name, "tool-audit");

        let ignored_tool_payloads = dispatcher
            .dispatch(&HookEvent::before_tool_call(
                session_id,
                run_id,
                "gateway.chat",
            ))
            .expect("dispatch non-matching tool hook");
        assert!(ignored_tool_payloads.is_empty());

        let agent_payloads = dispatcher
            .dispatch(&HookEvent::agent_stopped(session_id, "planner"))
            .expect("dispatch matching agent hook");
        assert_eq!(agent_payloads.len(), 1);
        assert_eq!(agent_payloads[0].target.name, "agent-audit");

        let status_payloads = dispatcher
            .dispatch(&HookEvent::run_stopped(
                session_id,
                run_id,
                RunStatus::Failed,
            ))
            .expect("dispatch matching status hook");
        assert_eq!(status_payloads.len(), 1);
        assert_eq!(status_payloads[0].target.name, "failed-runs");

        let summary = dispatcher.settings().summary();
        assert!(summary.enabled);
        assert_eq!(summary.rule_count, 3);
        assert_eq!(summary.target_count, 3);
    }

    #[test]
    fn command_targets_are_sandboxed_and_receive_payload_on_stdin() {
        use loom_sandbox::{Sandbox, SandboxPolicy};

        let command = stdin_echo_command();
        let sandbox = Sandbox::new(SandboxPolicy::default().allow_command(command.program()));
        let dispatcher = HookDispatcher::from_settings(HookSettings::enabled(vec![HookRule::new(
            HookEventKind::BeforeToolCall,
        )
        .with_target(HookTarget::command("command-audit", command))]));
        let event = HookEvent::before_tool_call(SessionId::new(), RunId::new(), "sandbox.exec");

        let deliveries = dispatcher
            .dispatch_with_sandbox(&event, &sandbox)
            .expect("command hook runs through sandbox");

        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].target.kind, HookTargetKind::Command);
        let output = deliveries[0]
            .command_output
            .as_ref()
            .expect("command target records output");
        assert!(output.status_success);
        assert!(output.stdout.contains("before_tool_call"));
        assert!(output.stdout.contains("sandbox.exec"));
    }

    #[test]
    fn command_targets_remain_denied_without_sandbox_allow_policy() {
        use loom_sandbox::Sandbox;

        let dispatcher = HookDispatcher::from_settings(HookSettings::enabled(vec![HookRule::new(
            HookEventKind::BeforeToolCall,
        )
        .with_target(HookTarget::command("command-audit", stdin_echo_command()))]));
        let event = HookEvent::before_tool_call(SessionId::new(), RunId::new(), "sandbox.exec");

        let error = dispatcher
            .dispatch_with_sandbox(&event, &Sandbox::default())
            .expect_err("default sandbox denies command hook");

        assert!(error.to_string().contains("denied"));
    }

    #[test]
    fn malformed_command_target_returns_error_instead_of_panicking() {
        use loom_sandbox::Sandbox;

        let malformed_target = HookTarget {
            kind: HookTargetKind::Command,
            name: "missing-command".to_owned(),
            command: None,
        };
        let dispatcher = HookDispatcher::from_settings(HookSettings::enabled(vec![HookRule::new(
            HookEventKind::BeforeToolCall,
        )
        .with_target(malformed_target)]));
        let event = HookEvent::before_tool_call(SessionId::new(), RunId::new(), "sandbox.exec");

        let error = dispatcher
            .dispatch_with_sandbox(&event, &Sandbox::default())
            .expect_err("malformed command target should be reported");

        assert!(error.to_string().contains("missing command"));
    }

    #[cfg(windows)]
    fn stdin_echo_command() -> loom_sandbox::SandboxCommand {
        loom_sandbox::SandboxCommand::new("cmd")
            .arg("/C")
            .arg("findstr sandbox.exec")
    }

    #[cfg(not(windows))]
    fn stdin_echo_command() -> loom_sandbox::SandboxCommand {
        loom_sandbox::SandboxCommand::new("grep").arg("sandbox.exec")
    }
}
