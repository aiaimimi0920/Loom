# Loom Plugin Security

## Threat model

Plugin packages and remote stores are untrusted input. A package may attempt
path traversal, archive collision, decompression exhaustion, publisher
takeover, signature substitution, dependency confusion, remote binary swap,
private-network access, output flooding, process-tree escape, rollback to a
revoked version, credential disclosure, or source-tree modification.

## Security controls

- Publisher-qualified IDs isolate packages with the same local ID.
- Ed25519 signatures cover a canonical SHA-256 package digest.
- Trust policy supports allow-unsigned, require-signed, and require-trusted.
- Revocation is checked on validation, readiness, rollback, and Art execution.
- ZIP extraction rejects traversal, absolute paths, links, collisions, Windows
  reserved paths/ADS, size/count/path limits, and high compression ratios.
- Remote package and Art binary downloads require bounded responses. Art store
  packages and remote binaries require SHA-256 pins.
- HTTP redirects and DNS results are revalidated; metadata, private,
  link-local, loopback, and special addresses are blocked unless an explicit
  loopback development policy applies.
- Immutable package versions separate code from writable state/cache/output.
- Activation journals recover interrupted installs and quarantine malformed or
  unsafe records.
- Uninstall uses same-parent tombstone renames. Startup restores a tombstone
  when registry removal was not committed, or finishes deletion when it was.
- Lockfiles pin framework/runtime/binary identity, version, and digest.
- Managed process trees enforce timeout and stdout/stderr limits and are
  terminated on timeout, cancellation, or drop. Windows Job Objects additionally
  enforce memory and active-process limits; those two declarations remain
  advisory on Unix process groups.
- Credential list and support/diagnostic APIs never return secret values.

## Outbound peer binding

The shared outbound client validates the exact DNS answer set consumed by its
connector, including IPv4-mapped IPv6 addresses. Empty, mixed forbidden/allowed,
and oversized DNS answers fail closed. A preliminary URL check is not treated
as proof that a later connection reaches an approved peer.

HTTP/HTTPS proxy routes use numeric `CONNECT IP:port` destinations after local
policy validation; target TLS still uses the original URL hostname for SNI and
certificate verification. HTTPS proxies must also pass their own normal TLS
certificate verification. The operator-selected proxy is trusted to honor the
numeric endpoint; this is not remote socket attestation. Proxy failure never
falls back to a direct connection.
Proxy selection and `NO_PROXY` are snapshotted with each protected client, using
the same system matcher as reqwest. Ordinary Gateway proxy clients are unchanged.
Custom proxy URLs are normalized (including IDNA hostnames) before matching;
an unrepresentable custom route fails client creation rather than selecting direct.

A client-owned loopback adapter preserves reqwest's request/streaming interface.
It uses an ephemeral 256-bit credential and strips local proxy credentials before
forwarding HTTP requests. Its process-wide limits are 128 clients and 128 active
connections, with 16 connections per client, 256 KiB/128-field request headers, a 20-second
connection deadline, a 60-second idle/write deadline, and a 15-minute total tunnel
limit. Remaining connection time is divided across approved DNS candidates, with
non-final attempts capped at five seconds so a stalled first address cannot
consume the entire fallback window. Dropping the client lease cancels its listener
and active connections.
The incoming header budget accommodates MCP's 64 configured headers and 128 KiB
aggregate limit plus protocol-managed headers. Upstream CONNECT response headers
retain a separate 16 KiB/64-field limit; target HTTPS headers remain end-to-end encrypted.
No external proxy service or persistent credential is installed.

The Windows native image fallback uses the same adapter for proxy and direct
routes, while keeping Windows TLS validation. Its explicit `IWebProxy` never
bypasses loopback: .NET `WebProxy` otherwise bypasses loopback even with
`BypassProxyOnLocal=false`. Direct routes connect to the already validated numeric
address. The native child and adapter share the download's bounded lifetime.

Protected proxy transport currently requires HTTP/HTTPS proxies; unsupported
proxy schemes fail explicitly rather than silently bypassing the operator's
route. HTTP development endpoints also require the upstream proxy to permit a
CONNECT tunnel to their approved port.

## Trust policy

The current default is `allow-unsigned` for local/development packages. This is
an active trust-policy choice, not permission to accept retired package layouts
or protocol aliases. Production deployments should set:

```text
LOOM_PLUGIN_TRUST_POLICY=require-trusted
```

Trust records are keyed by `(publisherId, keyId)`. A publisher cannot replace a
different publisher's installed package by reusing its local ID.

## Source immutability

Install, execution, upgrade, rollback, disable, uninstall, and crash recovery
operate only below the configured control-plane/evidence roots. They do not edit Loom or Hook source. The independent
plugin-boundary smoke fingerprints both repositories before and after the full
lifecycle and fails if either fingerprint changes.

## OS isolation boundary

The current Windows boundary uses Job Objects; Unix uses process groups. This
reliably bounds and terminates descendants, but it is not a complete
AppContainer, restricted-token filesystem broker, Linux namespace, or seccomp
profile. Direct arbitrary executable access to network, filesystem, GPU, or
clipboard is therefore not claimed as fully OS-denied.

Both Windows process launch paths create the child suspended, configure and
assign its kill-on-close Job Object, and only then resume its verified primary
thread. Assignment or resume failure kills and reaps the child without returning
a usable process. There is no unsupervised fallback. A thread snapshot that does
not identify exactly one owned primary thread is rejected rather than guessed.

Use brokered Cloud API/MCP paths for mediated network access, keep plugin
publishers trusted, review declared permissions, and run high-risk plugins in a
separate OS account/VM until a platform sandbox backend is available. The
daemon doctor and Desktop framework panel expose declared permissions and trust
state so this limitation is operator-visible.

The default permission mode is `audit`. Set
`LOOM_PLUGIN_PERMISSION_MODE=strict` to reject packages requesting the currently
unenforceable direct network/filesystem/GPU/clipboard capabilities before
self-test or execution.

## Reporting

Use `/v1/support-bundle` for a redacted report. It removes passwords, secrets,
authorization values, bearer/basic credentials, tokens, private keys, cookies,
credential values, URL userinfo/query/fragment, and truncates oversized text.
The bundle contains package hashes, trust state, permission declarations, and
selected run evidence, but no raw environment or credential values.
