## Loom V0.2.0

This is the first refactored Loom release. It publishes the desktop workbench and
Plugin SDK with aligned `V0.2.0` metadata and the existing provenance, SBOM,
checksum, and smoke-verification gates.

### Highlights

- Keeps the daemon-first runtime, desktop shell, and Plugin SDK version-aligned at
  `0.2.0`.
- Preserves the framework-first Art bridge and the hardened plugin/security
  boundaries from the refactored codebase.
- Uses the reproducible tagged-release workflow with draft-first publication and
  exact asset verification.

### Package notes

The public Release intentionally keeps the download surface focused. Desktop users
should download `Loom-V0.2.0-windows-x64.zip`; Plugin and Art node developers should
download `Loom-Plugin-SDK-V0.2.0-windows-x64.zip`. Each ZIP has a matching `.zip.sha256`
sidecar for integrity verification. The CLI remains built and verified by the
pipeline for maintainers, but is not uploaded to the public Release. SBOMs,
provenance, manifests, and full checksum inventories remain maintainer-side
evidence. GitHub also provides automatic source ZIP and tarball links for the tag.

**Full Changelog**: [V0.1.0...V0.2.0](https://github.com/aiaimimi0920/Loom/compare/V0.1.0...V0.2.0)
