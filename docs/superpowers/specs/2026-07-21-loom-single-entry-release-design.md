# Loom Single-Entry Release Design

## Goal

Make the Windows desktop release present one user-facing Loom entry while
preserving the daemon as an independent process for Hook, Tea, scripts, Docker,
and headless operation. The desktop package must not require users to choose
among three top-level executables.

## Approved Layout

The desktop candidate keeps the UI executable at its root and groups all
daemon-owned runtime files under `runtime`:

```text
Loom.exe
runtime/
  loom-daemon.exe
  resources/ocr/*
  bin/python-embed/*
  python/*
packages/
  Loom-{versionId}-windows-x64.zip
  Loom-{versionId}-windows-x64.zip.sha256
  Loom-CLI-{versionId}-windows-x64.zip
  Loom-CLI-{versionId}-windows-x64.zip.sha256
```

`loom-desktop.exe` remains the source build target, but the release builder
publishes it as `Loom.exe`. `loom-daemon.exe` remains a real sidecar process and
is copied to `runtime/loom-daemon.exe`. The CLI binary is never copied into the
desktop package root; it is published only inside the separate CLI ZIP.

## Runtime Resolution

The desktop shell resolves the daemon in this order:

1. `LOOM_DAEMON_EXECUTABLE`, when explicitly configured;
2. `runtime/loom-daemon.exe` beside the packaged `Loom.exe`;
3. the development `target/debug/loom-daemon.exe` path.

The daemon's existing executable-relative resource discovery remains valid by
placing its OCR, embedded Python, and Python Art resources under the same
`runtime` directory as the daemon. No duplicate resource tree is created.

## Artifacts and Manifest

The standalone release builder continues to accept the existing `-OutputRoot`
boundary and produces one versioned candidate directory. The manifest changes
from three top-level executables to two desktop executables:

- `Loom.exe`
- `runtime/loom-daemon.exe`

The manifest adds an explicit CLI artifact record and distinguishes the desktop
payload ZIP from the CLI ZIP. The verifier checks both ZIPs, their sidecar
hashes, the two desktop executable paths, and all runtime support files. The
desktop ZIP contains only the desktop payload; the CLI ZIP contains only
`loom.exe`.

## Compatibility and Non-Goals

- The daemon HTTP API, port, environment variables, and process boundary do not
  change.
- Docker remains daemon-first and continues to publish the daemon as its server
  entrypoint; it does not acquire the desktop shell.
- The CLI command semantics do not change.
- Existing source build names are retained to avoid unnecessary Cargo/Tauri
  package churn.
- This change does not merge daemon logic into Tauri or create a universal
  multi-mode executable.

## Validation

The change is accepted only when all of the following hold:

- desktop sidecar unit tests require `runtime/loom-daemon.exe`;
- layout and release contracts reject root-level `loom.exe` and
  `loom-desktop.exe` in the desktop candidate;
- the release dry run catalogs the two desktop executables and separate CLI
  artifact;
- the verifier accepts the new candidate and rejects missing/misplaced files;
- packaged desktop, persistence, Gateway, concurrency, OCR, Python, and CLI
  smokes pass;
- the final desktop ZIP and CLI ZIP both have deterministic checksum records;
- no unrelated Neuro parent paths are modified.
