# OCR Golden Dataset And Metrics

The OCR quality gate starts with small, checked-in golden fixtures. A fixture
contains the source image and a JSON expectation under
`resources/ocr/fixtures/golden/`. The expectation is deliberately model
agnostic: it records the text and block order that the installed capability is
expected to expose, not an internal RapidOCR tensor or a renderer snapshot.

## Format

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

`fixture`, source dimensions, and `expectedFullText` are required. Blocks are
listed in the reading order used by `OcrDetectResult.full_text`. A block may
  also provide `boxPoints` when the detector geometry is stable enough to become
  an explicit layout assertion. Omitting boxes makes the geometry metric
  `null`, rather than pretending that an unmeasured box is correct. The checked-
  in fixture includes detector boxes because its bundled model and image are
  stable; new fixtures should omit them until that stability is measured.

Golden files must remain small and human-reviewable. They must not contain model
weights, access tokens, screenshots of private user data, or renderer-specific
coordinates that are not part of the OCR contract.

## Metrics

`loom_ocr` exposes `evaluate_quality` and the `measure_quality` example. The
report contains:

- `characterErrorRate`: Unicode-scalar Levenshtein distance divided by the
  expected character count. An empty/empty pair is `0`; an empty reference with
  non-empty output is `1`. Inputs above 4,096 characters or comparisons above
  4,194,304 edit cells return `null` so a malformed fixture cannot create an
  unbounded quadratic allocation.
- `punctuationRecall`: multiset recall for ASCII and common CJK punctuation,
  including `#`, `-`, quotes, and `。`. It is `null` when the expectation has no
  punctuation.
- `blockBoxIou`: one-to-one greedy average IoU for expected and actual axis-
  aligned block bounds. It is `null` until every expected block has an explicit
  `boxPoints` value and all actual blocks have valid bounds.
- `readingOrderAccuracy`: the fraction of adjacent expected blocks that appear
  in the same order after matching unique, near-identical block text. A match
  must be within 25% Unicode edit distance (with a one-character minimum), so
  unrelated blocks cannot inflate the order score. It is `null` for an empty
  expectation; a reversed two-line result scores `0`.
- `dimensionsMatch`, block counts, and OCR duration provide structural and
  performance context without hiding recognition failures.

These metrics are complementary. CER detects missing or substituted content,
punctuation recall keeps symbols visible, reading order detects row reordering,
and IoU can be enabled when a fixture has reliable detector geometry. None of
the metrics authorizes synthetic text or guessed glyph boxes in production.

## Commands

From `Loom/`:

```powershell
cargo test --locked -p loom_ocr
cargo run --locked -p loom_ocr --example measure_quality --quiet
```

The current `test_1.png` baseline is two blocks with dimensions `678x108`.
Measured cold OCR includes lazy ONNX session initialization; use
`measure_baseline` for cold/warm timing and `measure_quality` for the semantic
report. CI may run the example only on workers that have the bundled Windows
runtime and models; deterministic metric unit tests remain platform-neutral.
