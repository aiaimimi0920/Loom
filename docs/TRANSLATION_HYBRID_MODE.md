# Translation hybrid mode

The official `text-translation` capability keeps one OCR input/output contract
and selects its model backend inside Loom's host-side model broker. The plugin
process never receives a provider URL or credential.

## Modes

Set `LOOM_TRANSLATION_MODE` in the Loom daemon environment to one of:

- `gateway`: use the configured Loom Gateway only.
- `local`: use the loopback local translation service only. A missing or failed
  local service is a hard failure and never sends OCR text to Gateway.
- `auto`: for each OCR paragraph, try the local service once, then use Gateway once
  if local inference fails. Fallback is bounded to that broker request.

When the mode is unset, Loom preserves the existing Gateway behavior unless
`LOOM_LOCAL_TRANSLATION_BASE_URL` is configured. With that URL present, the
implicit mode is `auto`.

Hook's default `auto` selection follows this daemon configuration. An explicit
`local` or `gateway` selection in Hook overrides the daemon default. In particular,
an operator's `LOOM_TRANSLATION_MODE=local` remains local-only when Hook uses its
default selection.

## Local service contract

The local service must expose an OpenAI-compatible non-streaming endpoint at
`/v1/chat/completions`. The host sends the same bounded system and user messages
used by the Gateway path and reads the first assistant message. Configure it
with:

```powershell
$env:LOOM_TRANSLATION_MODE = "auto"
$env:LOOM_LOCAL_TRANSLATION_BASE_URL = "http://127.0.0.1:11434"
$env:LOOM_LOCAL_TRANSLATION_MODEL = "translation-local"
```

`LOOM_LOCAL_TRANSLATION_TOKEN` is optional for a local service that expects a
Bearer token. The token remains in the daemon process and is never injected
into the capability child process.

Only an `http` or `https` origin whose host is an IP loopback address is
accepted. Paths, query strings, fragments, URL credentials, non-loopback hosts,
and system proxies are rejected for the local path. Local-only mode allows up to
60 seconds per model request. Automatic mode reserves 35 seconds for the local
service and up to 25 seconds for Gateway fallback per request. The capability
command has a shared 65-second deadline across all paragraph requests.

The local endpoint must support OpenAI-compatible `response_format.json_schema`.
The capability joins compatible OCR fragments into visual lines and paragraphs
before making one bounded model-broker request per paragraph. This preserves
sentence context across wrapped lines. Grouping checks font scale, sampled text
and background colors, alignment, line gaps, list boundaries, and intervening
OCR regions. It keeps columns, different-sized headings, timestamps, and short
stacked controls separate. Ambiguous geometry remains separate; grouping does
not repair OCR spelling or infer missing text.

The translation prompt preserves claims, negation, conditions, certainty, and
the speaker's stance. Idioms are translated in context without summarizing or
softening the source. Model quality still needs semantic checks against the
original text; successful JSON validation alone does not establish fidelity.

Each request uses the paragraph's first source block ID and an exact one-item
schema; missing, duplicate, unknown, or empty results fail without replacing
OCR. Inputs without geometry retain one whole-text request. The broker rejects
external references, regexes, combinators, and oversized schemas. Local and
Gateway requests use the same paragraph contract.

From capability version `0.2.34`, output `textBlocks` are translated paragraphs.
Their `text` joins the source fragments in reading order, their geometry is the
union of those fragments, and `sourceBlockIndices` maps back to the unchanged
OCR input. That additive field is optional in the v1 schema so cached older
results remain readable. `originalText`, source dimensions, source attachment
identity/digest/revision, and source revision remain unchanged.

The scene wraps each translation inside its paragraph rectangle. Font size is
capped by the original line typography, then reduced when the translated text
needs more room; paragraph height is never used as a single-line font size.
Container-relative width/height bounds preserve the fit when resized. The
scene uses the renderer-supported `1.2em` line height. These geometric
heuristics and model output still require real-rendering and linguistic
verification for representative documents.

## Failure and privacy semantics

`local` mode is the strict offline choice. `auto` is a convenience mode, not a
privacy guarantee: it may send an OCR paragraph to the configured Gateway after a
local failure. The UI or operator configuration must make that distinction
visible before enabling `auto` for sensitive OCR content.

The existing translation host still validates the exact number of translations,
source revision, block geometry, response size, and final attachment size. A
provider error therefore leaves the original OCR attachment intact. The Hook
command and renderer contracts do not change.

The repository does not bundle a translation model. An operator must install a
local OpenAI-compatible translation runtime and language model separately. The
runtime must be treated as a signed, versioned local dependency before it is
made part of a release package.
