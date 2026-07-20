# Phase 19: OCR and Image Helper Parity

## Goal

Restore the next bounded ArtLoom compatibility layer in Loom: the
`art_loom/ocr_image` protocol surface, dynamic OCR capability reporting, and
first-class `image_path` / `image_base64` / `image_buffer` helper conversion.

## Tasks

- [x] P19.1 Audit old ArtLoom OCR/image helper packs
  - Acceptance: source-backed design and implementation plan identify the old
    contracts and split real PaddleOCR packaging into a later bounded layer.
  - Evidence:
    - Old `art_loom/ocr_image` request and response path sampled from
      `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ipc_service.rs`.
    - Old OCR runtime sampled from
      `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ocr_service.rs`.
    - Old image helper converters sampled from
      `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\converters.rs`.
    - Old packaged OCR models found under
      `Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom\resources\ocr`.
- [x] P19.2 Image helper converter runtime
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_image_io -- --nocapture`
    proves base64/path/buffer image helper conversion.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_image_io -- --nocapture`
      passed with 4 unit tests.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon image_helper -- --nocapture`
      passed with 2 daemon helper-route tests:
      `daemon_image_helper_converts_base64_to_rgba_buffer` and
      `daemon_image_helper_converts_path_to_base64`.
- [x] P19.3 OCR Hook Bridge protocol and release smoke
  - Acceptance: daemon tests and packaged release smoke prove
    `art_loom/ocr_image` request handling, OCR capability reporting, and image
    helper conversion evidence while keeping Loom-only executable names.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge -- --nocapture`
      passed with 19 unit tests.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon ocr_image -- --nocapture`
      passed with 2 daemon OCR tests:
      `daemon_hook_bridge_ocr_image_fixture_provider_returns_success` and
      `daemon_hook_bridge_ocr_image_unavailable_by_default`.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
      passed with 52 library tests and 2 CLI integration tests.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - Release id: `loom-ocr-image-helper-8ad62b76`.
    - Release directory:
      `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\loom-ocr-image-helper-8ad62b76`.
    - Executables: `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-ocr-image-helper-8ad62b76 -Apps Loom`
      passed with `gitDirty=false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-ocr-image-helper-8ad62b76 -Apps Loom`
      passed.
    - Smoke summary:
      `C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\runs\20260612-153539-Loom-47684-a1eab73ca7864b54bcdab04f622f914a\release-local-apps-loom-ocr-image-helper-8ad62b76-Loom-summary.json`.

## Evidence

Phase 19 restored the first OCR/image helper compatibility layer:

- `loom_image_io` converts `image_base64`, `image_path`, and `image_buffer`
  data through deterministic PNG/RGBA8 helpers.
- `loom-daemon` exposes `POST /v1/image-helpers/convert`.
- Hook Bridge accepts legacy `art_loom/ocr_image`.
- Hook Bridge reports dynamic OCR capability through
  `art_loom/get_capabilities`.
- Default OCR behavior remains honest when no engine is configured:
  `OCR enhancement unavailable`.
- A provider boundary allows a later phase to attach the real PaddleOCR/ONNX
  runtime without changing Hook Bridge or daemon contracts.
- Packaged release smoke proved:
  - `imageHelperConvert.outputRgba = "10,20,30,255"`;
  - `ocrImage.method = "art_loom/ocr_image"`;
  - `ocrImage.ocrAvailable = true`;
  - `ocrImage.fullText = "release loom ocr"`.

## Notes

- This phase follows the approved "scheme B" layered restoration path after
  shared image I/O.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- Real PaddleOCR model packaging remains a later bounded layer.
