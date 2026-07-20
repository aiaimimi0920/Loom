# Loom Standalone Repository Design

## Status

The design direction was approved by the user on 2026-07-20. This written
spec is the review gate before implementation. This commit does not create a
remote repository, initialize nested Git, replace the parent directory with a
submodule, or push credentials.

## Goal

Turn Loom from an ordinary directory inside the Neuro monorepo into an
independently maintained public repository, while keeping Neuro able to consume
Loom through the same Git submodule/gitlink architecture already used by Hook.

A fresh Loom clone must build and validate its Rust workspace, desktop shell,
release package, smoke contracts, documentation, and Docker daemon/CLI image
without depending on parent-only scripts or files. The Neuro parent must keep
its existing Loom release boundary:

C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

## Baseline and Safety

The verified source backup is:

C:\Users\Public\nas_home\AI\GameEditor\Neuro\_temp\Loom-standalone-backup-20260720-195938-be4bbb7b

It contains 134 core files and approximately 58.7 MB. Each included file was
compared by relative path, byte length, and SHA-256. The excluded paths are
regenerable caches only:

- Loom/target
- Loom/apps/desktop/node_modules
- Loom/apps/desktop/dist
- Loom/apps/desktop/src-tauri/target

Existing release/Loom candidates remain immutable.

## Selected Architecture

Create the public repository:

https://github.com/aiaimimi0920/Loom.git

The parent Neuro repository records it in .gitmodules and tracks Loom as a
160000 gitlink. The standalone repository owns all Loom source, docs, examples,
resources, build scripts, smoke contracts, release metadata, and Actions.

A mirror without a parent gitlink is rejected because it creates two sources of
truth. Removing Loom from the parent entirely is rejected because it breaks the
existing Neuro checkout and release workflows.

## Repository Layout

The standalone root contains:

    .github/workflows/
    apps/daemon
    apps/cli
    apps/desktop
    crates/
    docs/analysis
    docs/plan
    docs/progress
    docs/superpowers
    examples/
    resources/
    scripts/tests/
    Cargo.toml
    Cargo.lock
    Dockerfile
    LICENSE
    README.md
    CONTRIBUTING.md
    .gitignore

Generated target, node_modules, dist, local release output, and runtime evidence
are ignored and never committed.

## Documentation Consolidation

Move or copy parent-owned Loom documents into the standalone repository:

- docs/loom/analysis -> Loom/docs/analysis
- docs/loom/plan -> Loom/docs/plan
- docs/loom/progress -> Loom/docs/progress
- docs/loom/superpowers -> Loom/docs/superpowers

Merge collisions only after comparing the existing Loom docs and the parent
progress/spec history. Update links and commands so they work from both a
standalone clone and a Neuro parent checkout. Historical evidence paths remain
historical and are not silently rewritten.

## Script Consolidation

The standalone repository owns Loom-specific replacements for:

- parent scripts/build-release-exes.ps1
- parent scripts/verify-release.ps1
- parent scripts/smoke-release-local-apps.ps1
- parent scripts/tests/test-loom-desktop-shell-contract.ps1
- parent scripts/tests/test-loom-artloom-parity-contract.ps1
- all existing Loom/scripts/Invoke-*Smoke.ps1 and Test-*Contract.ps1

Each script resolves the repository root from PSScriptRoot and accepts explicit
input/output paths. A standalone clone has a deterministic local release
default. The Neuro parent passes the absolute release/Loom destination required
by this workspace. CI writes only inside its workspace and artifact staging
area.

The Tauri sibling-daemon lookup must stop relying on the hard-coded
C:\release\Loom path and prefer the executable location plus explicit
configuration overrides.

## Parent Integration

The parent migration has only these responsibilities:

1. Add the Loom URL to .gitmodules.
2. Replace the ordinary Loom directory index entry with a 160000 gitlink.
3. Update parent wrappers and documentation that assume Loom is ordinary files.
4. Keep release/Loom and historical candidates unchanged.
5. Leave Hook, Gateway, Platform, Tea, Talk, and unrelated changes untouched.

Parent operators use:

    git submodule update --init --recursive Loom

The parent does not keep a second copied source tree.

## Standalone Git History

Because the current source is interleaved with a dirty monorepo and parent-owned
docs/scripts must be consolidated, use a verified snapshot import:

1. Build a staging tree under _temp.
2. Consolidate and test the staging tree.
3. Initialize Git in the staging tree.
4. Add a provenance document recording the parent commit and backup manifest.
5. Create one auditable baseline commit.
6. Validate a second clean clone.
7. Create and push the public repository.
8. Clone the published commit into the parent Loom path and record the gitlink.

The parent history is not rewritten. The baseline commit is traceable to the
parent commit and the verified backup manifest.

## GitHub Actions

Add these workflows:

### ci.yml

Run on pull requests and pushes to main:

- pinned Rust toolchain;
- cargo fmt check;
- cargo check and cargo test with locked dependencies;
- Windows daemon/binary contracts;
- Node setup, npm ci, desktop typecheck and build;
- Tauri cargo check;
- no secrets.

### build-windows.yml

Run on main pushes and manual dispatch:

- build loom.exe, loom-daemon.exe, and loom-desktop.exe;
- stage embedded Python, OCR, and declared support files;
- run formal verification;
- upload a versioned Windows x64 artifact;
- use contents read and artifact-only permissions.

### release-tag.yml

Run for tags matching V*.*.* and manual tag dispatch:

- validate the tag;
- repeat the Windows build, formal verifier, and packaged smoke matrix;
- publish the ZIP and SHA-256 sidecar as a GitHub Release;
- use the Actions-provided GITHUB_TOKEN with contents write only in the
  release job;
- never use or print a personal PAT.

### docker.yml

Build and validate the existing Linux daemon/CLI Docker image on relevant
changes and manual dispatch. It does not claim to be the full Windows desktop
distribution.

All workflows use explicit permissions, fail-fast artifact checks, and no
credential output.

## Release Contract

The standalone manifest records version, source Git head, repository dirty
state, scoped source dirty state, source path policy, executable/support hashes,
ZIP/sidecar hashes, build commands, and logs.

The payload ZIP contains exactly the declared executable/support files.
Manifest, checksums, build information, logs, and ZIP sidecars remain outer
candidate metadata. The parent wrapper can direct output to the required
Neuro/release/Loom path; the standalone default remains local and deterministic.

## Authentication

No personal access token is committed, placed in an Action, written to config,
or passed as a persistent command-line argument. Remote creation and push use
an existing credential helper, a newly rotated process-only environment token,
or an interactive credential flow.

The PAT pasted into the conversation is treated as compromised and must not be
reused. Before remote creation, verify whether the target repository already
exists, use the least required scope, and redact all credential-related output.

## Acceptance Criteria

The migration is accepted only when:

1. The verified backup manifest remains available.
2. A clean standalone clone builds without the Neuro parent.
3. Standalone Rust, desktop, Tauri, Windows contract, and packaged smoke gates
   pass.
4. The release manifest and checksums verify with scoped provenance.
5. New workflow YAML is valid and its commands reproduce locally or in CI.
6. A clean parent clone initializes the Loom submodule and runs the documented
   integration command.
7. The parent git tree records Loom as 160000 at the published commit.
8. No sibling project changes enter the migration commits.
9. A fresh public clone reproduces the release without a personal PAT.

## Migration Sequence

1. Consolidate Loom docs/scripts and remove parent path assumptions in staging.
2. Add standalone ignore rules, license/provenance docs, scripts, and Actions.
3. Initialize and baseline-commit the staging repository.
4. Validate a second clean clone and a standalone release.
5. Confirm safe credentials and create/push aiaimimi0920/Loom.
6. Clone the published commit into the parent Loom path and update .gitmodules.
7. Run parent integration, standalone CI-equivalent gates, packaging, and
   submodule reproducibility checks.
8. Commit parent integration separately from standalone repository history.

Failures return to staging or the verified backup. No destructive reset,
forced checkout, or recursive deletion is used.

## Non-goals

- Rewriting Loom runtime behavior unrelated to repository ownership.
- Moving Gateway, Hook, Tea, Talk, or Platform source into Loom.
- Creating a personal-token deployment workflow.
- Deleting historical release candidates.
- Making the Docker daemon image a full desktop distribution.
- Claiming cross-platform desktop support without a separate validation matrix.
