# Loom Installable Frameworks and Art Store Closure Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Loom's installable framework plus framework-gated Art installation feature so the runtime-selection contract, daemon/store APIs, desktop client coverage, and project docs are aligned.

**Architecture:** `loom_tool_registry::framework` remains the source of truth for framework install/readiness, `loom_tool_registry::install` keeps enforcing that an Art can only install when its framework is installed and ready, and the Python Art runtime resolver must prefer a framework-provisioned runtime before packaged or ambient fallbacks. The daemon and desktop keep exposing the same `/v1/frameworks` and `/v1/arts/store/*` surface, while docs record the feature as a dedicated tracked phase.

**Tech Stack:** Rust 2021, serde/serde_json, reqwest blocking client, Node test runner, TypeScript, PowerShell 5.1, Loom release scripts.

---

## File Map

Create:

```text
docs/progress/phase-45-installable-frameworks-art-store.md
```

Modify:

```text
crates/loom_tool_registry/src/lib.rs
apps/desktop/src/services/loomApi.test.ts
docs/progress/MASTER.md
README.md
```

---

## Tasks

- [ ] **Task 1: Fix Python Art framework runtime precedence in `loom_tool_registry`**
  - Reconcile the new framework-runtime-first tests with the production helper so `resolve_python_executable_from(...)` and `resolve_python_executable()` share the same precedence order.
  - Validate with `cargo test -p loom_tool_registry framework -- --nocapture` and then `cargo test -p loom_tool_registry -- --nocapture`.

- [ ] **Task 2: Add desktop API regression coverage for framework and art-store routes**
  - Extend `apps/desktop/src/services/loomApi.test.ts` so the framework list/install/uninstall helpers and art-store catalog/install helpers are covered through fetch-based route assertions.
  - Validate with `npm test --prefix apps/desktop`.

- [ ] **Task 3: Record the feature in project docs and regenerate a Loom release**
  - Add a dedicated progress entry for installable frameworks and framework-gated Art installation.
  - Update `docs/progress/MASTER.md` and `README.md` so users can discover the feature.
  - Validate with a parent-scoped release build:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-installable-frameworks `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
```
