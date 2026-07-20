# Phase 20 Real OCR Engine Packaging Audit

## Scope

Phase 20 attaches a real OCR engine behind the Phase 19 daemon OCR provider
boundary. It must keep the existing Hook Bridge protocol and visible Loom
artifact names unchanged:

- `art_loom/ocr_image`
- `art_loom/get_capabilities`
- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom repository remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old ArtLoom source evidence

Old runtime source:

- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ocr_service.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ipc_service.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\lib.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\Cargo.toml`

Old dependencies:

```toml
image = { version = "0.25", features = ["default", "jpeg", "png", "webp", "bmp"] }
paddle-ocr-rs = "=0.6.0"
num_cpus = "1.17.0"
rayon = "1.10.0"
ort = { version = "=2.0.0-rc.10", default-features = false, features = ["load-dynamic", "download-binaries", "copy-dylibs"] }
```

Old OCR request handling:

- `art_loom/ocr_image` accepts `image_base64`.
- If OCR state is unavailable, it returns:
  `{"type":"error","data":{"message":"OCR enhancement unavailable"}}`.
- It base64-decodes the image, runs `OcrService::run_ocr(image_bytes, false)`,
  and returns `{"type":"success","data": OcrDetectResult}`.

Old OCR model discovery:

1. Tauri `resource_dir()/resources/ocr`
2. Tauri `resource_dir()/ocr`
3. `current_dir()/resources/ocr`
4. `current_dir()/src-tauri/resources/ocr`
5. `current_dir()/../ArtHook/src-tauri/resources/ocr`

Old runtime sets OCR available when a candidate directory exists and contains
`ch_PP-OCRv4_det_infer.onnx`.

Old packaged model files:

```text
Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom\resources\ocr\ch_PP-OCRv4_det_infer.onnx
Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom\resources\ocr\ch_ppocr_mobile_v2.0_cls_infer.onnx
Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom\resources\ocr\ch_PP-OCRv4_rec_infer.onnx
Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom\resources\ocr\ch_PP-OCRv5_rec_mobile_infer.onnx
```

The old package also contains `DirectML.dll` at its package root. Local
`ort-sys` source shows `copy-dylibs` copies ONNX Runtime dynamic provider DLLs
from `%LOCALAPPDATA%\ort.pyke.io\dfbin\...` to the Cargo target profile
directory.

Phase 20 keeps ArtLoom's `ort/load-dynamic` dependency shape, but Loom must not
allow Windows DLL search order to pick the host-level
`C:\Windows\System32\onnxruntime.dll`. This host has ONNX Runtime `1.17.1`,
while `ort 2.0.0-rc.10` requires `1.22.x`; an unqualified dynamic load panics
before OCR can run. Loom therefore packages the official Windows x64 ONNX
Runtime `1.22.0` DLLs under `resources/ocr` and calls
`ort::init_from(<model_root>\onnxruntime.dll)` before creating OCR sessions.

A static-link experiment was rejected. With `ort/load-dynamic` disabled, the
MSVC link step failed on unresolved STL vectorized helper symbols such as
`__std_find_last_of_trivial_pos_1`, `__std_search_1`, and
`__std_remove_1`. The failure indicates the prebuilt ORT static library expects
a newer MSVC STL ABI than this Windows host provides. Explicitly bound dynamic
loading is the compatible path and matches the old ArtLoom dependency feature
set.

## Loom current state before Phase 20

Phase 19 added:

- `loom_image_io` for deterministic image decode/encode helpers.
- `art_loom/ocr_image` parsing and legacy success/error response helpers in
  `loom_hook_bridge`.
- `loom-daemon` provider boundary:
  - `OcrProvider::Unavailable`
  - `OcrProvider::Fixture { text }`
  - `LOOM_OCR_FIXTURE_TEXT` fixture override.
- Default unavailable behavior remains honest.

Release build behavior before this phase:

- `scripts/build-release-exes.ps1` copies only the three Loom exes.
- Loom `supportFiles` is empty.
- `scripts/verify-release.ps1` already verifies `supportFiles`,
  `checksums.sha256`, manifest records, and zip sidecars if support files are
  present.

Phase 20 adds release support files for OCR models, the fixture image used by
the real release smoke, and the bound ONNX Runtime DLLs under
`resources/ocr/*`.

## Phase 20 implementation design

### Crate boundary

Create a focused crate:

```text
Loom/crates/loom_ocr
```

Responsibilities:

- Resolve OCR model directories.
- Validate required model files.
- Decode image bytes into RGB.
- Lazily initialize `paddle_ocr_rs::ocr_lite::OcrLite`.
- Map native OCR output into ArtLoom-compatible JSON-ready structs.

It should not depend on daemon HTTP/WebSocket internals.

### Model layout

Track migrated model resources under:

```text
Loom/resources/ocr/
```

Package them to:

```text
release/Loom/<version>/resources/ocr/
```

The daemon should discover models in this order:

1. `LOOM_OCR_MODEL_DIR`, when set.
2. `current_exe()/../resources/ocr`, matching local release execution.
3. `current_dir()/resources/ocr`.
4. `current_dir()/Loom/resources/ocr`, matching repo-root daemon execution.
5. `Loom/resources/ocr` relative to `CARGO_MANIFEST_DIR` ancestors when running
   tests.

Availability must require all three RapidOCR v4 files:

- `ch_PP-OCRv4_det_infer.onnx`
- `ch_ppocr_mobile_v2.0_cls_infer.onnx`
- `ch_PP-OCRv4_rec_infer.onnx`

The v5 recognition file may be packaged for parity but is not required by the
Phase 20 default provider.

The release package also includes:

- `onnxruntime.dll`
- `onnxruntime_providers_shared.dll`
- `fixtures/test_1.png`

### Provider precedence

Keep deterministic fixture support for tests:

1. If `LOOM_OCR_FIXTURE_TEXT` is set, use the fixture provider.
2. Else if a valid model directory is found, use the real provider.
3. Else use `Unavailable`.

This keeps existing smoke tests deterministic unless they explicitly opt into
real OCR by clearing the fixture and selecting/using packaged models.

### Runtime behavior

Real provider behavior:

- `art_loom/get_capabilities` reports `ocr=true` only when fixture or real
  provider is available.
- `art_loom/ocr_image` uses real OCR when no fixture is set and models are
  available.
- Invalid image/base64 errors continue returning legacy error shape.
- Missing models keep returning `OCR enhancement unavailable`.
- The OCR session is lazy, matching old ArtLoom's `hot_start=false`.

### Release packaging

Update `scripts/build-release-exes.ps1` so Loom `supportFiles` includes:

- `Loom/resources/ocr/README.txt`
- `Loom/resources/ocr/ch_PP-OCRv4_det_infer.onnx`
- `Loom/resources/ocr/ch_ppocr_mobile_v2.0_cls_infer.onnx`
- `Loom/resources/ocr/ch_PP-OCRv4_rec_infer.onnx`
- `Loom/resources/ocr/ch_PP-OCRv5_rec_mobile_infer.onnx`
- `Loom/resources/ocr/fixtures/test_1.png`
- `Loom/resources/ocr/onnxruntime.dll`
- `Loom/resources/ocr/onnxruntime_providers_shared.dll`

Support files are already represented in manifest/checksum/zip verification.

### Release smoke

Add a real OCR smoke path that:

1. Starts packaged `loom-daemon.exe` without `LOOM_OCR_FIXTURE_TEXT`.
2. Uses the packaged `resources/ocr` directory.
3. Sends `art_loom/get_capabilities` and expects `ocr=true`.
4. Sends `art_loom/ocr_image` with a text image fixture.
5. Expects success, non-empty `fullText`, image dimensions, and at least one text
   block.

Keep the existing fixture OCR smoke as a deterministic protocol smoke if needed,
but Phase 20 release evidence must include separate `realOcrImage` output.

## Risks

- The real OCR test may be slower than normal unit tests because it loads ONNX
  models.
- Generated text recognition can vary by font/rasterization. Assertions should
  require non-empty text and block count, not exact text, unless the fixture is
  proven stable locally.
- Build time and release size increase substantially because model files and
  ONNX Runtime support files are packaged.
- The old ArtLoom `tauri.conf.json` does not list resources explicitly; its
  custom local package layout had `resources/ocr` beside the executable. Loom's
  release script must make this explicit.

## Acceptance checks

- `cargo test --manifest-path Loom/Cargo.toml -p loom_ocr -- --nocapture`
- `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon ocr_image -- --nocapture`
- `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon`
- `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId <phase20-id> -Force`
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId <phase20-id> -Apps Loom`
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId <phase20-id> -Apps Loom`
