# Loom Development Manual

This manual defines the permanent code-size, modularization, and post-split
hardening rules for Loom. It applies to feature code, bug fixes, refactors,
tests, scripts, release tooling, and styles.

The historical implementation record is
`docs/progress/phase-79-large-file-modularization-hardening.md`. This document is
the day-to-day development contract. The machine-authoritative implementation
is:

- `scripts/effective-code-lines.mjs`
- `scripts/effective-code-lines-lexer.mjs`
- `scripts/effective-code-lines-policy.json`
- `scripts/effective-code-lines-exceptions.json`

If a prose rule drifts from the checker, follow the checker and correct the
documentation in the same change.

## 1. Effective code lines

Use the repository checker rather than physical line counts. An effective code
line is a line containing code after the language-aware lexer removes:

- blank or whitespace-only lines;
- comment-only lines;
- multiline comment regions that contain no code.

A line containing code plus an inline comment counts as one effective line.
String, raw-string, template, regex, and here-string content remains code. A URL
containing `//` is not treated as a comment.

The policy currently scans handwritten Rust, TypeScript/TSX,
JavaScript/JSX/MJS/CJS, CSS/SCSS, HTML, PowerShell, Python, CMD, and BAT files.
It covers production code, tests, build/release scripts, and styles. Generated
output and immutable third-party source are excluded only through explicit,
reviewed entries in `scripts/effective-code-lines-policy.json`.

Do not exclude an inconvenient directory merely because it is large. Do not
move code into strings, generated files, minified forms, comments, or broad
`helpers`, `common`, or `utils` modules to evade the metric.

## 2. Size thresholds

| Effective lines | Classification | Required action |
| ---: | --- | --- |
| About 150 | Design target | Normally aim for 100-250 lines with one responsibility. |
| 251-500 | Acceptable | Keep only when the file has one clear, cohesive responsibility. |
| 501-700 | Soft exception | Split, or record why another boundary would damage cohesion or obscure an invariant. |
| 701-1500 | Mandatory split | The owning task is incomplete until every result is at most 700 lines. |
| More than 1500 | Hard-cap violation | No waiver. Continue splitting, including a second split when necessary. |

The 150-line target is a design heuristic, not a quota. Do not split a cohesive
300-line state machine into arbitrary fragments just to approach 150. A
140-line file with unrelated responsibilities can still require a split.

No new file may exceed 700 effective lines. No modified file may remain above
700. A large baseline or historical exception never permits new growth.

## 3. Designing a split

Split by ownership and invariants, not by equal line counts. Before editing:

1. Identify public symbols, serialization fields, command/event names, feature
   and platform branches, state ownership, side effects, concurrency, and
   resource lifetimes.
2. Select or add focused behavior tests that protect those contracts.
3. Write one sentence of responsibility for every destination module.
4. Define dependency direction. Shared code moves only when at least two real
   owners need it.
5. Keep public facades when they are actual package, protocol, IPC, or UI
   boundaries. Use private or narrow package visibility for extracted details.

Prefer responsibility names such as `validation`, `transport`, `persistence`,
`lifecycle`, `rendering`, or a domain term. Avoid generic dumping grounds.

The structural extraction should not intentionally change behavior. Keep
hardening or performance changes separately reviewable when practical, then
rerun the extraction's focused tests after each intentional change.

Add concise comments for:

- module purpose and ownership;
- cross-module or protocol invariants;
- security and trust assumptions;
- platform constraints;
- non-obvious lifetime or performance decisions.

Comments must reduce maintenance cost. Comment padding is prohibited even
though comment-only lines are excluded from the effective count.

## 4. The 501-700 exception process

A 501-700 file is allowed only when another split would reduce cohesion,
separate an ordering-sensitive invariant, or create a worse public dependency.
Convenience, schedule pressure, or familiarity with a large file is not a valid
reason.

Every exception in `scripts/effective-code-lines-exceptions.json` must contain:

- repository-relative `path`;
- exact `effectiveLines`;
- exact normalized-source `sourceSha256`;
- one clear `responsibility`;
- a specific `reason` that explains why another boundary is harmful;
- `owner`;
- an independent `approvedBy` value different from the owner;
- an unexpired `reviewBy` date in `YYYY-MM-DD` format;
- non-empty protecting `tests`.

Changing the source invalidates the recorded hash even when the line count does
not change. The developer must either split the file to 500 or fewer lines or
refresh the exception with current evidence. Coding agents must not fabricate
an approver; request a real decision when an exception is necessary.

An exception cannot cover a file at 700 lines or fewer if its recorded count or
hash is stale, cannot cover a file at 500 lines or fewer, and can never cover a
file above 700 lines. Expired exceptions fail the gate.

## 5. Required feature workflow

Every feature, fix, or refactor follows this sequence:

1. **Measure first.** Generate a report and inspect every file the design is
   likely to touch.
2. **Characterize behavior.** Establish focused tests and identify observable
   contracts before moving code.
3. **Design ownership.** Define destination responsibilities, invariants, and
   dependency direction.
4. **Extract safely.** Preserve behavior, serialization, events, resource
   ownership, and public facades.
5. **Document invariants.** Add useful purpose and invariant comments without
   padding.
6. **Run focused gates.** Compilation alone is not evidence of preserved
   behavior.
7. **Run the strict size gate.** New and changed files must comply; update a
   legitimate 501-700 exception when required.
8. **Audit each resulting file.** Review security, vulnerability, memory and
   resource lifetime, cancellation, cleanup, and performance.
9. **Fix confirmed findings.** Add regression tests or runtime measurements;
   do not record a code-inspection guess as a completed leak or performance fix.
10. **Run owning-subsystem and final gates.** Include formatters and
    `git diff --check`.

## 6. Post-split security review

Review each destination file at its owning boundary:

- validate HTTP bodies and headers, IPC/events, JSON/serde, CLI arguments,
  environment data, package ZIPs, paths, URLs, images/resources, plugins, and
  MCP data;
- check traversal, symlink/reparse escape, unsafe extraction, archive bombs,
  TOCTOU, permission checks, identity, digest, signature, revocation, and atomic
  activation;
- check command construction, shell use, inherited environment, injection,
  secret exposure, and complete process-tree termination;
- check credentials and tokens in exact, encoded, nested, logged, persisted,
  error, and response forms;
- check Surface/browser origin, source, URL, HTML/script, message-port, CSP,
  resource-budget, and stale-revision boundaries;
- audit Rust `unsafe`, FFI/COM handles, integer conversions, byte lengths, image
  dimensions, and allocation arithmetic;
- keep trust, signing, capability, and permission failures fail-closed.

## 7. Memory and resource lifetime review

For Rust and native code, inspect process and thread joins, channel shutdown,
sockets and response bodies, files and temporary paths, SQLite transactions,
locks, COM/D3D objects, image buffers, cancellation, and all error paths.

For frontend code, inspect subscriptions, WebSocket and MessagePort closure,
timers and animation frames, AbortController use, object/Blob URL revocation,
DOM/GPU cleanup, stale promises, and effect teardown.

Bound queues, caches, retries, concurrent work, response bodies, decoded text,
image dimensions, and intermediate representations. Avoid keeping simultaneous
full-size byte, UTF-16 string, and parsed-object copies when a bounded or
streaming representation is practical.

Suspected leaks require teardown or repeated-use evidence when runtime
measurement is feasible.

## 8. Performance review

Measure before and after changing a hot path. Inspect:

- lock scope and synchronous work on UI or request threads;
- repeated serialization, parsing, validation, and filesystem scans;
- process or MCP reconnection;
- SQLite transaction width;
- queue and daemon backpressure;
- event coalescing and write frequency;
- frontend derived state and rerender breadth;
- GPU and other native-resource cleanup.

Do not make speculative micro-optimizations. Add a stable threshold only when a
meaningful measurement can protect it.

## 9. Commands and completion gates

Run from the Loom repository root. The strict local size gate is:

```powershell
node --test .\scripts\tests\effective-code-lines.test.mjs
node .\scripts\effective-code-lines.mjs `
  --mode strict `
  --json .\.tmp\effective-code-lines.json
```

CI may retain a ratchet command for migration compatibility, but new local
feature work must pass strict mode. A task is incomplete when the report has a
violation, a changed exception is stale, or any new/changed file exceeds 700.

Per-batch minimum evidence is:

- official formatter or checker for every touched language;
- focused unit or contract tests;
- direct dependent compile or typecheck;
- strict effective-line gate;
- security, lifetime, and performance regressions added by confirmed findings;
- `git diff --check` and a scoped review of the final diff.

The Loom full source gate is:

```powershell
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --workspace --locked

Push-Location .\apps\desktop
npm test
npm run typecheck
npm run build
cargo check --locked --manifest-path .\src-tauri\Cargo.toml
Pop-Location
```

Run the relevant repository-owned PowerShell contracts and semantic smoke tests
for the changed subsystem. Process exit zero alone is insufficient when the
smoke is intended to prove a user-visible, persistence, protocol, security, or
release contract.

## 10. Review checklist

Before declaring a task complete, confirm:

- [ ] relevant files were measured with the repository checker;
- [ ] every new or changed file is at most 700 effective lines;
- [ ] every 501-700 file has a current, exact, independently approved exception;
- [ ] responsibilities and dependency direction are clear;
- [ ] useful invariant comments were added without padding;
- [ ] focused behavior and direct dependent gates passed;
- [ ] each resulting file received security, lifetime, and performance review;
- [ ] confirmed findings have fixes and regression evidence;
- [ ] strict effective-line tests and report passed;
- [ ] official formatters and `git diff --check` passed;
- [ ] actual commands, results, and remaining reviewed exceptions are reported.
