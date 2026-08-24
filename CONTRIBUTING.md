# Contributing to Loom

Loom is a Rust workspace with a React/Tauri desktop application. Keep changes
inside the subsystem that owns the behavior and preserve the boundaries
documented in `docs/ARCHITECTURE.md`.

## Prerequisites

- Rust 1.95.0 with Cargo and rustfmt
- Node.js 22 with npm
- PowerShell 5.1 or newer on Windows
- Tauri v2 platform prerequisites for desktop builds
- Docker when changing the container image or daemon packaging

## Setup

Run commands from the repository root:

```powershell
cargo fetch --locked
Push-Location .\apps\desktop
npm ci
Pop-Location
```

## Code Size and Modularity

All feature, refactor, test, script, and style changes must follow the effective
code-line and post-split hardening rules in `docs/DEVELOPMENT.md`.

The preferred module size is about 150 effective lines. A cohesive file may be
up to 500 lines without an exception. Files from 501 through 700 lines require
an exact, unexpired cohesion exception; files above 700 must be split, and files
above 1500 never receive a waiver. Effective lines exclude blank and
comment-only lines and are measured only by the repository checker.

Run the checker tests and strict gate before submitting a change:

```powershell
node --test .\scripts\tests\effective-code-lines.test.mjs
node .\scripts\effective-code-lines.mjs --mode strict --json .\.tmp\effective-code-lines.json
```

Changing a 501-700 file invalidates its recorded source hash. Split it to 500 or
fewer lines, or update `scripts/effective-code-lines-exceptions.json` with exact
current evidence and a real independent approval. Do not invent approvals or
use comments, generated files, minification, or generic helper modules to evade
the metric.

## Validation

Run the checks that cover the files you changed. Before opening a pull request,
run the complete local gate when the required platform dependencies are
available:

```powershell
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace

Push-Location .\apps\desktop
npm ci
npm run typecheck
npm run build
Pop-Location

cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneLayout.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DevelopmentManualContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DependencySecurityContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-GitHubActionsContract.ps1
docker build -t loom:local .
```

PowerShell release and workflow contracts live under `scripts/tests`. Run all
of them after changing packaging or GitHub Actions.

## Dependency Security

Follow `docs/DEPENDENCY_SECURITY.md` whenever a manifest, lockfile, build image,
GitHub Action, vendored package, or release dependency changes. Loom scans the
root Rust workspace, detached Tauri wrapper, detached framework runtime host,
and desktop npm lockfiles. Do not assume that a detached manifest is covered by
the root workspace scan.

Before submitting a dependency change, run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DependencySecurityContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-DependencySecurityScan.ps1
```

OSV exceptions are advisory-specific, evidence-based, independently reviewed,
and limited to 90 days. Never add a broad package override or invent an
approval. A bot-generated update still requires source, build-script, license,
behavior, compilation, and regression review.

## Release Validation

Standalone packages default to `release\Loom` inside this repository. A parent
checkout can provide its required destination explicitly:

```powershell
.\scripts\build-release.ps1 -VersionId local-check
.\scripts\build-release.ps1 `
  -VersionId parent-check `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
```

The Windows desktop candidate has exactly one user-facing executable at its
root: `Loom.exe`. The daemon and its resources belong under `runtime\`, with
the daemon at `runtime\loom-daemon.exe`. The CLI is not copied into that
candidate root; release automation publishes it as a separate
`Loom-CLI-<versionId>-windows-x64.zip` containing only `loom.exe`. Both the
desktop and CLI ZIPs must have matching `.sha256` sidecars and pass
`scripts\verify-release.ps1`.

Verification also rejects stale or extra root executables, mismatched CLI
manifest/artifact metadata, ZIP payload drift, malformed sidecar content, and
any file omitted from `checksums.sha256`. Keep the synthetic tamper contract
green when changing release layout or packaging scripts.

Docker follows a different, intentional server boundary. It remains
daemon-first and keeps the CLI inside the image for operator scripts; it does
not package the desktop shell.

Do not commit generated packages, `target`, `node_modules`, desktop `dist`,
runtime evidence, credentials, or local environment files.

## Contributions and Licensing

Keep commits focused and include tests for behavior changes. By contributing,
you agree that your contribution may be distributed under either Apache-2.0
or MIT, at the recipient's option, as described in `LICENSE`.
