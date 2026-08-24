# Loom Repository Instructions

These instructions apply to this repository root and every subdirectory. They
exist so every new coding-agent conversation started in Loom follows the same
module-size and hardening rules.

When Loom is mounted in the Neuro workspace, the expanded common baseline lives
at `../docs/DEVELOPMENT_STANDARD.md`. That common policy grandfathers untouched
legacy debt, but Loom has already established a repository-wide strict checker,
baseline, and exception schema. The stricter Loom rules below therefore remain
authoritative for every changed or new Loom file; the Neuro incremental policy
must not be used to bypass a Loom strict-gate failure. This file is self-contained
for standalone Loom checkouts.

## Required development guidance

Before feature work, refactoring, or test/tooling changes, read:

- `CONTRIBUTING.md`
- `docs/DEVELOPMENT.md`
- `docs/DEPENDENCY_SECURITY.md` for dependency, build, CI, or release changes
- the owning architecture or protocol document for the subsystem being changed

The machine-authoritative size policy is:

- `scripts/effective-code-lines-policy.json`
- `scripts/effective-code-lines.mjs`
- `scripts/effective-code-lines-exceptions.json`

If prose and the checker disagree, obey the checker and fix the stale prose in
the same change.

## Mandatory effective-code-line limits

The checker removes blank lines, comment-only lines, and comment-only multiline
regions. A code line with an inline comment counts as code. Do not use physical
line counts as a substitute.

- Target about 150 effective lines; 100-250 is the normal design range.
- 251-500 is acceptable only for one clear responsibility.
- 501-700 is a reviewed soft exception, not the default. Either split the file
  or keep an exact, unexpired entry in
  `scripts/effective-code-lines-exceptions.json`.
- 701-1500 requires a split before the task is complete. No final exception is
  allowed.
- More than 1500 is a hard-cap violation with no waiver. Continue splitting
  until every result is at most 700 effective lines.

These rules apply to handwritten production code, tests, scripts, and styles.
Generated or immutable third-party files are excluded only by the explicit
policy. Do not game the metric with comment padding, strings, generated files,
minification, or generic dumping-ground modules.

Changing a 501-700 file invalidates its recorded source hash. The task must
either reduce it to 500 or fewer lines or update the exception with its exact
line count, source hash, responsibility, cohesion reason, owner, independent
approver, review date, and protecting tests. An agent must not invent approval.

## Required workflow for every development task

1. Measure relevant files before designing the change.
2. Characterize public behavior, serialization, events, side effects, state,
   concurrency, and resource lifetimes with focused tests.
3. Design modules by responsibility and dependency direction. Preserve public
   facades where they are real boundaries and use the narrowest visibility.
4. Extract structure without intentional behavior changes. Add concise comments
   for purpose and non-obvious invariants; do not add comment padding.
5. Run focused tests and direct dependent compile/typecheck gates.
6. Run the strict effective-line gate. A task is not complete while any changed
   or new file violates the limits or has a stale exception.
7. Review each resulting file for security, vulnerability, resource lifetime,
   memory bounds, cancellation/cleanup, and performance. Confirmed findings need
   a fix and regression evidence.
8. Run official formatters, relevant subsystem gates, and `git diff --check`.
9. If a manifest, lockfile, build image, GitHub Action, or release workflow
   changes, run the dependency security contract and real OSV scan. Do not add
   an exception without an exact advisory ID, evidence-based reason, expiry of
   at most 90 days, a real owner, and independent review. An agent must not
   invent that approval.

Minimum effective-line commands from the repository root:

```powershell
node --test .\scripts\tests\effective-code-lines.test.mjs
node .\scripts\effective-code-lines.mjs --mode strict --json .\.tmp\effective-code-lines.json
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DependencySecurityContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-DependencySecurityScan.ps1
```

For the full workflow, exception schema, hardening checklist, and Loom gates,
follow `docs/DEVELOPMENT.md`. For vulnerability response, lockfile coverage,
temporary exceptions, and release security gates, follow
`docs/DEPENDENCY_SECURITY.md`.
