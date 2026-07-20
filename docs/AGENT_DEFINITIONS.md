# Loom Agent Definitions

`loom_agent` defines the v1 agent specification format and catalog resolution
rules.

## File format

Agent definitions are Markdown files with leading YAML frontmatter:

```markdown
---
id: planner
name: Default Planner
scope: project
model: gateway:gpt-5
role: courtroom_judge
tools:
  allow:
    - workflow.read
  deny:
    - shell.exec
---
System prompt text goes here.
```

Required frontmatter fields:

- `id`
- `name`
- `scope`

Optional fields:

- `model`
- `role`
- `tools.allow`
- `tools.deny`

The Markdown body after frontmatter becomes `system_prompt`.

## Scope precedence

`AgentScope` supports:

- `project`
- `user`

`AgentCatalog::resolve(id)` always prefers a project-scoped spec over a
user-scoped spec with the same `id`. `effective_agents()` returns deterministic
resolution results sorted by agent id.

Duplicate definitions for the same `(id, scope)` are rejected.

## Roles

Current cognitive role metadata:

- `courtroom_judge`
- `courtroom_advocate`
- `courtroom_critic`
- `moa_proposer`
- `moa_synthesizer`

Roles are metadata only in the v1 runtime. They allow Courtroom/MoA concepts to
be represented without hard-coding a separate execution engine.

## Tool policy

`tools.allow` and `tools.deny` are declarative metadata for later tool routing
and sandbox policy integration. They do not bypass `loom_sandbox`; process
execution remains deny-by-default unless the sandbox policy explicitly allows a
command.

## Example agents

Canonical sample agents live in:

- `Loom/examples/agents/default.md`
- `Loom/examples/agents/writer.md`
- `Loom/examples/agents/reviewer.md`

The CLI lists effective agents with:

```powershell
.\target\debug\loom.exe agents list --examples-dir .\examples
```
