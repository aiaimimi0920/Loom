# Phase 20: Real OCR Engine Packaging

## Goal

Attach a real ArtLoom-compatible OCR engine behind the Phase 19 OCR provider
boundary, package the required OCR model/runtime resources, and prove
`art_loom/ocr_image` can return non-fixture OCR output from packaged Loom.

## Tasks

- [x] P20.1 OCR engine packaging audit
  - Acceptance: source-backed spec and implementation plan identify the exact
    Rust dependencies, ONNX/runtime DLL requirements, model files, release
    layout, and fallback behavior.
  - Evidence:
    - `docs/loom/analysis/phase-20-real-ocr-engine-audit.md` records the old
      ArtLoom source evidence, required PaddleOCR/ONNX dependencies, model
      names, `resources/ocr` release layout, provider precedence, and runtime
      DLL decision.
    - Static ORT linking was tested and rejected because the MSVC link step
      failed on unresolved STL helper symbols; Loom keeps ArtLoom's
      `ort/load-dynamic` dependency shape but explicitly binds the packaged
      ONNX Runtime 1.22 DLL through `ort::init_from`.
- [x] P20.2 Real OCR provider runtime
  - Acceptance: targeted tests prove model discovery, image decode, OCR
    invocation, error handling, and fallback behavior behind the existing
    provider boundary.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_ocr --offline -- --nocapture`
      passed with 3 tests.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon ocr_image --offline -- --nocapture`
      passed with 3 daemon OCR tests.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline`
      passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed.
- [x] P20.3 Packaged OCR release smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `art_loom/ocr_image` real OCR output while preserving Loom-only executable
    names.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-real-ocr-phase20 -Force`
      generated `release\Loom\loom-real-ocr-phase20` with `loom.exe`,
      `loom-daemon.exe`, and `loom-desktop.exe`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-real-ocr-phase20 -Apps Loom`
      passed formal verification with `gitDirty=false` and 18 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-real-ocr-phase20 -Apps Loom`
      passed. The smoke summary includes fixture OCR protocol evidence
      `ocrImage.fullText = "release loom ocr"` and real packaged OCR evidence
      `realOcrImage.fullTextLength = 63`, `realOcrImage.width = 678`,
      `realOcrImage.height = 108`, and `realOcrImage.blockCount = 2`.

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Old ArtLoom packaged OCR models were found under:
  `Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom\resources\ocr`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- Do not reintroduce user-visible NeuroLoom/Neuro prefixes.
- Phase 20 restores real OCR through a new `loom_ocr` crate, packaged OCR model
  files, packaged `onnxruntime.dll`, and an isolated release smoke that starts a
  non-fixture daemon from the release directory.
- Full Loom migration is not complete yet. Known later work still includes
  cloud multipart/template parity, fuller desktop workflow editor/import and
  interface-inference UI parity, embedded Python packaging, and a final
  full-source audit against old ArtLoom.
