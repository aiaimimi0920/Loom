# Projection editing session v1

Status: shared-Loom authority, cross-Loom forwarding, persistent merge engine,
Hook native IPC, annotation rendering and mode/conflict controls are implemented.
Focused HTTP and browser validation is separate from packaged PC1/PC3 acceptance.
Existing PNG clients do not opt into this endpoint automatically.

## Ownership

POST /v1/projections/edit requires a paired keyed device session. Administrator
credentials do not impersonate a device. Each request names an existing projectionId;
that record must still authorize the source or its accepted receiver at the recorded
epoch. Unlinked, expired unaccepted and revoked records cannot authorize editing.
Knowing an edit session ID grants nothing by itself.

A session belongs to one source device, epoch and Unit. Up to eight projection
bindings share its document revision, mode and object versions. Only the source
can initialize, attach bindings or change direction. Receiver authority follows
the established projection binding; changing an invitation receive rule does not
implicitly revoke an already accepted link.

Loom stores opaque Hook annotation objects without executing or rasterizing them.
Object IDs must match each object's id. Allowed initial types: rect, round-rect,
ellipse, triangle, polygon, line, polyline, arrow, text, brush, highlighter, serial.
Hook must fully validate its own annotation fields before rendering. Crop, image
filters, eraser, flattened layers, mosaic and blur are outside this first contract.

## Requests and responses

All fields use camelCase; unknown request fields are rejected.

- initialize: projectionId, sessionId, expectedDigest, objects (ID-to-object map).
  Client generates edit:<32 lowercase hex digits> and durably retains the request.
  Digest must match the current projection PNG. Starts at revision 1, modeRevision 1,
  one_way. Identical retries return current state; altered initialization cannot
  reset an existing document.
- attach: projectionId, sessionId. Source device/epoch/Unit and image basis must
  match; a projection cannot bind to two sessions.
- read: projectionId. Discovers the session and returns its current document.
- mode: projectionId, sessionId, opId, baseModeRevision, mode (one_way or two_way).
  Source-only compare-and-set; advances revision and sets modeRevision to it for
  every attached target.
- apply: projectionId, sessionId, opId, baseRevision, modeRevision, changes.
  Each of 1-32 changes contains objectId and value. Null deletes while retaining
  a tombstone. Receiver writes require two_way; all writers need current
  modeRevision and the correct sessionId.
- checkpoint: projectionId, sessionId, expectedRevision. Source-only compare-and-set
  advances revision, modeRevision and checkpointRevision together, then removes
  receipts and tombstones. Retrying the same completed checkpoint is idempotent.

Response: schema=neuro.projection-edit.v1, sessionId, basis (digest, width, height),
revision, modeRevision, checkpointRevision, receiptCount, mode and objects. Each object has its last mutation revision
and value (possibly null). Other binding IDs, internal receipts and credentials
are never included.

Ordinary projection read/accept responses may include the document in `editing`.
POST /v1/offline-projections/edit uses the existing signed peer transport to reach
the source Loom. Accepted target binding, peer trust and device epochs are checked
before forwarding and again after network I/O. Foreign actors are scoped to the
peer, target device and epoch; shared and offline targets use one authority.

The basis is immutable in this slice. Once bound, legacy PNG update returns
projection_edit_snapshot_locked instead of changing shared object coordinates.
Non-participating v1 projections retain existing behavior. Hook must separate the
base image from annotations before opt-in; a flattened PNG must not be imported
together with the objects already baked into it.

## Merge and replay

Older baseRevision is accepted if none of the changed objects has mutated since
that base. Different objects merge. Any same-object conflict rejects the whole
batch with 409 projection_edit_object_conflict; no subset commits. Tombstones stop
old-base writes from resurrecting deleted objects. Clients retain conflicting
local changes and request a user decision; they must not silently rebase and overwrite.

Receipts are session-scoped and bind opId to authenticated actor and canonical typed
request digest. Identical retries return current state without another mutation.
Changed request/actor returns projection_edit_operation_reused. Mode changes reject
old-mode receiver packets and fresh one-way receiver writes. Explicit sessionId
fences delayed writes from previous sessions.

After a checkpoint, writes below checkpointRevision or with the previous
modeRevision are rejected even though their receipts have been removed. A client
must read and preserve unacknowledged changes for explicit reconciliation; it must
not reinterpret a missing receipt as permission to replay an old mutation.

Mode, document, session and basis mismatches use explicit 409 codes:
projection_edit_mode_conflict, projection_edit_revision_conflict,
projection_edit_session_mismatch and projection_edit_basis_conflict. Reconcile by
read. Existing projection serialization returns 429 projection_busy rather than
queuing unbounded work; retries retain the original request.

## Persistence and limits

Session files are beside the device registry under projection-edits/. Atomic writes
with restricted permissions precede in-memory publication. An ambiguous write
failure blocks subsequent editing until reopen, avoiding writes from stale memory.
Startup uses bounded reads and rejects corrupt/unknown fields, duplicate bindings,
symlinks and invalid filenames. No existing v1 record schema is rewritten; an older
daemon can ignore the separate store.

Limits: 64 sessions; 8 bindings/session; 256 objects including tombstones;
256 receipts/session; 16 KiB compact JSON/object; 256 KiB/request and persisted
session; 8 MiB aggregate persisted sessions. File accounting uses the actual
pretty-JSON encoding plus newline. Receipts are not silently evicted; a full ledger
fails with projection_edit_log_full. Hook requests a source checkpoint before the
receipt limit, or when tombstones approach the object limit. Live objects still
count toward the 256-object limit; checkpoints do not expand that budget.

Initialization/attachment reclaims expired or revoked shared and offline bindings
before admitting new sessions. Explicit unlink releases its binding; removal of the
last binding removes the session. Cold-path pruning is bounded, not a background
expiry timer. There is no in-place session reset/detach API yet. Existing unlink remains
available. Receipt identity is scoped to an editing session, not all future sessions.

## Follow-up

Hook separates a frozen base image from supported annotations and durably journals
pending requests before sending. Same-object and recovery conflicts keep local
edits until the user chooses local or remote state. Isolated Windows candidates
passed the PC1/PC3 shared/cross-Loom editing and mixed-binding stop matrix using
pinned SSH forwarding. The batch receiver used two Loom identities on one physical
PC3; this does not establish direct-LAN or additional-machine coverage. Base-image
changes within an editing session remain outside this contract. Official accounts,
friends, NAT traversal and premium relay remain reserved.
