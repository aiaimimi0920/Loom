# Browser document provider commands (candidate)

Status: a locally signed development runtime is installed for the explicit Web
Live entry. See Hook `docs/LIVE_WEB_PINNED_ACCEPTANCE_20260908.md` for the actual
acceptance boundary. The newer Ctrl+2 relay candidate is documented in Hook
`docs/LIVE_CTRL2_BROWSER_CAPTURE_20260908.md`; full native acceptance is still a gate.
This is not a formal release.

## Transport and authority

Use bounded `loom.extension.command.invoke` through the existing authenticated
Hook capability session. This is command polling, not a modification of the
runtime framing or an unbounded response stream. Command IDs are contributed by
the installed package, not hard-coded browser window IDs.

Payload protocol: `hook.browser-document.commands.v1`.

The Hook consumer requires a trusted package and pins its package digest,
permission-grant digest, version and scope for the source lifetime. It validates
command ownership and requires `requiresUserGesture: true` on open and false on
poll/close. Among frame-lifecycle calls, only open includes the originating
gesture token; the separate preparation command also requires a direct gesture.
Loom remains authoritative for signature, installation, enablement, permissions,
snapshot generation, command ownership, and runtime isolation.

The invocation target is the originating Unit ID/revision. The consumer does not
follow subsequent selection changes. Source session/document tokens are opaque,
bounded to 256 ASCII letters, digits, underscore, dot, colon or hyphen. They are
transient state, never persisted in Unit image data.

### New-Unit Ctrl+2 capture

Hook may reserve a fresh `browser-<UUID>` Unit ID at revision zero for a capture
creation intent. It rejects graph/session collisions and creates the resulting
Live Unit with exactly that identity. No placeholder sticker or unrelated
selected Unit is required. `createUnit` is Hook-local lifecycle metadata, not a
provider command input or a browser permission.

On supported foreground Edge windows, Hook prepares the existing provider and
records its selection cursor before forwarding the original keyboard gesture to
the browser's `_execute_action` shortcut (`Ctrl+Shift+2`). A one-shot native
ticket pins HWND/PID, expires after 30 seconds, rejects foreground changes, and
waits for held shortcut keys to be released. Cancellation clears unconsumed
tickets. Hook never acquires a tab ID or constructs a browser grant.

The browser action still supplies `activeTab` and the actual tab. Its trusted
region picker produces the opaque grant through the existing selection path.
There is no second palette import, second region selection, native capture
fallback, additional host permission, or production remote-debugging port.
The existing palette entry remains available for deliberate browser selection.

## Command payloads

### Selection handoff before open

`prepare` accepts empty input and returns `{ protocol, ready: true }`. It requires
a direct user gesture and starts no capture. `selection-ready` requires no
gesture and returns `{ protocol, ready, cursor, browserGrantId? }`. With empty
input it returns only the current cursor and `ready: false`. Later requests send
`{ after: cursor }`; only a newer unexpired browser offer can be returned. The
query does not consume a grant. It exposes no URL, title or document pixels.

Hook freezes its target and separate prepare/open gesture tokens at the initial
command, waits within a bounded deadline, and opens the exact returned opaque
grant once. The native broker removes that offer by ID, not FIFO. Old offers
cannot satisfy a new wait. Poll/close remain bound to the resulting document
session and originating Unit revision. Browser authorization still comes from
the explicit browser action, never the readiness query.

Open/poll/close input and output objects include the exact payload `protocol`
string above. Preparation/readiness use the setup input shapes just described.
Only a succeeded extension result with no effects is consumable. Accepted,
progress, cancelled, failed and effect-bearing results are not frames.

### Open

Input contains `selection`, using Hook's existing capture rectangle request.
The provider must resolve that selection to an explicitly authorized browser
tab and document. A HWND is not a tab ID. The provider must reject ambiguous
resolution or unsupported source coordinates, not attach to a guessed tab.

Output contains:

- `sessionId`: opaque source session identity.
- `documentId`: opaque original document identity.
- `width`, `height`: positive logical source dimensions, at most 8192 per side
  and 4,194,304 pixels in total.

Browser zoom, OS scale, screenshot pixel size and Hook logical dimensions are
separate quantities. The resolver must prove the mapping rather than changing
the user's device metrics to make a test pass.

### Poll

Input contains `sessionId`, `documentId`, and `afterFrameId`.
Output echoes the exact session/document tokens and optionally contains `frame`:

- `id`: positive safe integer, strictly increasing for new frames.
- `capturedAtMs`: positive safe integer, not decreasing within a session.
- `mime`: `image/png` or `image/jpeg` only.
- `base64`: padded base64, at most 1,398,104 characters and 1,048,576 decoded bytes.

Absent/null frame means no new frame. A mismatched document is a terminal source
failure, not implicit navigation recovery. Scroll and tab activation do not
change the document binding. Navigation/reload, tab close, grant revocation or
runtime restart invalidate it. Automatic rebind is forbidden.

### Close

Input contains `sessionId` and `documentId`. It is idempotent for an already
closed source. Output contains the payload protocol. Close releases the capture
binding, not the user's browser tab/window.

## Provider responsibilities and remaining validation

The Hook consumer alone cannot supply browser pixels. Provider implementations
must cover the responsibilities below; installed-candidate evidence is tracked
in the acceptance report linked above, not implied by this protocol document:

1. Browser extension/native transport installation and explicit browser grant.
2. Tab/document resolution and correct rectangle conversion from real selection.
3. Session-owned listeners, browser attachment, bounded frame storage and fair
   scheduling across document regions.
4. A short renewable lease so disconnect, dropped open response, malformed
   response, permission removal or disabled command cannot orphan a capture.
   Hook cannot invoke a revoked command merely to request cleanup; Loom/provider
   shutdown and lease expiry must release its resources independently.
5. Cancellation/deadlines and cleanup when a command times out or the runtime is
   stopped. The Hook bridge's pending-request timeout does not cancel arbitrary
   browser work by itself.
6. Independent interaction commands with document identity and input permissions;
   the current consumer intentionally advertises no browser input capability.
7. Real browser-to-Hook Unit acceptance and verified release packages. Ordinary
   DOM, virtualized elements, background video, cross-origin frames and
   real-gesture controls require separate evidence.

This protocol itself grants no browser permission, package trust or automatic
installation. The ordinary browser profile still needs explicit onboarding.
