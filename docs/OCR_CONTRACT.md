# Loom OCR Contract and Golden Dataset

OCR and QR/barcode recognition are implemented by the optional signed `neuro.official/ocr` Capability Plugin (`apps/ocr-host` and `crates/loom_ocr`). Otter invokes its namespaced commands through `loom.extension.v1` and `loom.capability.runtime.v1`; `loom-daemon` does not expose a hardcoded `loom.hook.ocr.execute` route or statically link ONNX inference into the daemon binary.

Image bytes pass as bounded host-mediated resource references rather than unbounded inline `imageBase64` payloads. While an OCR command is running, the Otter Bridge connection continues serving control frames; closing the connection or stopping the bridge cancels the active invocation and releases staged resources. At most one follow-up command may be deferred per connection.

## 1. Result attachment contract

A successful text-recognition attachment (`neuro.official/ocr.text.v1` / `ocr-text.v1`) contains:

```json
{
  "fullText": "Ctrl+2",
  "textBlocks": [
    {
      "text": "Ctrl+2",
      "boxPoints": [{ "x": 10, "y": 20 }],
      "boxScore": 0.99,
      "textScore": 0.98,
      "colorHex": "#ffffff",
      "bgColorHex": "#101010",
      "lineGeometry": {
        "baseline": [{ "x": 10.0, "y": 42.0 }, { "x": 90.0, "y": 42.0 }],
        "angleDegrees": 0.0,
        "source": "estimatedFromRapidOcrLineQuad"
      },
      "characterSpans": [{
        "text": "C",
        "boxPoints": [
          { "x": 12.0, "y": 20.0 },
          { "x": 23.0, "y": 20.0 },
          { "x": 23.0, "y": 46.0 },
          { "x": 12.0, "y": 46.0 }
        ],
        "score": 0.98,
        "source": "ctcAlignedFromRecognitionTimesteps"
      }],
      "wordSpans": [{
        "text": "Ctrl",
        "boxPoints": [
          { "x": 12.0, "y": 20.0 },
          { "x": 48.0, "y": 20.0 },
          { "x": 48.0, "y": 46.0 },
          { "x": 12.0, "y": 46.0 }
        ],
        "score": 0.97,
        "source": "ctcAlignedFromRecognitionTimesteps"
      }]
    }
  ],
  "scaleFactor": 1.0,
  "width": 320,
  "height": 180
}
```

### Request parameters and immutable packaging

- `input.mode` (optional): `auto` (default bounded rescue), `quick` (skips rescue passes for lower latency), or `highAccuracy` (expands rescue budget and enables the contrast-enhanced PP-OCRv5 fallback model). Unknown modes fail validation.
- `input.region` (optional): `{ left, top, width, height }` in source-image pixels. Empty, overflowing, or out-of-bounds crops are rejected. Detection and line merging run inside the crop, and all output polygons, baselines, character spans, and word spans are translated back into full-image coordinates.
- The OCR host binary, ONNX Runtime shared libraries, RapidOCR/PP-OCR models, and command/result schemas ship together in one signed Capability ZIP verified on install and before every runtime launch.

### Evidence and text-correction semantics

- `boxPoints`, `boxScore`, `text`, and `textScore` come from the RapidOCR line result. `modelTextScoreMean` is an uncalibrated recognizer heuristic, never a calibrated probability.
- `lineGeometry` is derived from the detected line quadrilateral (`source: "estimatedFromRapidOcrLineQuad"`).
- `characterSpans` (max 256/block) and `wordSpans` (max 64/block) project real CTC recognizer timestep intervals into the line quad (`source: "ctcAlignedFromRecognitionTimesteps"`), providing recognition-aligned bounds rather than pixel-level glyph segmentation.
- Conservative post-processing merges evidenced same-row detector fragments left-to-right, orders rows top-to-bottom, and preserves `rawText` whenever `text` / `normalizedText` receives a conservative correction:
  - CTC blank-valley recovery for repeated punctuation and space-probability-backed word separators;
  - high-contrast pixel-stroke recovery of a leading `- ` list dash (omitting CTC spans when a pixel-only prefix is prepended);
  - token-bounded keyboard-shortcut normalization only when followed by `+Key` (for example `A1t+2` -> `Alt+2` or `Ctr1+E` -> `Ctrl+E`), while literal prose tokens such as `Ctr1` without a shortcut key combination remain unchanged.
- `copy-full-text` copies reading-order `fullText`; `copy-layout-text` reconstructs relative indentation, horizontal spacing, and paragraph breaks from cached block geometry without re-running inference.
- Interactive overlays sample a bounded `128x128` region plus outer perimeter to estimate background (`bgColorHex`) and readable foreground (`colorHex`) colors, inherited by `text-translation` through `ocr-text.v1`. Substring text drag-selection, single-click block copy, and `Shift+Click` multi-block selection are preserved.

## 2. QR and barcode child capability

The `neuro.official/ocr` package also decodes QR codes and barcodes into the separate `neuro.official/ocr.codes.v1` attachment:

- `Ctrl+4` runs text OCR and code decoding against the same staged image; `Alt+4` toggles visibility of both layers together.
- Input is capped at 16 MiB and 50 megapixels, emitting at most 32 code results (16 KiB per payload, 16 geometry points, 240 KiB total attachment). Text blocks centered inside decoded code bounds are suppressed so QR modules do not render as noisy text blocks.
- Selecting a code marker opens an anchored Surface panel with **Copy** and—only for validated credential-free `https://` URLs with `hook.external.open` and a direct user click—**Open** (`external.openUrl`).

## 3. Golden dataset and quality metrics

Deterministic and model-backed OCR quality evaluation uses small, human-reviewable golden fixtures under `resources/ocr/fixtures/golden/` (never storing model weights, credentials, or private screenshots):

```json
{
  "fixture": "test_1.png",
  "sourceWidth": 678,
  "sourceHeight": 108,
  "expectedFullText": "first line\nsecond line",
  "expectedBlocks": [
    { "text": "first line" },
    { "text": "second line" }
  ]
}
```

`loom_ocr` exposes `evaluate_quality` and the `measure_quality` / `measure_baseline` examples, reporting:

- `characterErrorRate`: Unicode-scalar Levenshtein distance divided by expected character count (`null` above 4,096 chars or 4,194,304 edit cells to bound memory).
- `punctuationRecall`: multiset recall across ASCII and common punctuation (`null` when the reference has no punctuation).
- `blockBoxIou`: greedy one-to-one average axis-aligned IoU when all expected blocks supply `boxPoints` (`null` otherwise).
- `readingOrderAccuracy`: fraction of adjacent expected blocks appearing in matching order within 25% edit distance.
- `dimensionsMatch`, block counts, and cold/warm inference duration.

Run the OCR unit and golden quality checks from the repository root:

```powershell
cargo test --locked -p loom_ocr
cargo run --locked -p loom_ocr --example measure_quality --quiet
```

To build and verify a standalone signed OCR capability candidate locally:

```powershell
.\scripts\Build-LoomOcrCapabilityPackage.ps1 -OutputRoot <OCR_OUTPUT> -SigningKeyPath <KEY> -SigningPublisherId neuro.official
.\scripts\Build-LoomOcrCapabilityCatalog.ps1 -PackageRoot <OCR_OUTPUT> -BaseUrl https://downloads.example/capabilities/ocr/ -SigningKeyPath <KEY> -SigningPublisherId neuro.official
.\scripts\tests\Test-OcrCapabilityPackageContract.ps1 -ArtifactRoot <OCR_OUTPUT>
```
