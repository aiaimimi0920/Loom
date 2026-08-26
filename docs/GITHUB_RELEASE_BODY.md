## Loom V0.2.0

This is the first refactored Loom release. It publishes the desktop workbench with
aligned `V0.2.0` metadata and the existing provenance, SBOM, checksum, and
smoke-verification gates.

### Highlights

- Keeps the daemon-first runtime and desktop shell version-aligned at `0.2.0`.
- Preserves the framework-first Art bridge and the hardened plugin/security
  boundaries from the refactored codebase.
- Uses the reproducible tagged-release workflow with draft-first publication and
  exact asset verification.

### Package notes

The public Release intentionally keeps the download surface small. Desktop users
should download `Loom-V0.2.0-windows-x64.zip`; users who want to verify the download
should also download its matching `.zip.sha256` file. The CLI and Plugin SDK are
still built and verified by the pipeline for maintainers, but are not uploaded to
the public Release. SBOMs, provenance, manifests, and full checksum inventories
remain maintainer-side evidence. GitHub also provides automatic source ZIP and
tarball links for the tag.

**Full Changelog**: [V0.1.0...V0.2.0](https://github.com/aiaimimi0920/Loom/compare/V0.1.0...V0.2.0)
