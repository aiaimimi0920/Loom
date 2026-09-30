import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import { object, type JsonObject } from "./runtime.ts";
import { acceptance, digest, expectCode, png, ProjectionClient, update, type Snapshot } from "./protocol.ts";

function expectImage(reply: JsonObject, revision: number, snapshot: Snapshot): void {
    assert.equal(reply.revision, revision, "received revision");
    assert.equal(reply.digest, digest(snapshot), "received digest");
    assert.equal(object(reply.snapshot).imageBase64, snapshot.imageBase64, "received PNG");
    assert.equal(reply.width, snapshot.width, "received width");
    assert.equal(reply.height, snapshot.height, "received height");
}

export async function runScenarios(client: ProjectionClient, checks: string[]): Promise<void> {
    const a = await client.pair("QR release source");
    const b = await client.pair("QR release receiver");
    const outsider = await client.pair("QR release outsider");
    assert.equal(new Set([a.id, b.id, outsider.id]).size, 3, "independent device identities");
    checks.push("three_independent_device_sessions");

    const initial = png(10);
    const envelope = client.invitation(a, initial);
    const create = { envelope, snapshot: initial };
    await client.request("/v1/projections/create", create, 401);
    await client.request("/v1/projections/create", create, 403, "admin");
    checks.push("public_and_admin_cannot_impersonate_source");

    const tampered = { ...envelope, serverOrigin: "https://invalid.example.test" };
    expectCode(await client.post(a, "create", { envelope: tampered, snapshot: initial }, 403), "projection_signature_invalid");
    const expired = client.invitation(a, initial, Date.now() - 1);
    expectCode(await client.post(a, "create", { envelope: expired, snapshot: initial }, 410), "projection_invitation_expired");
    expectCode(await client.post(a, "create", { envelope, snapshot: png(99) }, 400), "projection_digest_mismatch");
    expectCode(await client.post(a, "create", { envelope, snapshot: { ...initial, width: 8193 } }, 413), "projection_image_budget");
    checks.push("signed_origin_expiry_digest_and_image_budget");

    await client.post(a, "create", create);
    expectCode(await client.post(a, "create", create, 409), "projection_invitation_replayed");
    const read = { projectionId: envelope.projectionId, knownRevision: 0 };
    await client.post(b, "read", read, 403);
    const preview = await client.post(b, "inspect", { envelope });
    expectImage(preview, 1, initial);
    assert.equal(preview.sourceName, a.name);
    const accept = acceptance(envelope);
    await client.post(b, "accept", { ...accept, confirmed: false }, 400);
    expectImage(await client.post(b, "accept", accept), 1, initial);
    expectImage(await client.post(b, "accept", accept), 1, initial);
    await client.post(b, "accept", acceptance(envelope, "other-unit"), 409);
    await client.post(outsider, "accept", accept, 409);
    await client.post(outsider, "read", read, 403);
    checks.push("preview_confirmation_and_single_receiver_binding");

    for (let prior = 1; prior <= 2; prior++) {
        await delay(550, undefined, { signal: client.runtime.signal });
        const snapshot = png(10 + prior);
        const request = update(envelope, prior, snapshot);
        const sent = await client.post(a, "update", request);
        assert.equal(sent.revision, prior + 1);
        assert.equal((await client.post(a, "update", request)).revision, prior + 1);
        await client.post(outsider, "update", request, 403);
        expectImage(await client.post(b, "read", read), prior + 1, snapshot);
    }
    const latest = { projectionId: envelope.projectionId, knownRevision: 3 };
    assert.equal((await client.post(b, "read", latest)).snapshot, null, "unchanged read omits image");
    await client.post(a, "update", update(envelope, 1, png(99)), 409);
    checks.push("two_updates_idempotent_retries_and_revision_rollback");

    await client.runtime.stop();
    await assert.rejects(() => client.runtime.request("/health"), "stopped daemon must be unreachable");
    await client.runtime.start();
    await client.post(b, "read", read, 401);
    await client.session(a);
    await client.session(b);
    expectImage(await client.post(b, "read", read), 3, png(12));
    checks.push("process_loss_and_persistent_snapshot_recovery");

    const unlink = { projectionId: envelope.projectionId };
    await client.post(b, "unlink", unlink);
    await client.post(b, "unlink", unlink);
    expectCode(await client.post(a, "read", read, 410), "projection_unlinked");
    await client.post(a, "update", update(envelope, 3, png(99)), 410);
    await client.runtime.stop();
    await client.runtime.start();
    await client.session(a);
    await client.session(b);
    expectCode(await client.post(b, "read", read, 410), "projection_unlinked");
    checks.push("unlink_is_idempotent_and_survives_restart");

    const revoked = client.invitation(a, initial);
    const revokedRead = { projectionId: revoked.projectionId, knownRevision: 0 };
    await client.post(a, "create", { envelope: revoked, snapshot: initial });
    await client.post(b, "accept", acceptance(revoked));
    await client.setEnabled(b, false);
    await client.post(b, "read", revokedRead, 401);
    await client.setEnabled(b, true);
    await client.session(b);
    expectCode(await client.post(b, "read", revokedRead, 403), "projection_access_denied");
    await client.post(b, "accept", acceptance(revoked), 409);
    await client.post(b, "unlink", { projectionId: revoked.projectionId }, 403);
    checks.push("receiver_reauthorization_does_not_restore_revoked_link");

    await client.setEnabled(a, false);
    await client.setEnabled(a, true);
    await client.session(a);
    expectCode(await client.post(a, "read", revokedRead, 403), "projection_source_revoked");
    await client.post(a, "update", update(revoked, 1, png(99)), 403);
    checks.push("source_reauthorization_does_not_restore_revoked_link");
}
