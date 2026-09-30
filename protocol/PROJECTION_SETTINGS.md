# Projection groups and recipient policy

Status: persistent administrator API and shared/offline-peer enforcement implemented.
Desktop group/rule editing and Hook current-Loom receiver migration are implemented.
Hook group target selection remains in development.
Official user authentication is reserved, not implemented.

## Administrator API

GET /v1/projection-settings returns storageVersion=1, revision, groups and rules.
It also returns targets (approved keyed local devices and verified peer directory
entries) and peerDirectory.status (complete, partial, busy or unavailable).
Directory data is advisory; unavailable members are preserved in saved settings.
PUT on the same path replaces groups and rules using expectedRevision. Both routes
require the local administrator credential; paired Hook credentials cannot read or
write the administrative document. Writes require all rule targets to be approved,
enabled keyed devices on this Loom. Unknown fields and invalid references fail.

Example PUT body (device identifiers here are illustrative):

    {
      "expectedRevision": 0,
      "groups": [{
        "groupId": "team", "name": "Team", "members": [
          {"deviceId": "local-source"},
          {"deviceId": "remote-source", "peerId": "loom-<64 lowercase hex characters>"}
        ]
      }],
      "rules": [{
        "deviceId": "local-receiver", "policy": "confirm",
        "whitelist": {"devices": [], "groups": ["team"], "users": []},
        "blacklist": [{"deviceId": "blocked-source"}]
      }]
    }

Policies are auto, confirm, reject. No rule means confirm. Device references are
scoped by optional peerId: an unscoped local device never matches a foreign device
with the same deviceId. Offline peerId is taken from authenticated pinned-peer
transport, not from the invitation's display name or an arbitrary body field.
Groups are this administrator's local definitions, not self-asserted source groups.

Decision order: any matching whitelist device/group/verified user yields auto;
otherwise a matching blacklist device yields reject; otherwise use the general
policy. Current v1/offline callers supply no verified account user, so a stored
user whitelist cannot authorize a delivery yet. All normal pairing, key, epoch,
signature, target-binding, expiry and image validation still apply.

## Delivery enforcement

POST /v1/projections/targets additionally returns settingsRevision and groups:
each group contains groupId, name, targetIds and unavailableCount. Group members
are expanded by Loom against that caller's current bounded local/verified peer
directory, using (peerId, deviceId), never a display name or an unscoped remote ID.
Only directory target IDs are disclosed; unavailable member identities and recipient
rules are omitted. Empty groups remain visible, and unavailable stored members are
not deleted. Overlapping groups may reference the same target ID; the batch sender
must deduplicate the selected union. Directory membership does not authorize a
send: create and peer offer continue to recheck live recipient authority.

The friends field is {status: "unavailable", reason:
"official_account_not_implemented"}; it does not represent a signed-in account.
Hook now consumes this directory for device/group multiselect, persistent source
bindings and bounded batch creation. Successful saved bindings are not recreated
on partial retry. Hook persists the signed v1 invitation and original PNG before
targeted creation, then reconciles that projection ID through source read before
retrying. Shared-Loom replay errors are not treated as new invitations. Offline
source read uses the saved local route; retries never regenerate the signed identity.
This client journal does not change server authorization, expiry, epoch or policy
checks, and is separate from ordinary workspace persistence.

Shared-Loom targeted create checks the recipient policy. Pending invitation inspect
and accept recheck it. Shared target discovery evaluates the current source identity
and hides rejected recipients. Shared and offline inbox entries carry receivePolicy
(auto or confirm); rejected pending invitations are omitted. Hook follows this
per-invitation decision instead of overriding it with its legacy local auto setting.
Absent receivePolicy remains readable from older Looms using the legacy setting.

Offline offers check the authenticated foreign source namespace on the receiving
Loom. Pending inspect/accept check before peer I/O and again before publishing the
response, including recovery of a newly accepted peer response through read.
Signed peer errors preserve projection_receiver_rejected and
projection_settings_unavailable rather than disguising a rejection as a bad network.

Changing policy does not silently delete existing accepted stickers or terminate
established associations. An already accepted invitation can recover after restart.
A newly rejected pending invitation cannot be accepted while rejected, including
by using an old confirmation dialog. A policy change may hide pending invitations;
this does not manufacture a displayed receipt or remove the sender's source sticker.

Hook's disabled presence still means this Hook is not participating in delivery.
Loom cannot deliver to a stopped or non-polling Hook. Hook follows its current Loom
automatically; only participation pause/resume remains local. Fresh installs follow
Loom, explicitly saved legacy disabled settings migrate to pause, and legacy auto
does not override the authoritative invitation decision. Hook clears stale manual
origins before asynchronous context discovery and fails closed on context errors.

## Storage and bounds

The private document is projection-settings/settings.json beside the device
registry. It uses an exclusive writer lock, bounded reads, restrictive permissions
and atomic replacement. A corrupt existing document fails daemon startup. An
uncertain persistence failure makes subsequent decisions fail closed until reopen.

Limits: 256 KiB JSON, 32 groups, 64 recipient rules, 64 members per group and 64
entries per whitelist/blacklist category, 128-byte group names, JavaScript-safe
revision integers. Duplicate groups, targets, members and dangling whitelist group
references fail validation. A stale expectedRevision returns HTTP 409 with
projection_settings_revision_conflict; clients must not blindly retry or overwrite.

Focused validation: projection_settings unit tests cover priority, namespace,
unverified users, writer ownership, conflict, restart and corrupt files. Real daemon
HTTP tests cover shared-Loom and two-Loom offline create/inbox/inspect/accept,
administrator scope and policy updates between invitation creation and acceptance.
