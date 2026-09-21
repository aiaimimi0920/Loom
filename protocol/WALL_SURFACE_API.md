# Wall Art Surface presentation and input

This extends [WALL_CONTROL_API.md](WALL_CONTROL_API.md) with connection-owned
views of existing `loom.surface.v1` instances. The wall never installs an Art,
starts another source instance, or evaluates application code on a terminal.
All routes below are exact POST routes under `/v1/walls/surfaces/`. Unknown
request fields are rejected. Paired-device authentication and fresh request
nonces are required, including on loopback. An administrator bearer cannot
substitute for presenter identity.

## View grant

`binding` is `{endpointId, leaseId, revision}`: the endpoint must belong to the
paired device, hold its current presenter lease, advertise `surface_v1`, and
reference the requested instance in a placement intersecting that output.
`revision` is the current complete wall layout revision.

- `open`: `{binding, instanceId}` returns the state envelope below.
- `state`: `{view, snapshotRevision?}` returns current state. When the supplied
  snapshot revision is current, `snapshot` is null; all other state is refreshed.
- `close`: `{view}` releases that connection-owned attachment and its resources.
  Closing an already absent view is accepted. A late close cannot remove a
  successor with a different binding or attachment identity.

`view` is `{binding, instanceId, attachmentId}`. It is bound to this endpoint,
device and layout; it is not a general-purpose Surface attachment grant.
Ordinary paired-device Surface routes and streams exclude wall attachments.

Repeated placements of one instance on an endpoint share one attachment.
An independent instance must have exactly one mounted ordinary source view
when creating its wall mirror. Zero or multiple source views fail with
`surface_conflict`; the terminal cannot choose a source from an ambiguous
instance-only reference. Already created mirrors remain pinned to their source.
Shared instances use their existing shared snapshot and action fanout.

The canonical viewport uses the manifest's default view `fullSize`. Without
that view it is 800 x 600, increased as needed to satisfy `minimumSize`. A
minimum is not a preferred viewport size. Either axis above 4096 is rejected.
Crop and rotation operate on this viewport through the wall geometry contract.

The current terminal negotiates a declarative runtime, `loom_resource` transport
and `remote_resources`. Pointer, hover and keyboard capabilities reflect the
registered output; native multitouch is not advertised. Unsupported runtime or
required capability combinations fail explicitly instead of executing scripts.

## State envelope

```json
{
  "protocolVersion": "loom.wall.v1",
  "view": {
    "binding": {"endpointId": "tile", "leaseId": "lease", "revision": 2},
    "instanceId": "instance:example",
    "attachmentId": "attachment:example"
  },
  "width": 800,
  "height": 600,
  "generation": 1,
  "sequence": 0,
  "snapshot": null,
  "preview": null,
  "result": null,
  "confirmations": [],
  "pending": [],
  "failure": null
}
```

`snapshot` contains a declarative `SurfaceSnapshot`. Resource descriptors remain
present, but `resourceLeases` is always empty. The terminal resolves images
through the view-scoped route below; leases and private paths never escape.
`preview` and `result` are independently revisioned Surface commits. Preview
updates do not replace formal output; downstream persistence/export consumes
only the formal result. Scene state can change while the previous result remains.

`confirmations` contains only host confirmation requests owned by this view.
`pending` contains `{ack, actionId, cancelable}` for its outstanding events.
This includes queued/running continuous edits, even though their payloads never
enter the durable pending-event queue. Continuous edits report `cancelable:
false`; the existing explicit cancellation path requires a durable pending event.
`sequence` is the last admitted wall input sequence, including requests which
were subsequently rejected by the Art executor. After an uncertain response,
clients must recover state instead of inventing or retrying input order.

`failure` is null or `{requestId, code: "wall_surface_action_failed"}` for the
instance's latest failed/interrupted action, only if it belongs to this view's
retained accepted request identities and its current generation. At most 64
identities are retained, with unfinished work protected from eviction. No
runtime error message, stderr, path or arbitrary error code is exposed. A
cancelled request is not reported as a failed action. Closing the view discards
this transient history. Failure reporting preserves the last valid scene and
formal result; it does not turn failure into successful completion.

## Resources and actions

`image`: `{view, resourceId}` returns the same immutable raster envelope as
`/v1/walls/images/read`. The resource must be referenced by this view's snapshot
or the instance's current preview/formal commit. Content hash, stored length,
MIME, dimensions and allocation budgets are checked by the existing resource
and native image paths. No reusable resource lease is returned.

`event`: `{view, placementId, pixel, sequence, event}` submits a standard
`SurfaceEvent`. The endpoint must have acknowledged this layout as applied.
`pixel` uses physical output pixels; a hit test must select that exact visible,
interactive placement. Occlusion, crop, rotation and half-open edges use the
same geometry as presentation. The event's instance and attachment must equal
the view. Sequence starts at 1 and advances consecutively for that binding.
An event ID already owned by another attachment is rejected with HTTP 409
`surface_conflict`, including before the executor's idempotent-ack return.
Repeating a retained event ID within the same view preserves idempotency;
the wall sequence still advances. Clients must not use retries to bypass the
bounded acknowledgement retention window.
The response is the existing `SurfaceActionAck` with HTTP 202; acceptance does
not imply execution, confirmation or formal output completion.

`confirmation`: `{view, placementId, pixel, confirmationId, approved}` uses the
same applied-layout and hit-test checks and returns a Surface action ack.
Approval goes through the existing device-bound host confirmation broker.
`approved: false` rejects a waiting confirmation. An Art cannot approve itself.

`cancel`: `{view, requestId}` cancels only an accepted action still owned by this
attachment and declared cancelable. The existing executor seals cancellation
before late runtime responses can publish patches or formal results. Waiting
confirmation must be rejected through `confirmation`, not this route.

## Lifetime and bounds

Wall attachments, input history and presenter authority are ephemeral. Source
attachments, source application state, formal results and wall references are
not deleted when a terminal leaves. Unapproved confirmations are discarded on
close. Already accepted execution retains its attachment until it reaches a
terminal state; maintenance then releases its resources. Disconnecting is not
an implicit cancellation of work already accepted by Loom.

Each ephemeral attachment retains at most 64 event acknowledgements and their
ownership metadata. Admission retires the oldest completed entry when full;
64 unfinished entries cause a capacity refusal. Both continuous and discrete
wall acknowledgements are excluded from persistent source records. Releasing
the view removes its remaining acknowledgement history, while ordinary source
acknowledgements, state and formal results remain intact. Retired identities
cannot reappear as pending work or borrow a successor view's acknowledgement.

The server admits at most 128 wall views globally and 4 per endpoint. Hook
admits 4 Art sources, 16 Art placements, 512 scene nodes, depth 16, and 16 image
URLs totalling at most 16,777,216 pixels. Its single serial poll owner rejects
late generations and closes late grants. Removed resources revoke their URLs.
Control JSON traversal and native request/response budgets remain bounded.

Hook's Art queue contains at most 32 events; adjacent continuous events may
coalesce. Up to four workers preserve order per Art instance while waiting for
independent source execution. Native event submissions remain serial, reserving
request capacity for the poll owner, images and confirmation. A slow form does
not consume another instance's action waiting budget. Absolute scalar
`input`/`change` drafts on supported value controls
have a 30-second waiting budget, so cold Art execution does not discard the
newest text or a following field edit. Independent queued actions expire after
1.5 seconds. An action explicitly queued behind the same view's local edits can
wait for those edits within a 30-second total budget; after they settle, it has
at most 1.5 seconds to revalidate and submit. Reported edit failure or exhausted
execution wait discards that instance's queued successor work. Expiry is reported; stale
generations or mappings are discarded.
The host keeps at most 64 expiring confirmation anchors. Activating
Art releases Live input authority. Layout/lease loss removes old controls and
input authority before preparing the successor.

Before submitting queued input, Hook obtains a fresh state through the same
serial poll owner and waits for its earlier accepted work. It can refresh the
base snapshot revision only while the target control's action, properties and
layout remain unchanged; absolute value edits may account for a previous value
edit. A replaced control, attachment or generation is never retargeted. Already
submitted mutations are not retried after an uncertain response.

Unchanged controls retain their node identity across unrelated snapshot updates.
The host preserves local text and textarea drafts until outstanding edit promises
settle after a fresh outcome read, with a 30-second execution-wait budget and
separately bounded native requests. An accepted acknowledgement alone does not
settle the draft. Replacing the instance, attachment or node invalidates late
draft callbacks. This does not promise focus retention after DOM reordering.

## Verification

`cargo test --locked -p loom-daemon --lib wall_surface_http_` covers real signed
pairing, view/resource isolation, capability negotiation, input sequence,
confirmation, sanitized failure, ambiguous-source rejection and source survival.
Its packages contain test runtime stubs; it does not establish real Art process
or hardware acceptance. The Hook `Invoke-TileWallArtProbe.ps1` entry uses the
packaged binaries, installed Surface prototypes and an actual physical output.
See the implementation plan and stage records for executed scope and limits.
