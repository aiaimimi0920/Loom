# Phase 21 Cloud Multipart Template Parity Audit

## Scope

Phase 21 restores the cloud API layer that old ArtLoom used for hosted image
tools beyond the Phase 16 JSON-only cloud runtime:

- old `url` field compatibility
- `contentType`, `headers`, and `body` execution config
- `{{inputs.x.value}}`, `{{inputs.x.path}}`, `{{inputs.x}}`, and `{{x}}`
  template substitution
- `multipart/form-data` file uploads from Hook Bridge image input
- packaged release smoke coverage for the restored contract

The visible Loom product names remain unchanged:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old ArtLoom source evidence

Reviewed old runtime and UI source:

- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\cloud_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\converters.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src\components\AddArtModal.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\utils\cliTemplateParser.ts`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\Cargo.toml`

Old dependency shape:

```toml
reqwest = { version = "0.11", features = ["json", "multipart", "blocking"] }
```

Old cloud execution config reads:

- `execution["url"]`
- `execution["method"]`
- `execution["contentType"]`
- `execution["headers"]`
- `execution["body"]`

Old request templating supports:

- `{{inputs.x.value}}`
- `{{inputs.x.path}}`
- `{{inputs.x}}`
- `{{x}}`

The old engine injects a temp image file path under the input key when it runs
image tools. Multipart bodies are JSON key/value maps. Empty values,
`"__DISABLED__"`, and still-unresolved templates are skipped. File fields are
recognized when the template refers to `.path}}`, when the key is `file`,
`image`, or `image_file`, or when the key ends with `_file`.

The old UI exposes this contract through the cloud API editor:

- Smart Import for cURL and response samples.
- Content-Type selector with `application/json`, `multipart/form-data`, and
  `application/x-www-form-urlencoded`.
- Headers JSON textarea.
- Body textarea.
- Multipart tooltip instructing users to write JSON key/value pairs and use
  `{{inputs.x.path}}` for file paths.

## Loom state before Phase 21

Phase 16 restored only safe JSON cloud execution:

```rust
CloudApi { endpoint: String, method: String }
```

The runtime substituted only the endpoint and sent JSON arguments for methods
with request bodies. It did not accept the old `url` alias, did not persist
`contentType`, `headers`, or `body`, did not build multipart requests, and did
not provide Hook Bridge cloud tools with a real file path for
`{{inputs.input.path}}`.

The packaged release smoke covered:

- direct JSON cloud tool execution
- cloud-backed `art_loom/execute_art_node`
- cloud-backed AHRP `art/process`

It did not cover old ArtLoom multipart/template config.

## Phase 21 implementation design

### Tool registry execution config

`ToolExecution::CloudApi` keeps the current `endpoint` field and accepts the old
`url` field as a deserialization alias:

```rust
#[serde(alias = "url")]
endpoint: String
```

It adds optional fields that serialize in the same camelCase shape the old UI
used:

- `contentType`
- `headers`
- `body`

### Template rendering

Cloud templates are rendered from the execution arguments using the old ArtLoom
forms:

- `{{key}}`
- `{{inputs.key}}`
- `{{inputs.key.value}}`
- `{{inputs.key.path}}`

Template rendering is applied to:

- endpoint URL
- headers JSON
- explicit body
- multipart field values

### Multipart execution

When `contentType` is `multipart/form-data`, Loom parses `body` as a JSON map
of field names to template strings and builds a `reqwest::blocking::multipart`
form.

Fields are skipped when the rendered value is empty, equals `__DISABLED__`, or
still contains `{{`. File parts use `Form::file` when the field is recognized as
a file field and the rendered path exists. Multipart requests deliberately do
not set a manual `Content-Type` header because reqwest must add the boundary.

### Hook Bridge cloud image input

For cloud API Art node execution only, daemon converts non-empty
`input_base64` into a per-call temp file named:

```text
loom-cloud-input-<pid>-<timestamp>.png
```

It keeps `input_base64` available for existing JSON cloud tools, and injects the
temp file path under `input` and `image` for old templates such as:

```json
{"file":"{{inputs.input.path}}"}
{"file":"{{inputs.image.path}}"}
```

Only temp files created by the current call are removed after execution.

### Release smoke

The release smoke registers an old-style tool using `url`, `contentType`,
`headers`, and `body`, then calls it through `art_loom/execute_art_node`. The
fixture writes multipart evidence to `multipart-request.json` and the smoke
asserts:

- `/multipart/image` templated route
- multipart content type with boundary
- `file` part
- `loom-cloud-input-*` temp filename
- prompt field template substitution
- `X-Trace` header template substitution
- no unresolved `{{...}}` templates

## Non-goals

Phase 21 does not complete the whole Loom migration. Remaining layered gaps
still include embedded Python packaging parity, fuller desktop workflow
editor/import/interface inference UI parity, and a final full-source audit
against old ArtLoom.
