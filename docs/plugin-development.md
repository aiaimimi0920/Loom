# Loom Plugin Development

Loom frameworks and Arts are independently built packages. The supported ABI
is process plus JSON; repository source, Rust crate internals, Hook internals,
and desktop implementation details are not plugin APIs.

## Install the SDK

Download `Loom-Plugin-SDK-<version>-windows-x64.zip`. It contains:

- `loom-plugin.exe`;
- `protocol/README.md`;
- public v1 JSON Schemas, including capability package/runtime/extension;
- the clean-host capability conformance harness under `scripts/`;
- Rust, TypeScript, and Python capability templates plus a fake host under `sdk/capability/`;
- signing, security, permission, migration, and provenance documentation.

## Create a framework

```powershell
.\loom-plugin.exe init framework .\my-framework my-framework publisher.example
```

Replace the generated runtime placeholder with the process named by
`framework.manifest.json`. Implement the normative stdin/stdout contract from
`protocol/README.md`. Keep protocol output on stdout and logs on stderr.

Validate and exercise the real executable:

```powershell
.\loom-plugin.exe validate .\my-framework
.\loom-plugin.exe conformance `
  .\my-framework\runtime\my-framework.exe `
  my-framework `
  .\my-art
```

## Create an Art

```powershell
.\loom-plugin.exe init art .\my-art my-art publisher.example/my-framework publisher.example
```

An Art owns its `manifest.json`, `art.runtime.json`, runtime entry, resources,
and optional dependency declarations. The framework host reads these files; it
must not require a new Loom enum variant or a Hook source branch.

## Sign, trust, and pack

```powershell
.\loom-plugin.exe keygen .\publisher-key.json release-key-1
.\loom-plugin.exe sign .\my-framework .\publisher-key.json publisher.example
.\loom-plugin.exe sign .\my-art .\publisher-key.json publisher.example
.\loom-plugin.exe trust add .\plugin-trust.json publisher.example .\publisher-key.json
.\loom-plugin.exe validate .\my-art --trust-store .\plugin-trust.json
.\loom-plugin.exe pack .\my-framework .\my-framework.zip
.\loom-plugin.exe pack .\my-art .\my-art.zip
```

`pack` refuses missing payload entries, unsafe paths, links, excessive package
size/count, and case-insensitive collisions. It writes a SHA-256 sidecar.

## Exercise an unknown capability package

The SDK conformance harness generates, signs, installs, invokes, upgrades,
rolls back, and uninstalls a publisher-qualified capability that is unknown to
the host. It requires a Rust toolchain plus a built `loom-daemon.exe`; it does
not require a Loom or Hook source checkout.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomCapabilityPluginConformance.ps1 `
  -PublisherId publisher.example `
  -PackageId capability-a `
  -DaemonExecutable C:\path\to\loom-daemon.exe `
  -PluginCliExecutable .\loom-plugin.exe
```

Repository maintainers can additionally pass `-AuditSourceIsolation` together
with explicit `-LoomRepository` and `-HookRepository` paths. That optional mode
proves that the lifecycle did not mutate either source tree; it is not a plugin
runtime dependency.

From the extracted SDK, verify all three language templates without Loom or
Hook source:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\sdk\capability\Test-Templates.ps1
```

## Install and operate

Framework packages install through `POST /v1/frameworks/install`. Art packages
install through `POST /v1/arts/install`. Publisher-qualified IDs use `%2F` in a
single URL path segment. Installed packages support enable/disable, immutable
upgrade, verified rollback, packaging, and uninstall.

Use:

```http
GET /v1/doctor/frameworks
GET /v1/doctor/arts
GET /v1/diagnostics/executions/{runId}
GET /v1/support-bundle?runId={runId}
```

HTTP and `loom.hook.v1` Art execution create durable run evidence. AHRP,
ArtLoom routes and unnamespaced bridge methods are retired; they are not alternate
plugin execution entrypoints. Response fields follow the public protocol schemas.

## Authoring schemas

Framework `authoringSchema` fields support `string`, `number`, `boolean`,
`enum`, `path`, `secret`, and `json`. Desktop builds the Art form dynamically.
Secret fields store a credential binding name, not the secret value. Prefer the
installed framework's schema over assumptions about a fixed set of authoring modes.

## Canonical package scope

Installed Framework and Art packages require publisher identity. Art manifests
also require a matching `metadata.art.qualifiedId`. Package code is immutable
under `<frameworks|arts>/<publisher>/<id>/versions/<version>-<digest>`; activation,
locks and writable state follow the [public package contract](../protocol/README.md).
No flat install roots, latest-package copies or old layout migration are provided.
The Art Store serves exact versioned ZIPs with matching digest sidecars.

The host selects and locks the publisher-qualified framework; the selected
framework's process ABI uses its manifest-local ID. These two identity scopes
must not be confused. Generic internal `ToolDefinition` values can lack a
publisher, and a registry query may resolve a bare local ID when it has exactly
one match. Neither rule permits a publisher-less installed package or a short-ID
Hook catalog alias. Native `core.image.*` operations are not installed packages.

The official package families are `process`, `cloud_api`, `mcp` and `workflow`.
Commands, PowerShell and Python Arts use the process framework; adding a third-party
Art does not require an Art-ID execution branch in Loom or Hook. Package operations
must not edit either repository's source.

`art-packages/samples` and `art-packages/surface-prototypes` have distinct catalogs.
The latter are separately built test/development packages; a passing prototype
smoke does not imply that every prototype ships in the formal sample catalog.
Use the actual release manifest to establish payload membership and trust state.
Bundled availability or a checksum alone does not establish a trusted signature.

Workflow child packages stay independent and are not reference-counted or
automatically garbage-collected when a parent is removed; see
[workflow dependencies](WORKFLOW_CONTRACT.md). Hosted marketplace operation,
payment/licensing and remote publisher governance are outside the local package
platform's implementation boundary. OS sandbox and Unix key-storage limits remain
explicit in [security](plugin-security.md) and [permissions](plugin-permissions.md).

## Compatibility

Keep `loom.framework.v1` and advertise it in `supportedProtocolVersions`.
Ignore unknown optional request fields. Do not parse Loom application versions
to infer ABI behavior. New transports require a new negotiated protocol name.
