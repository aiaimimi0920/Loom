//! Agent definitions and resolution for Loom.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version of the agent crate.
pub const LOOM_AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Agent definition parsing and catalog errors.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent definition is missing YAML frontmatter")]
    MissingFrontmatter,
    #[error("agent frontmatter is invalid: {0}")]
    InvalidFrontmatter(#[from] serde_yaml::Error),
    #[error("agent `{id}` already exists in `{scope:?}` scope")]
    DuplicateScope { id: String, scope: AgentScope },
    #[error("agent `{0}` was not found")]
    NotFound(String),
}

/// Result alias for agent-definition operations.
pub type AgentResult<T> = Result<T, AgentError>;

/// Defines where an agent spec was loaded from.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentScope {
    Project,
    User,
}

/// Optional cognitive role metadata for specialized agent specs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    CourtroomJudge,
    CourtroomAdvocate,
    CourtroomCritic,
    MoaProposer,
    MoaSynthesizer,
}

/// Tool permission metadata parsed from agent frontmatter.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolPolicy {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Markdown/YAML-frontmatter agent contract used by Loom.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSpec {
    pub id: String,
    pub name: String,
    pub scope: AgentScope,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub role: Option<AgentRole>,
    #[serde(default)]
    pub tools: ToolPolicy,
    #[serde(skip)]
    pub system_prompt: String,
}

impl AgentSpec {
    /// Loads an agent spec from Markdown with leading YAML frontmatter.
    pub fn from_markdown(markdown: &str) -> AgentResult<Self> {
        let (frontmatter, body) = split_frontmatter(markdown)?;
        let mut spec: Self = serde_yaml::from_str(frontmatter)?;
        spec.system_prompt = body.trim().to_owned();
        Ok(spec)
    }
}

/// Catalog that resolves project-scoped specs before user-scoped specs.
#[derive(Debug, Default)]
pub struct AgentCatalog {
    specs: BTreeMap<String, BTreeMap<AgentScope, AgentSpec>>,
}

impl AgentCatalog {
    pub fn add(&mut self, spec: AgentSpec) -> AgentResult<()> {
        let scoped = self.specs.entry(spec.id.clone()).or_default();
        if scoped.contains_key(&spec.scope) {
            return Err(AgentError::DuplicateScope {
                id: spec.id,
                scope: spec.scope,
            });
        }

        scoped.insert(spec.scope, spec);
        Ok(())
    }

    pub fn resolve(&self, id: &str) -> AgentResult<&AgentSpec> {
        let scoped = self
            .specs
            .get(id)
            .ok_or_else(|| AgentError::NotFound(id.to_owned()))?;

        scoped
            .get(&AgentScope::Project)
            .or_else(|| scoped.get(&AgentScope::User))
            .ok_or_else(|| AgentError::NotFound(id.to_owned()))
    }

    #[must_use]
    pub fn effective_agents(&self) -> Vec<&AgentSpec> {
        self.specs
            .keys()
            .filter_map(|id| self.resolve(id).ok())
            .collect()
    }
}

fn split_frontmatter(markdown: &str) -> AgentResult<(&str, &str)> {
    let markdown = markdown.strip_prefix('\u{feff}').unwrap_or(markdown);
    let rest = markdown
        .strip_prefix("---\r\n")
        .or_else(|| markdown.strip_prefix("---\n"))
        .ok_or(AgentError::MissingFrontmatter)?;

    if let Some(index) = rest.find("\r\n---\r\n") {
        let frontmatter = &rest[..index];
        let body = &rest[index + "\r\n---\r\n".len()..];
        return Ok((frontmatter, body));
    }

    if let Some(index) = rest.find("\n---\n") {
        let frontmatter = &rest[..index];
        let body = &rest[index + "\n---\n".len()..];
        return Ok((frontmatter, body));
    }

    Err(AgentError::MissingFrontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_AGENT: &str = r#"---
id: planner
name: User Planner
scope: user
model: gateway:gpt-5
tools:
  allow:
    - workflow.read
    - memory.search
  deny:
    - shell.exec
---
You are the user-level planning agent.
"#;

    const PROJECT_AGENT: &str = r#"---
id: planner
name: Project Planner
scope: project
model: gateway:gpt-5.5
tools:
  allow:
    - workflow.read
    - workflow.write
  deny: []
---
You are the project-level planning agent.
"#;

    #[test]
    fn markdown_frontmatter_loads_agent_spec_and_body_prompt() {
        let spec = AgentSpec::from_markdown(USER_AGENT).expect("parse user agent");

        assert_eq!(spec.id, "planner");
        assert_eq!(spec.name, "User Planner");
        assert_eq!(spec.scope, AgentScope::User);
        assert_eq!(spec.model.as_deref(), Some("gateway:gpt-5"));
        assert_eq!(spec.tools.allow, vec!["workflow.read", "memory.search"]);
        assert_eq!(spec.tools.deny, vec!["shell.exec"]);
        assert_eq!(spec.system_prompt, "You are the user-level planning agent.");
    }

    #[test]
    fn project_scope_overrides_user_scope_for_same_agent_id() {
        let user = AgentSpec::from_markdown(USER_AGENT).expect("parse user agent");
        let project = AgentSpec::from_markdown(PROJECT_AGENT).expect("parse project agent");
        let mut catalog = AgentCatalog::default();

        catalog.add(user).expect("add user agent");
        catalog.add(project).expect("add project agent");

        let resolved = catalog.resolve("planner").expect("resolve planner");
        assert_eq!(resolved.scope, AgentScope::Project);
        assert_eq!(resolved.name, "Project Planner");
        assert_eq!(
            resolved.tools.allow,
            vec!["workflow.read", "workflow.write"]
        );
    }

    #[test]
    fn effective_agents_are_deterministically_sorted_after_scope_resolution() {
        let mut catalog = AgentCatalog::default();
        catalog
            .add(AgentSpec::from_markdown(USER_AGENT).expect("parse user planner"))
            .expect("add user planner");
        catalog
            .add(
                AgentSpec::from_markdown(
                    r#"---
id: reviewer
name: Reviewer
scope: user
tools:
  allow: []
  deny: []
---
Review implementation quality.
"#,
                )
                .expect("parse reviewer"),
            )
            .expect("add reviewer");
        catalog
            .add(AgentSpec::from_markdown(PROJECT_AGENT).expect("parse project planner"))
            .expect("add project planner");

        let effective: Vec<_> = catalog
            .effective_agents()
            .into_iter()
            .map(|agent| (agent.id.clone(), agent.scope))
            .collect();

        assert_eq!(
            effective,
            vec![
                ("planner".to_owned(), AgentScope::Project),
                ("reviewer".to_owned(), AgentScope::User),
            ]
        );
    }

    #[test]
    fn missing_frontmatter_is_rejected() {
        let error = AgentSpec::from_markdown("No frontmatter here").expect_err("must fail");
        assert!(matches!(error, AgentError::MissingFrontmatter));
    }

    #[test]
    fn courtroom_and_moa_roles_can_be_represented_as_agent_specs() {
        let judge = AgentSpec::from_markdown(
            r#"---
id: courtroom.judge
name: Judge
scope: project
role: courtroom_judge
tools:
  allow:
    - workflow.read
  deny:
    - shell.exec
---
Weigh competing agent arguments and produce a final ruling.
"#,
        )
        .expect("parse judge");

        let synthesizer = AgentSpec::from_markdown(
            r#"---
id: moa.synthesizer
name: MoA Synthesizer
scope: project
role: moa_synthesizer
tools:
  allow:
    - memory.search
  deny: []
---
Synthesize multiple model outputs into a single response.
"#,
        )
        .expect("parse synthesizer");

        assert_eq!(judge.role, Some(AgentRole::CourtroomJudge));
        assert_eq!(synthesizer.role, Some(AgentRole::MoaSynthesizer));
    }
}
