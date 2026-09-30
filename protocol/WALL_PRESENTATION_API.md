# Wall presentation controls

This is the September 2026 presentation-control extension of the development
`loom.wall.v1` control API. It leaves immutable geometry unchanged and adds the
specific fields below. Unknown fields and enum values still fail closed. See
[WALL_CONTROL_API.md](WALL_CONTROL_API.md) for authentication, catalog CAS, lease
lifetimes and storage bounds.

## Manager operation and persistence

`PUT /v1/walls/presentation` accepts exactly
`{baseRevision, wallId, mode}` from an administrator. A paired presenter cannot
manage display modes. `mode` is `running`, `frozen` or `black`.

The operation uses the global catalog revision. A stale request returns 409
without replay; an unchanged mode with a current base revision is a no-op.
Changing the mode commits through the existing bounded, private, atomic wall
document. The successful response is the current wall state.

State and the storage-version-1 document have an optional `presentations` array:

```json
{
  "presentations": [
    {"wallId": "wall-1", "revision": 12, "mode": "frozen"}
  ]
}
```

Each existing wall has at most one control. Revisions are positive safe integers
no greater than the catalog revision. Only `frozen` and `black` are stored;
absence means `running`. Empty arrays are omitted on serialization. Removing a
wall removes its control. Device snapshots filter controls to member walls.

Freeze and black do not change the layout revision. Input admission checks the
control under the same wall lock as mapping and action admission, immediately
rejecting new Live events, controller acquisition/renewal, Art events and Art
confirmation decisions with `wall_presentation_paused`. The route prunes Live
controllers before responding, releasing held source keys/buttons even when the
terminal has not observed the display command. Cancellation of an already owned
Art request remains permitted; existing execution is not cancelled by a display
control or by closing its view. View cleanup waits for accepted execution.

Returning to `running` removes the control and assigns the new catalog revision
to the layout. All applied acknowledgements are cleared. Old queued input must
fail even when the geometry is numerically identical; fresh input requires the
new layout's applied acknowledgement. Geometry or endpoint changes during a
pause keep the requested mode and invalidate the affected acknowledgements.

## Terminal application and reports

Heartbeat may additionally include `presentation: {revision, outcome}`. This
report has its own control revision, independent from `appliedRevision`:

| Requested mode | Outcome | `appliedRevision` |
| --- | --- | --- |
| frozen, complete retained frame | `applied` | Current layout revision |
| frozen, no valid retained frame | `frame_unavailable` | null |
| black | `applied` | null |

The daemon rejects stale reports and inconsistent combinations. A heartbeat
without a report remains accepted and clears any previous report. This permits
an in-flight ordinary heartbeat to finish after a new control is committed.
Reports are volatile and appear as optional `presentation` on endpoint status.
They disappear when the lease expires, the device is revoked, the mapping or
control changes, or the daemon restarts. Offline endpoints cannot report success.

The terminal applies controls in its serial presenter loop and reports actual
application. Freeze retains only a complete frame for the same endpoint, lease,
layout revision and pixel dimensions. It stops media decoding, Art polling and
input, retains admitted Art snapshots and image URLs, and discards late results.
It does not persist screen pixels or unsubmitted local form drafts. The source
program and already accepted Art work continue independently.

Black clears all visible content without downloading or decoding it. A fresh
terminal, changed mapping, lost output, expired authorization or reconnect cannot
recover a previously frozen frame from durable configuration. It stays black
and reports `frame_unavailable` when freeze is still requested. Black followed
by freeze also has no retained frame. Resume establishes the new mapping and
loads the current source state rather than replaying old display or input data.

Authorization/output-loss monitoring continues while paused. Losing it clears
retained content; a freeze does not extend the right to display old media.

These operations are asynchronous across terminals. A mode request is not an
application acknowledgement, and acknowledgements are not evidence of common
physical scanout time, Frame Lock or Genlock.

## Explicit compatibility boundary

[wall-presentation.v1.schema.json](schemas/wall-presentation.v1.schema.json)
defines this extension's strict objects. Ordinary documents/state/heartbeats
without controls retain their prior shape. Readers predating this extension
reject state containing its new fields; they cannot acknowledge freeze or black.
Upgrade the daemon, management client and terminals together before relying on
these controls. An old terminal clearing output after strict-parse failure is
not reported as a successfully frozen frame.

An old storage reader rejects a document containing controls without overwriting
it. This extension does not silently downgrade or translate persisted controls.
No optional-field exception is granted to other geometry or control objects.

The management UI retains a dirty geometry draft and its original CAS revision
when display controls change the catalog. Only an unchanged, current, saved draft
is refreshed to the successful control response. Conflicted drafts require an
explicit reload and cannot overwrite a newer layout.
