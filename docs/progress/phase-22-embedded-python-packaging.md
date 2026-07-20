# Phase 22: Embedded Python Packaging

## Goal

Restore old ArtLoom's release-layer embedded Python packaging contract so
packaged Loom can run `.py` script-backed tools without depending on host PATH
Python.

## Tasks

- [x] P22.1 Source audit and runtime boundary
  - Acceptance: source-backed audit identifies old ArtLoom embedded Python
    layout, launcher behavior, current Loom packaging gaps, and the Phase 22
    layered recovery boundary.
  - Evidence:
    - `docs/loom/analysis/phase-22-embedded-python-packaging-audit.md`
      records old `bin/python-embed`, `python/Launcher.py`,
      `python_engine.rs`, `mcp_engine.rs`, dist layout, current Loom gaps, and
      non-goals.
- [x] P22.2 Embedded Python resources and launcher
  - Acceptance: Loom stages a minimal embedded Python runtime and a Loom-named
    Python launcher resource.
  - Evidence:
    - `Loom/resources/python-embed/` contains `python.exe`, `pythonw.exe`,
      `python3.dll`, `python312.dll`, `python312.zip`, `python312._pth`,
      `LICENSE.txt`, `vcruntime140.dll`, `vcruntime140_1.dll`, and
      `site-packages/.loom-keep`.
    - `Loom/resources/python/Launcher.py` keeps the old launcher request and
      plugin entrypoint contract without visible old product naming.
    - Packaged Python was smoke-checked with `import sys,json`.
    - The launcher was smoke-checked with a temporary plugin and returned
      `status = 200`.
- [x] P22.3 Python script resolver parity
  - Acceptance: `.py` script-backed tools prefer `LOOM_PYTHON`, then packaged
    `bin/python-embed/python.exe`, then safe development/PATH fallbacks.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry resolve_python_executable --offline -- --nocapture`
      passed.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script --offline -- --nocapture`
      passed.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon script --offline -- --nocapture --test-threads=1`
      passed.
- [x] P22.4 Packaged release Python smoke
  - Acceptance: regenerated Loom release smoke proves a `.py` script-backed
    tool uses the packaged Python executable when `LOOM_PYTHON` is unset.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-embedded-python-phase22 -Force`
      generated `release\Loom\loom-embedded-python-phase22` with `loom.exe`,
      `loom-daemon.exe`, `loom-desktop.exe`, `bin\python-embed\python.exe`,
      and `python\Launcher.py`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-embedded-python-phase22 -Apps Loom`
      passed formal verification with
      `gitHead = 58ad84cf6bf028ad3c963ab898052ce827731633`,
      `gitDirty = false`, and 29 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-embedded-python-phase22 -Apps Loom`
      passed. The smoke summary includes
      `pythonToolExecution.text = "python saw release embedded python"`,
      `pythonToolExecution.pythonExecutable = "...\release\Loom\loom-embedded-python-phase22\bin\python-embed\python.exe"`,
      and `pythonToolExecution.packagedPython = true`.

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- Protocol compatibility names such as `art_loom/*`, `art_hook/*`,
  `art/process`, `shared_memory`, `image_path`, `image_base64`,
  `image_buffer`, and old cloud template names remain intentionally supported.
- Phase 22 restores embedded Python packaging and `.py` script execution
  parity. It does not complete the whole Loom migration. Known later work still
  includes fuller desktop workflow editor/import/interface inference UI parity,
  optional richer Python Art plugin management surfaces, and a final full-source
  audit against old ArtLoom.
