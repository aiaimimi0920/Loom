# Loom documentation

Use the current functional contracts below when operating or extending Loom.
Implementation, configuration and executable tests remain the source of truth.

## Product and development

- [Product, desktop, daemon and CLI](../README.md): startup, configuration,
  installation, Hook synchronization, request concurrency and run persistence.
- [Architecture](ARCHITECTURE.md): runtime ownership, package boundaries,
  durable events, memory and workflow contracts.
- [Development manual](DEVELOPMENT.md) and [contribution guide](../CONTRIBUTING.md):
  module size, hardening, formatting, testing and release requirements.
- [Agent definitions](AGENT_DEFINITIONS.md) and [workflow contract](WORKFLOW_CONTRACT.md).
- [Gateway integration](GATEWAY_INTEGRATION.md): model access and planner behavior.
- [Asset Library integration](ASSET_LIBRARY_INTEGRATION.md).

## Plugins and integrations

- [Public protocols and schemas](../protocol/README.md).
- [Plugin development](plugin-development.md): framework/Art authoring and SDK usage.
- [Plugin security](plugin-security.md), [permissions](plugin-permissions.md),
  and [signing and trust](plugin-signing-and-trust.md).
- [OCR contract](OCR_CONTRACT.md), [OCR golden dataset](OCR_GOLDEN_DATASET.md),
  and [translation modes](TRANSLATION_HYBRID_MODE.md).
- [Wall guide](TILE_WALL.md): terminal and source ownership, functional contracts
  and the 19 remaining joint acceptance conditions.
- [Wall control API](../protocol/WALL_CONTROL_API.md): paired terminal outputs,
  layouts, image delivery and acknowledgement.

## Packaging and operations

- [Release provenance](release-provenance.md): independent packages, checksums,
  source identity, SDK, attestations and release verification.
- [Dependency security](DEPENDENCY_SECURITY.md): exact-lock inventory and advisory handling.
- [Release body](GITHUB_RELEASE_BODY.md): public release description template.

## Historical records and acceptance

Completed migration phase 0-66 logs, the old migration ledger, migration audits
and task-by-task implementation plans have been removed from the active tree.
They describe earlier candidates, compatibility adapters and packaging layouts;
they must not override current protocols or reinstall retired compatibility paths.
For an exact historical record use the local recovery tag, for example:

```powershell
git show cleanup-base-20260928:docs/progress/phase-40-run-event-persistence.md
```

The remaining phase, planning and acceptance documents are not a claim that
every proposed feature is delivered. In particular, screen-wall multi-computer
acceptance and later native/runtime gates must retain their explicit evidence
boundaries until they are individually reviewed. A source cleanup does not
constitute a new packaged or native acceptance result.
