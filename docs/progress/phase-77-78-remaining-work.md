# Phase 77-78 remaining development plan

Date: 2026-08-23

Source records:

- `docs/progress/phase-77-stock-pysnowball-freshness.md`
- `docs/progress/phase-78-post-baseline-review.md`
- `docs/progress/phase-78-lane-sync.md`

This file remains a forward-looking queue and therefore keeps only work that is not complete. R1-R12
were removed after implementation and fresh repository/release validation. Phase 77 has no remaining
item.

## R13 - F10 clean-source joint release

Status: **release blocked by shared dirty worktrees**.

The product and release paths are runtime-validated, and both repositories now have an explicit
clean-source gate. The current shared Loom and Hook worktrees nevertheless contain owned and unowned
changes from concurrent development. Bulk staging, deleting scratch files, or creating commits without
that ownership decision would violate the repository safety boundary. Development artifacts cannot make
that source state reproducible.

### Current development candidates (not F10 releases)

- Loom: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260823-phase78-remaining-work-r78`
  - `verify-release.ps1 -RunSmoke` passed all seven release smoke groups and checked 57 files.
  - `manifest.json` records `gitDirty=true` and Git HEAD
    `74db71ae25c0295e7c4aa3e292bda282c84ebd80`.
- Hook: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260823-phase78-remaining-work-r93`
  - The built EXE reports `hook 0.1.7`; `--self-check` reports `status=ok`.
  - `build-provenance.json` records the EXE digest, Git HEAD
    `5c7d6da4b7bdeddcc95f7e79598403be77b01dcf`, and `gitDirty=true`.
  - The portable ZIP contains the matching provenance file and release notices.

The older r76/r91 candidates remain untouched. The diagnostic r77/r92 candidates created during this
closeout are superseded by r78/r93 and must not be promoted.

### Enforced gates

- Loom `build-release.ps1 -RequireCleanSource` rejects the current tree before creating a release
  destination: `Formal Loom release requires a clean, readable Git worktree. gitDirty=True`.
- Hook `build-local-hook-exe.ps1 -RequireCleanSource` rejects the current tree before compilation with
  the equivalent `gitDirty=True` result.
- Hook's tag workflow passes `-RequireCleanSource` for both the portable build and reviewed UIAccess
  signing candidate.
- Hook local builds emit `build-provenance.json`; portable packaging refuses a missing or digest-mismatched
  provenance file and embeds the verified file in the ZIP.

### Remaining work

1. Assign ownership for every current Loom and Hook modification, then create coherent commits in each
   independent repository. Do not bulk-stage another agent's changes.
2. Review each untracked scratch, cache, crash, and generated path before deleting it or adding an ignore
   rule.
3. Require empty `git status --porcelain --untracked-files=all` output in both repositories.
4. From those reviewed clean commits, build new unused release IDs. Do not overwrite any existing release
   directory.
5. Build Loom with `build-release.ps1 -RequireCleanSource`, then run
   `verify-release.ps1 -RunSmoke -RequireCleanSource`. Require manifest and provenance `gitHead` values to
   match the reviewed commit and require `gitDirty=false`.
6. Run Hook's reviewed tag/candidate flow from its reviewed commit. Require the portable and reviewed
   candidate digests to match their public provenance records; keep this distinct from a future signed
   installer claim.
7. Re-run Hook lint, type checks, complete frontend/browser/Rust tests, production build, and the built
   EXE `--version`/`--self-check` contract from the clean checkout.

F10 is complete only when both new candidates are reproducible from the recorded clean commits and all
runtime gates pass. Artifact existence, a dirty-source provenance record, or a successful development
smoke is not sufficient.
