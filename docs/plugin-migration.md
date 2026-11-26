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

Loom installs packages strictly into immutable
`<frameworks|arts>/<publisher>/<id>/versions/<version>-<digest>` directories with
`active.json`, `locks`, `state`, `cache`, and `outputs`. Legacy flat install roots
are retired and are not migrated automatically. Code directories remain read-only;
mutable runtime files must use the execution-context directories.

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
6. Execute through HTTP and the `loom.hook.v1` Otter Bridge.
7. Upgrade, rollback, restart, disable/enable, uninstall/reinstall.
8. Verify Loom and Otter source fingerprints did not change.

Retired AHRP routes and legacy built-in script adapters are removed; all
Desktop-authored and external packages build as canonical Art ZIPs and install
through the same verified immutable lifecycle.
