# Plugin Migration Guide

## From built-in execution code

Move framework-specific behavior out of Loom into a framework process package.
Keep Loom integration limited to `ToolExecution::FrameworkArt { framework }`
and the public JSON ABI. Do not add a new host enum variant for each third-party
framework.

## From a flat Art directory

Package:

```text
manifest.json
art.runtime.json
runtime and resources
```

Loom migrates compatible legacy flat installs into an immutable
`versions/<version>-<digest>` directory and creates `active.json`, `locks`,
`state`, `cache`, and `outputs`. Code becomes read-only; mutable files must move
to the execution-context directories.

## Publisher namespace

Add publisher metadata and use `publisher/id` as the canonical identity. Bare
IDs remain usable only while they resolve to one installed publisher. Clients
must URL-encode the slash as `%2F` in path routes.

## Dependencies

Replace ambient runtime assumptions with manifest dependencies and semantic
version requirements. Pin downloaded binaries by SHA-256. Lockfiles are host
generated; do not edit them to force a version.

## Credentials

Replace embedded keys, environment lookups, and manifest secret values with
named credential bindings. Submit values to the credential store and request
only the names needed by the framework.

## Validation sequence

1. `loom-plugin validate`.
2. `loom-plugin conformance` against the real process.
3. `loom-plugin sign` and validate with a trust store.
4. `loom-plugin pack`.
5. Install into a fresh control plane.
6. Execute through HTTP, Hook Bridge, and AHRP as applicable.
7. Upgrade, rollback, restart, disable/enable, uninstall/reinstall.
8. Verify Loom and Hook source fingerprints did not change.

Legacy execution paths remain for compatibility during migration, but new
Desktop-authored content is built as an Art ZIP and installed through the same
validator/immutable lifecycle as external packages.
