# Loom OCR Result Contract

Loom executes OCR for Hook through `loom.hook.ocr.execute`. The request remains
compatible with `loom.hook.v1` and contains `requestId` plus `imageBase64`. The
successful `data` object contains:

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
- `fullText` is composed from the filtered, displayed block text, so clipboard
  text and block overlays cannot disagree after an empty or invalid block is
  rejected.

## Text correction boundary

Loom preserves recognized punctuation, quotes, whitespace, and general words.
For weak or tiny lines it runs a bounded contrast pass and can compare an
optional PP-OCRv5 recognition model against the PP-OCRv4 primary result. Candidate
selection only accepts stronger semantic evidence or narrowly supported added
symbols. The CTC decoder can restore a repeated punctuation mark when a strong
internal blank valley supports both copies, and can restore a space only when a
larger timestep gap and the model's space probability agree. These evidence
checks recover model omissions without adding arbitrary language-model guesses.

The separate shortcut correction remains bounded by a token boundary and an
actual keyboard shortcut key, such as `A1t+2` to `Alt+2` or `Ctr1+E` to
`Ctrl+E`. Literal text such as `Ctr1被识别成Ctr1` remains unchanged. A character
with no detector, primary/fallback recognition, or CTC probability evidence
cannot be reconstructed safely; Loom reports the best supported result instead
of fabricating text.

## Compatibility

`rawText`, `lineGeometry`, `characterSpans`, and `wordSpans` are additive block
fields. Older Hook builds ignore them and continue using `text` plus `boxPoints`.
New Hook builds bound list sizes, validate source tags, text correspondence,
scores, coordinate order, and line containment before using span extents.
Malformed, corrected/translated, or absent geometry falls back to the existing
axis-aligned renderer.
