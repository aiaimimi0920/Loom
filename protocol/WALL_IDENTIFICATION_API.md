# Physical display identification

This September 2026 development extension of `loom.wall.v1` adds a bounded
marker on one physical output. Authentication and lease ownership follow
[WALL_CONTROL_API.md](WALL_CONTROL_API.md). It is independent of persisted
[presentation controls](WALL_PRESENTATION_API.md) and does not change geometry.

## Endpoint metadata

An endpoint may register `display: {name, canIdentify}`. Both nested fields are
required when the object is present; null and unknown fields are rejected.
`name` is a nonblank string of at most 256 Unicode code points, with no C0 or DEL
control characters. `canIdentify` is a boolean capability declaration, not an
authorization grant. An absent object preserves the earlier endpoint shape.

Hook derives the name from the actual Windows output enumeration. Management
shows that name, stable device/output IDs, pixel size and advertised capabilities.
It does not assign a physical screen number from endpoint ordering. Metadata is
durable registration data; updating it follows the existing catalog CAS and
endpoint-lease invalidation rules.

## Manager command and volatile status

`POST /v1/walls/endpoints/identify` accepts exactly `{endpointId}` from an
administrator. The endpoint must belong to an approved, enabled device, have a
current online presenter lease and advertise identification. Unassigned outputs
are allowed. A frozen or black wall must resume before one of its outputs can
identify. Unsupported/offline endpoints return 409 with
`wall_identification_unsupported` / `wall_endpoint_offline`; paused walls return
`wall_presentation_paused`.

This operation uses no catalog CAS. It does not write storage or advance catalog
or layout revisions. Its response is the current wall state. Endpoint status
gains an optional object:

```json
{
  "identification": {
    "requestId": "124bda95-e35f-4b80-a12d-5097ae622231",
    "remainingMs": 10000,
    "applied": false
  }
}
```

One request belongs to the current presenter lease, for at most 10 seconds on
the daemon's monotonic clock. `remainingMs` is an integer from 1 through 10000.
Repeating the command while active returns the same request ID and remaining
deadline; it does not restart the timer. `applied` initially means false even
when the administrator request succeeds. Layout changes, display mode changes,
lease replacement/expiry, disconnect and daemon restart discard the request.
State hides it for revoked devices. It is never a durable scene or a source.

New Live control acquisition/renewal and Live/Art input fail immediately with
`wall_endpoint_identifying`. Existing Live keys/buttons are released before the
administrator response. The gate is scoped to this endpoint; peer outputs are
not paused. Accepted Art work continues, and cancellation of owned work remains
permitted. Source ownership, media access and ordinary source attachments are
unchanged.

## Presenter report and local lifecycle

`POST /v1/walls/endpoints/identify/report` accepts exactly
`{endpointId, leaseId, requestId, outcome}` from the paired owner with its current
lease. The daemon validates `requestId` as a UUID. `outcome` is `applied` or
`dismissed`; the response is `{protocolVersion: "loom.wall.v1", accepted: true}`.

An applied report marks only the matching active request; it does not extend
its deadline. Dismissal clears it. Late reports for an expired or replaced
request are harmless no-ops and cannot acknowledge a successor. Invalid owner
or lease remains an error, even for a late report. This acknowledgement is
separate from layout `appliedRevision` and presentation-control reports.

Hook pauses input before showing the marker, makes the covered content inert,
and reports after an animation-frame callback. A hidden WebView cannot block
the serial presenter indefinitely: the paint wait is bounded at 500 ms and
failure hides the marker without reporting application. This acknowledges
application rendering, not physical scanout or multi-display synchronization.

The local deadline starts at the beginning of the state read plus `remainingMs`,
conservatively including network delay. Repeated observations never extend it;
an expired/dismissed request cannot reopen from a delayed snapshot. Identifying
an unassigned screen does not wait for image, Live or Art content to load.
Heartbeat continues with null layout application while the marker is active.
After dismissal or expiry, normal presentation and input wait for a fresh
ordinary application/heartbeat cycle.

The marker shows the physical name, device/output identity, pixels and countdown.
Tab stays inside it. Its close button and Escape dismiss only the marker;
ordinary output Escape still exits the output when no marker is active.
Output loss, authorization loss and disposal clear the marker and timers.
Applied and dismissed reports are serialized; old work cannot affect a new lease.

## Schema and compatibility

[wall-identification.v1.schema.json](schemas/wall-identification.v1.schema.json)
defines command, status and report structures. Endpoint metadata is defined in
[wall.v1.schema.json](schemas/wall.v1.schema.json). Runtime validation additionally
checks ownership, active lease, declared capability and running display mode.

Earlier strict readers reject the new optional fields when present. Upgrade
daemon, management client and terminals together before registering display
metadata or requesting identification. An old store reader also rejects a
document with this metadata without overwriting it; there is no silent downgrade.
Documents without metadata and state without markers retain their earlier shape.
No general relaxation of unknown-field validation is introduced.

The manager keeps an unsaved layout and its original CAS revision when identifying
an output. It distinguishes waiting for an output report from an applied marker.
Source inventory reads retry failures at most twice, two seconds apart. The
separate source-refresh action preserves drafts and catalog revisions; failed
reads remain visible after the retry budget. Layout writes are never replayed.
