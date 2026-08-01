# Plugin Signing and Trust

Loom signs framework and Art directories with Ed25519.

## Key generation and signing

```powershell
.\loom-plugin.exe keygen .\publisher-key.json release-key-1
.\loom-plugin.exe sign .\package .\publisher-key.json publisher.example
```

Signing adds publisher/signature metadata to the manifest and writes the
detached `signature.json`. The detached document contains the algorithm, key
ID, canonical SHA-256 digest, public key, and Ed25519 signature.

Keep the generated private key outside published packages and releases. The
public key may be distributed through a trust record.

## Trust store

```powershell
.\loom-plugin.exe trust add `
  .\plugin-trust.json publisher.example .\publisher-key.json

.\loom-plugin.exe validate .\package --trust-store .\plugin-trust.json
```

Expected states:

- `Unsigned`: no signature metadata.
- `Verified`: cryptographically valid, key not in this trust store.
- `Trusted`: valid and matched to a non-revoked trust record.
- `Revoked`: valid signature from a revoked `(publisherId, keyId)`.
- invalid signatures return an error rather than a usable state.

## Revocation

```powershell
.\loom-plugin.exe trust revoke `
  .\plugin-trust.json publisher.example release-key-1
```

Revoked packages fail CLI validation with that trust store. Loom rechecks
revocation for framework readiness, Art integrity, and rollback; an old version
does not become safe merely because it was installed before revocation.

## Runtime policy

```text
LOOM_PLUGIN_TRUST_POLICY=allow-unsigned   # compatibility default
LOOM_PLUGIN_TRUST_POLICY=require-signed
LOOM_PLUGIN_TRUST_POLICY=require-trusted # production recommendation
```

Package SHA-256 sidecars protect transport, while signatures bind package bytes
to a publisher key. Use both.
