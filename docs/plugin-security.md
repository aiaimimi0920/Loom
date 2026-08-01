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

## Trust policy

The default `allow-unsigned` policy exists only for compatibility with old local
packages. Production deployments should set:

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
