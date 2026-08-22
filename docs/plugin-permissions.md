# Loom Plugin Permissions

Framework manifests declare a structured `permissionPolicy`.

## Vocabulary

- `network.domains`: intended remote domain allowlist. It names the hosts the Art
  or tool itself calls. It deliberately does not constrain the image URLs an MCP
  image-search result points at, because those are CDN hosts chosen by the
  upstream service rather than hosts the package can declare.
- `network.allowLocalhost`: explicit local-development access. It defaults to
  `false` for cloud Arts, for an MCP tool's image downloads, and for the host's
  own HTTP client, so an Art or tool that talks to a local service — a model
  server, Hook, the Loom daemon — has to declare it.
- `network.allowPrivateNetworks`: private network declaration.
- `cloudApi.timeoutMs`: deadline a cloud Art asks for when the caller states
  none. The default is 30 s and the host ceiling is 600 s.
- `filesystem.read` / `filesystem.write`: logical scopes such as `inputs`,
  `packageResources`, `state`, `cache`, and `outputs`.
- `process.spawn` / `process.maxProcesses`: descendant process capability.
- `gpu`: GPU access declaration.
- `clipboard`: clipboard access declaration.
- `credentials`: names requested from the credential broker.

## Enforcement matrix

| Boundary | Current enforcement |
| --- | --- |
| Package/resource path | Canonical containment and immutable package root |
| Cloud multipart upload | Author-declared `{{inputs.x.path}}` bindings only, with canonical containment in the Art package, control plane, or staged input root |
| Cloud request templating | Endpoint substitutions percent-encoded with a fixed-authority check; header and JSON body substitutions inserted as values into the parsed template |
| MCP image download | Candidate URLs chosen by the MCP server are fetched under the tool's own `allowLocalhost` / `allowPrivateNetworks` declaration, with the whole candidate loop bounded by one wall-clock budget and an attempt cap |
| State/cache/output | Dedicated writable directories outside version code |
| Timeout/stdout/stderr | Enforced on Windows and Unix |
| Memory/active process count | Windows Job Object enforced; Unix declared only |
| stdout/stderr | Bounded capture with truncation/error diagnostics |
| Cancellation/drop | Whole managed process tree termination |
| Host HTTP/download | HTTPS/DNS/IP/redirect/domain/size policy |
| Credential names/scopes/expiry | Brokered and enforced |
| Credential storage | Windows DPAPI current-user plus owner-only DACL; Unix owner-only `0700` directory/`0600` file with a reversible local-file fallback |
| Direct plugin network | Declared/audited; not fully OS-denied |
| Direct arbitrary filesystem | Declared/audited; not fully OS-denied |
| GPU/clipboard | Declared/audited; not fully OS-denied |

`LOOM_PLUGIN_PERMISSION_MODE=audit` is the compatibility default. It permits
launch while reporting requested permissions and the matrix above through the
Desktop and `GET /v1/doctor/frameworks`.

`LOOM_PLUGIN_PERMISSION_MODE=strict` fails closed before framework self-test or
Art execution when the manifest requests direct network, arbitrary filesystem,
GPU, or clipboard access. An unknown non-empty mode is a configuration error and
also prevents readiness/execution. Strict mode does not pretend those
capabilities are sandboxed; it rejects packages that require them.

The manifest is not permission by assertion. A caller should review requested
permissions and trust state before installation. Loom forwards only granted
credential values and package-scoped paths in the execution context. It does
not expose host environment secrets.

## Credentials

Credentials are stored by `(name, framework scope, Art scope)`. A grant is
returned only when:

1. the framework manifest requests the name;
2. the stored framework scope matches, when present;
3. the stored Art scope matches, when present;
4. the credential is not expired.

Desktop and API list operations return name, scope, expiration, and protection
method only. The value is write-only after submission. Credential files use
create-new temporary files, durable atomic replacement, and owner-only
permissions. On Unix, `local-file-base64` is deliberately named as a reversible
local fallback rather than OS-backed secret encryption; deployments requiring
an OS keyring should inject credentials from an external broker and avoid
persisting them through that fallback.

The loopback discovery manifest can contain a bearer token and is therefore
treated as a sensitive credential file: Loom writes it through a create-new
temporary, atomically replaces the prior manifest, and restricts the directory
and file to the current owner (plus `SYSTEM` on Windows).

## Recommended deployment policy

- Require trusted signatures for production.
- Prefer narrow Art-scoped credentials.
- Use brokered HTTP/MCP for network plugins.
- Reject or isolate plugins whose direct OS access cannot be accepted.
- Treat permission expansion on upgrade as a new security review.
