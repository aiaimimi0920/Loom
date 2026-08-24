# Loom Dependency Security

This document is Loom's operational policy for disclosed vulnerabilities,
unmaintained dependencies, and suspected malicious packages. The goal is a
repeatable control loop: inventory, detect, assess, contain, remediate, verify,
and publish new evidence. A green scan is a release gate, not a claim that the
software has no unknown vulnerability.

## 1. Deployed controls

Loom combines controls that cover different boundaries:

- Dependabot opens weekly Cargo, npm, GitHub Actions, and Docker update pull
  requests from `.github/dependabot.yml`.
- OSV-Scanner 2.5.0 scans the exact committed lockfiles on pull requests,
  pushes to `main`, a weekly schedule, manual dispatch, and tag publication.
- GitHub receives a SARIF artifact and publishes the results to the repository
  Code Scanning dashboard with the required `security-events: write` permission.
- CodeQL runs extended security queries against Rust, JavaScript/TypeScript, and
  GitHub Actions sources on pull requests, `main`, a weekly schedule, and manual
  dispatch. It uses supported no-build extraction so the scan is independent of
  packaging and still publishes language-specific Code Scanning results.
- `security/dependency-security-policy.json` pins both the reusable workflow and
  underlying scanner Action commits, local Windows binary URL and SHA-256, scan
  inventory, and exception lifetime.
- `security/osv-scanner.toml` contains only advisory-specific, expiring
  exceptions. Broad package overrides are forbidden.
- Release packaging continues to generate CycloneDX and SPDX SBOMs, checksums,
  and provenance. Container CI uses the SHA-pinned Trivy v0.36.0 Action as a
  separate image/runtime-layer gate, uploads SARIF before enforcing the result,
  and fails for fixed critical or high-severity findings.

The tag workflow scans the exact tag or manually requested ref before any
release job can run. A successful scan of another branch or an earlier commit
does not authorize publication.

## 2. Machine-authoritative inventory

The OSV gate scans these committed resolution files:

1. `Cargo.lock` for the root Rust workspace.
2. `apps/desktop/src-tauri/Cargo.lock` for the detached Tauri wrapper.
3. `framework-packages/runtime-host/Cargo.lock` for the detached runtime host.
4. `apps/desktop/package-lock.json` for the desktop frontend.

The list in `security/dependency-security-policy.json` is authoritative, and
its `maximumExceptionDays` field is the machine-enforced exception ceiling. When a
new package manager, detached manifest, or lockfile is introduced, update the
policy, GitHub workflow, Dependabot configuration, contract, and this document
in the same pull request.

`mcp-server-packages/stock-api/runtime/vendor/stock-api/package.json` is vendored
source without a committed lockfile and is not covered by the lockfile scan. It
currently declares development dependencies rather than installed runtime
dependencies, but changes to that vendored snapshot still require provenance,
license, source review, and package tests. Do not describe it as OSV-covered
unless a reproducible committed dependency resolution is added.

## 3. Local scan

Run the contract first, then the real online advisory scan:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DependencySecurityContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-DependencySecurityScan.ps1
```

The first scan downloads the pinned Windows binary into ignored `.tmp`, checks
its SHA-256, checks its reported version, and writes JSON evidence to
`.tmp/dependency-security/osv-results.json`. A custom scanner path is allowed
only for diagnosis and must still match the pinned SHA-256 and reported version.

Run the scan whenever a manifest or lockfile changes, before a formal release,
and while assessing a newly disclosed advisory. Because the OSV database
changes independently of Loom, scheduled CI may fail without a source change;
that is a new security signal, not flaky CI.

## 4. Triage and priority

Do not prioritize from CVSS alone. Record all of the following in the issue or
pull request:

- affected package, locked version, dependency path, and affected Loom target;
- whether vulnerable code is compiled, shipped, reachable, and attacker
  controlled in the actual deployment;
- exploit maturity, known exploitation, integrity of the package source, and
  exposure of credentials or persisted data;
- upstream fixed version or replacement, compatibility cost, and rollback plan;
- artifacts and release IDs that contain the affected dependency.

Use these response targets:

| Priority | Example | Required response |
| --- | --- | --- |
| P0 | Active compromise, malicious package, or applicable known-exploited RCE | Contain immediately; stop releases and rotate exposed secrets |
| P1 | Reachable critical/high-impact flaw with a practical exploit | Patch or mitigate within 24 hours |
| P2 | Applicable high severity with no active exploitation | Patch within 7 days |
| P3 | Applicable medium/low severity | Patch within 30 days |
| P4 | Unmaintained or currently non-applicable transitive package | Remove, replace, or reapprove before the exception expires |

If applicability is uncertain, use the higher priority until runtime evidence
narrows it. A lockfile presence alone does not prove reachability, while an
absence from one target does not make a cross-platform dependency safe for
another target.

## 5. Remediation workflow

1. Reproduce the scanner finding and preserve its JSON evidence.
2. Identify the owning direct dependency with `cargo tree`, `npm explain`, or
   the relevant package-manager graph.
3. Prefer a compatible upgrade. If unavailable, remove the dependency, replace
   the parent, disable the affected feature, or contain the exposed function.
4. Update manifests and lockfiles through the package manager; never hand-edit
   checksums or resolved package metadata.
5. Run focused regression tests, all compile/typecheck gates for affected
   targets, the dependency contract, and the real OSV scan.
6. Regenerate release SBOM, provenance, manifest, and checksums under a new
   release ID. Existing release evidence is immutable.
7. Record the affected and fixed release range, then notify downstream users
   when the issue applies to a published artifact.

Dependency updates are code changes. Review build scripts, feature activation,
install scripts, native binaries, licenses, and behavior changes rather than
merging solely because a bot opened the pull request.

## 6. Temporary exceptions

An exception is allowed only when no safe immediate fix exists and there is
specific evidence that the current release boundary is not affected or that a
time-bounded risk is accepted. Add one `[[IgnoredVulns]]` block per canonical
advisory ID with:

- the exact advisory ID;
- `ignoreUntil` no more than 90 days in the future;
- a concrete applicability or mitigation reason;
- a tracking issue or pull-request discussion identifying the owner and
  independent reviewer.

The config file cannot prove human approval. The reviewing pull request is the
approval record; an automation agent must not invent an owner or approver.
Aliases are covered by OSV's advisory alias mapping and should not be duplicated.

Before approval, run the unconfigured scan as well as the configured scan to
show exactly what the exception suppresses. Expiry renewal requires fresh
reachability, upstream, and release-target evidence. Never renew automatically.
Linux-only exceptions in the current baseline explicitly block a future Linux
desktop publication until those findings are removed or reassessed.

## 7. Suspected malicious dependency incident

Treat unexpected install scripts, package takeover, checksum drift, typosquat,
or compromised maintainer credentials as a P0 supply-chain incident:

1. Stop dependency installation, builds, releases, and artifact promotion.
2. Preserve logs, lockfiles, package archives, hashes, SBOMs, and runner details.
3. Isolate affected runners and developer hosts. Do not execute the package to
   gather more evidence on a trusted machine.
4. Revoke and rotate credentials reachable from the affected environment,
   including CI, signing, registry, GitHub, and application secrets.
5. Remove or pin away from the package using a trusted source, then rebuild on a
   clean host from a reviewed commit.
6. Compare regenerated SBOM/provenance/checksums, notify affected users, and
   revoke or mark compromised releases rather than silently replacing them.

OSV and Dependabot are disclosure controls, not malware detectors. Provenance,
review, least-privilege CI, immutable artifacts, and incident response remain
necessary even when no advisory exists.

## 8. Current baseline and limitations

The deployment baseline was established on 2026-08-24. Directly fixable
findings were removed by updating the locked versions of `anyhow`,
`crossbeam-epoch`, `quinn-proto`, `tar`, `plist`, and `quick-xml`. Remaining
entries in `security/osv-scanner.toml` are time-bounded transitive maintenance
findings or Linux-only GTK/glib findings not shipped in Loom's formal Windows
desktop release.

This scheme does not prove source code safety, runtime reachability, absence of
zero-days, or safety of untracked/vendored inputs. It also does not replace Rust
tests, frontend tests, plugin trust enforcement, secret scanning, container
scanning, SBOM verification, or clean-source release requirements.
