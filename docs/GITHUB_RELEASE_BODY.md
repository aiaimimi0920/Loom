## Loom V0.2.2

This release unifies authenticated Otter bridge connectivity, live projection, outbound/inbound MCP runtime capabilities, and multi-edition server/desktop packaging while tightening device, deserialization, and process-containment security boundaries.

### Highlights

- **Authenticated Otter Bridge & Multi-Device Hub**: strengthens native TLS 1.3 bridge identity verification, connection lifecycles, two-level host/app-session device hierarchy, multi-device Otter workflow sync, and reserved NAccount Cloud Relay configuration (cloud transport is not implemented). Upgrade alongside the matching Otter release.
- **Three Build Editions & Embedded Web Console**: supports `desktop` (Windows `Loom.exe` + sidecar daemon), `web` (`loom-daemon` with embedded `/ui/` management console + `loom` CLI), and `api` (headless `loom-daemon` + `loom` CLI) across Windows and Linux Docker deployments.
- **Outbound & Inbound MCP Ecosystem**: exposes Loom capabilities to external AI clients over `POST /mcp` and `loom mcp-stdio` guarded by fine-grained Scoped Share Tokens, alongside cross-platform packaged MCP server runtimes.
- **Live Projection & Hybrid Workflow Execution**: refines H.264/JPEG/raw BGRA media continuity and fallback rules, plus dynamic cross-end workflow step relay between server compute and paired terminals.
- **Runtime & Security Hardening**: enforces bounded deserialization before JSON parsing on model/broker responses, header-only upload admission, device pairing/challenge budgets, and managed child-process tree isolation on Windows (Job Objects) and Linux (Bubblewrap + `pidfd`).
- **Unified Versioning & Provenance**: aligns the desktop shell, workspace crates, and `Loom-Plugin-SDK` at `0.2.2` with draft-first tag verification, CycloneDX/SPDX SBOMs, and GitHub build attestations.

### Downloads and compatibility

- **Windows Desktop**: download `Loom-V0.2.2-windows-x64.zip`, verify against `.zip.sha256`, extract, and launch `Loom.exe`.
- **Plugin & Art SDK**: download `Loom-Plugin-SDK-V0.2.2-windows-x64.zip` (with `.zip.sha256`) for `loom-plugin`, public v1 JSON Schemas, templates, and documentation.

Loom enforces canonical-only wire protocols and publisher-qualified package layouts; retired legacy aliases are not accepted. Optional capabilities such as `neuro.official/ocr` and `text-translation` remain separately installed and permission-approved, and local translation models are configured independently rather than bundled into the base installer.

**Full Changelog**: [V0.2.1...V0.2.2](https://github.com/aiaimimi0920/Loom/compare/V0.2.1...V0.2.2)
