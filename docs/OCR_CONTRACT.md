# Loom OCR Result Contract

OCR is implemented by the separately installed `neuro.official/ocr` Capability
Plugin. Hook invokes its namespaced command through `loom.extension.v1` and
`loom.capability.runtime.v1`; `loom-daemon` does not expose a fixed
`loom.hook.ocr.execute` branch or link the OCR inference crate. Image bytes are
passed as a bounded host-mediated resource reference rather than an unbounded
core `imageBase64` request. The successful result attachment contains:

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

Recognition requests may include an optional `input.mode` value. `auto` is the
backward-compatible default and runs bounded local rescue; `quick` skips rescue
passes for lower latency; `highAccuracy` expands the rescue budget and may run
the contrast-enhanced fallback model. Unknown values are rejected as invalid
input. The mode changes inference effort only; attachment schemas, reading
order, clipboard behavior, and QR/code suppression remain unchanged.

Requests may also include an optional `input.region` object using source-image
pixel coordinates: `left`, `top`, `width`, and `height`. Empty, overflowing, or
out-of-bounds regions are rejected. Detection and line merging happen inside
the crop, then line boxes, baselines, character spans, and word spans are all
translated back into the full-image coordinate space before rendering or QR
suppression.

## Evidence semantics

- `boxPoints`, `boxScore`, `text`, and `textScore` originate from the current
  `paddle-ocr-rs`/RapidOCR line result.
- `lineGeometry` is optional. RapidOCR does not expose a typographic baseline,
  so Loom derives it from the detected line quadrilateral and identifies that
  fact in `source`.
- `characterSpans` and `wordSpans` are optional, bounded recognition geometry.
  Loom keeps the recognizer's real CTC timestep intervals, then projects those
  intervals into the detected quadrilateral. These fields are materially more
  accurate than equal-width character guesses, but they are recognition-aligned
  boxes rather than independent pixel-level glyph segmentation. The explicit
  `ctcAlignedFromRecognitionTimesteps` source prevents that distinction from
  being lost.
- Character spans retain each decoded token, including conservatively recovered
  punctuation or spaces. Word spans group ASCII word runs and retain non-ASCII
  symbols as independently addressable spans. A block emits at most 256
  character spans and 64 word spans.
- `rawText` is optional and appears only when Loom applies a conservative text
  correction. It preserves the recognizer output for diagnostics and future
  correction review.
- `text` remains the display/clipboard text for backward compatibility. When a
  correction was applied, `normalizedText` repeats that corrected value while
  `rawText` preserves the model output. `confidenceSource` identifies the
  heuristic behind the existing `textScore`; `modelTextScoreMean` is not a
  calibrated probability and must not be presented as one.
- Optional `confidence` diagnostics expose the mean and minimum decoded-symbol
  scores plus total and recovered symbol counts. Its explicit
  `ctcDecodedSymbolScores` source means consumers can identify a weak local
  symbol without treating the values as language confidence. Merged detector
  fragments use a symbol-count-weighted mean and retain the weakest score; if a
  fragment lacks complete evidence, the merged confidence summary is omitted.
- `fullText` is composed from the filtered, displayed block text, so clipboard
  text and block overlays cannot disagree after an empty or invalid block is
  rejected.
- After fragment merging, Loom applies a stable visual order: blocks sharing a
  line are sorted left-to-right, and separate rows top-to-bottom. Exact ties
  retain detector order, so punctuation and same-pixel fragments remain stable.

## Text correction boundary

Loom preserves recognized punctuation, quotes, whitespace, and general words.
For weak or tiny lines it runs a bounded contrast pass and can compare an
optional PP-OCRv5 recognition model against the PP-OCRv4 primary result. Candidate
selection only accepts stronger semantic evidence or narrowly supported added
symbols. The CTC decoder can restore a repeated punctuation mark when a strong
internal blank valley supports both copies, and can restore a space only when a
larger timestep gap and the model's space probability agree. These evidence
checks recover model omissions without adding arbitrary language-model guesses.
Detector fragments that share one evidenced visual row are first merged in
left-to-right order, preventing their opaque overlay rectangles from covering
one another while keeping unrelated rows independent.
When the detector excludes a short list dash from an otherwise horizontal line,
Loom may restore `- ` only from a bounded, high-contrast horizontal pixel stroke
immediately to the left of that line. The original recognized text is retained
in `rawText`, and CTC spans are omitted because they do not cover the pixel-only
prefix.

The interactive overlay uses one opaque fill selected against all recognized
foreground and source-background colors in the owning image. Red and red-adjacent
hues are excluded: they are reserved for error/destructive semantics rather than
ordinary OCR text. Typography is bounded by both detected line height and available
line width, so long results shrink instead of being clipped by their detector box.
Recognized text nodes opt into native WebView text selection. Dragging across a
substring keeps that selection instead of firing the block's whole-text copy action;
a selection-free click continues to copy the complete block.

The separate shortcut correction remains bounded by a token boundary and an
actual keyboard shortcut key, such as `A1t+2` to `Alt+2` or `Ctr1+E` to
`Ctrl+E`. Literal text such as `Ctr1被识别成Ctr1` remains unchanged. A character
with no detector, primary/fallback recognition, or CTC probability evidence
cannot be reconstructed safely; Loom reports the best supported result instead
of fabricating text.

## Compatibility

`rawText`, `lineGeometry`, `characterSpans`, and `wordSpans` are additive block
fields in the versioned OCR attachment schema. Generic Hook hosts keep unknown
attachments opaque; compatible OCR renderers bound list sizes and validate source
tags, text correspondence, scores, coordinate order, and line containment before
using span extents. Malformed, corrected/translated, or absent geometry falls
back inside the plugin renderer rather than a core OCR branch.

Legacy `UnitData.ocrResult` values are accepted only by Hook's bounded one-time
session migration. Successful migration writes the versioned generic attachment
and clears the legacy field; invalid, conflicting, or oversized values are
preserved without executing or rendering them so user data is not silently lost.

## QR and barcode child capability

The same `neuro.official/ocr` package also owns local QR/barcode decoding.
`Ctrl+2` performs text OCR and code decoding against the same bounded staged
image. The unit toolbar intentionally exposes only cached-result actions and
does not duplicate recognition with a code-only refresh item. The retained
`neuro.official/ocr.scan-codes` command remains protocol-compatible for existing
callers but is not contributed to `hook.unit.toolbar`. Code results use the separate
`neuro.official/ocr.codes.v1` attachment. Text and code results therefore have
independent schemas and renderers while sharing one install, signature, runtime
process, permission review, and lifecycle. `Alt+2` changes both attachments to
one shared visible/hidden state.

```json
{
  "schemaVersion": "1",
  "visible": true,
  "sourceWidth": 640,
  "sourceHeight": 480,
  "selectedId": "code-1",
  "results": [{
    "id": "code-1",
    "format": "QR_CODE",
    "text": "https://example.com/hook",
    "url": "https://example.com/hook",
    "points": [{ "x": 10.0, "y": 20.0 }],
    "bounds": { "left": 10.0, "top": 20.0, "right": 110.0, "bottom": 120.0 }
  }],
  "surfaceScene": { "type": "stack", "children": [] }
}
```

The decoder limits input to 16 MiB and 50 million pixels, results to 32, each
payload to 16 KiB, geometry to 16 points, and the complete attachment to 240
KiB. Text blocks centered inside decoded code bounds are suppressed so QR pixels
cannot become opaque OCR controls. Each code is represented by one high-contrast
information-blue center action circle; the active circle uses signal yellow.
Selecting it opens a bounded nearby Surface panel with a three-line editable
value and icon-only **Copy** and **Open** actions stacked vertically on its right;
non-URL results omit **Open**. The editor preserves native substring selection.
Its preferred top-left anchor is the selected code circle's bottom-right corner.
Collision handling uses the rendered panel size and shifts only an overflowing
axis enough to remain inside the owning sticker, so scaling and edge-adjacent
codes cannot push actions outside the clipped viewport.
Wheel input over the editor remains local to its native scroll area rather than
reaching sticker or canvas gestures.
Clicking the active circle again or clicking the sticker outside the panel closes
it. The dismiss backdrop relinquishes pointer-down ownership so sticker drag and
host-level interaction still work while the panel is open. Copy remains
clipboard-brokered. Open revalidates the edited value and emits the
generic `external.openUrl` effect and requires the package and command to declare
`hook.external.open`, a consumed user gesture, a direct Surface click, and a
credential-free HTTPS URL. Hook validates the URL again before its Windows
native broker asks the system browser to open it. Old `UnitData.barcodeResult`
uses the same lossless migration rule as legacy OCR data.

## Manual validation

1. Start the paired Hook candidate and a Loom candidate configured with a signed
   capability catalog.
2. Open **Settings > Capability Extensions** and select **Download and install
   OCR**. Review the immutable digest and seven requested permissions, including
   external HTTPS opening, then enable.
3. Capture a clear text image containing a QR code. Confirm `Ctrl+2` writes both
   attachments and `Alt+2` hides and restores both layers together.
4. Click each center circle. Confirm the nearby action panel, per-result copy,
   HTTPS browser opening, close behavior, block copy, overlay layout, and the
   unit-scoped notices. A non-HTTPS payload must not expose **Open**.
5. Disable and re-enable the package. Hook must refresh contributions without a
   restart; no OCR shortcut or decoder remains active while disabled.

`Build-LoomOcrCapabilityPackage.ps1`,
`Build-LoomOcrCapabilityCatalog.ps1`, and
`Start-LoomWithLocalOcrCatalog.ps1` create an isolated loopback candidate for
this flow without adding model files to the default Loom release.
