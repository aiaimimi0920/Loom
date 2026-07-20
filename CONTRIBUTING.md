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
docker build -t loom:local .
```

PowerShell release and workflow contracts live under `scripts/tests`. Run all
of them after changing packaging or GitHub Actions.

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

Docker follows a different, intentional server boundary. It remains
daemon-first and keeps the CLI inside the image for operator scripts; it does
not package the desktop shell.

Do not commit generated packages, `target`, `node_modules`, desktop `dist`,
runtime evidence, credentials, or local environment files.

## Contributions and Licensing

Keep commits focused and include tests for behavior changes. By contributing,
you agree that your contribution may be distributed under either Apache-2.0
or MIT, at the recipient's option, as described in `LICENSE`.
