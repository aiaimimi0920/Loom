# Loom Plugin Development

Loom frameworks and Arts are independently built packages. The supported ABI
is process plus JSON; repository source, Rust crate internals, Hook internals,
and desktop implementation details are not plugin APIs.

## Install the SDK

Download `Loom-Plugin-SDK-<version>-windows-x64.zip`. It contains:

- `loom-plugin.exe`;
- `protocol/README.md`;
- all 14 public v1 JSON Schemas, including capability package/runtime/extension;
- the clean-host capability conformance harness under `scripts/`;
- Rust, TypeScript, and Python capability templates plus a fake host under `sdk/capability/`;
- signing, security, permission, migration, and provenance documentation.

## Create a framework

```powershell
.\loom-plugin.exe init framework .\my-framework my-framework
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
.\loom-plugin.exe init art .\my-art my-art publisher.example/my-framework
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

Every normal HTTP, Hook Bridge, and AHRP Art execution creates durable run
evidence and returns an `executionId` when the compatibility response permits
additional fields.

## Authoring schemas

Framework `authoringSchema` fields support `string`, `number`, `boolean`,
`enum`, `path`, `secret`, and `json`. Desktop builds the Art form dynamically.
Secret fields store a credential binding name, not the secret value. The six
official authoring modes are compatibility fallbacks when an installed
framework has no schema.

## Compatibility

Keep `loom.framework.v1` and advertise it in `supportedProtocolVersions`.
Ignore unknown optional request fields. Do not parse Loom application versions
to infer ABI behavior. New transports require a new negotiated protocol name.
