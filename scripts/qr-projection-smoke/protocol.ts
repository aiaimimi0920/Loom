import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, randomUUID, sign, type KeyObject } from "node:crypto";
import { crc32, deflateSync } from "node:zlib";
import { object, textField, ProjectionRuntime, type JsonObject } from "./runtime.ts";

export interface Identity { id: string; key: KeyObject; token: string; name: string }
export interface Snapshot { imageBase64: string; width: number; height: number }
export interface Envelope {
    protocol: string; projectionId: string; serverOrigin: string;
    source: { deviceId: string; sessionId: string; unitId: string; revision: number };
    content: { kind: string; digest: string }; expiresAtMs: number; nonce: string;
    signature: { algorithm: string; keyId: string; value: string };
}

export function digest(snapshot: Snapshot): string {
    return createHash("sha256").update(Buffer.from(snapshot.imageBase64, "base64")).digest("hex");
}

export function png(red: number): Snapshot {
    const chunk = (name: string, data: Buffer): Buffer => {
        const payload = Buffer.concat([Buffer.from(name), data]);
        const length = Buffer.alloc(4);
        const checksum = Buffer.alloc(4);
        length.writeUInt32BE(data.length);
        checksum.writeUInt32BE(crc32(payload));
        return Buffer.concat([length, payload, checksum]);
    };
    const header = Buffer.alloc(13);
    header.writeUInt32BE(2, 0);
    header.writeUInt32BE(2, 4);
    header[8] = 8;
    header[9] = 6;
    const row = Buffer.from([0, red, 20, 40, 255, red, 20, 40, 255]);
    const image = Buffer.concat([
        Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header),
        chunk("IDAT", deflateSync(Buffer.concat([row, row]))), chunk("IEND", Buffer.alloc(0)),
    ]);
    return { imageBase64: image.toString("base64"), width: 2, height: 2 };
}

export class ProjectionClient {
    readonly runtime: ProjectionRuntime;

    constructor(runtime: ProjectionRuntime) { this.runtime = runtime; }

    async request(path: string, body: unknown, expected = 200, actor?: Identity | "admin", method = "POST"): Promise<JsonObject> {
        const headers: Record<string, string> = {};
        if (actor === "admin") headers.Authorization = `Bearer ${this.runtime.adminToken}`;
        else if (actor) {
            headers.Authorization = `Device ${actor.token}`;
            headers["X-Loom-Device-Nonce"] = randomUUID();
        }
        const reply = await this.runtime.request(path, body, headers, method);
        assert.equal(reply.status, expected, `${method} ${path} status`);
        return reply.body;
    }

    post(actor: Identity, operation: string, body: unknown, expected = 200): Promise<JsonObject> {
        return this.request(`/v1/projections/${operation}`, body, expected, actor);
    }

    async pair(name: string): Promise<Identity> {
        const keys = generateKeyPairSync("ed25519");
        const publicKey = keys.publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("base64");
        const result = await this.request("/v1/devices/requests", { name, kind: "computer", address: "127.0.0.1", publicKey });
        assert.ok(Array.isArray(result.pending), "pending devices missing");
        const pending = result.pending.map(object).find((entry) => entry.name === name);
        const id = textField(pending?.id, "device identifier");
        assert.match(id, /^[A-Za-z0-9._:/-]{1,160}$/, "unsafe device identifier");
        await this.request(`/v1/devices/${id}/approve`, {}, 200, "admin");
        const identity = { id, key: keys.privateKey, token: "", name };
        await this.session(identity);
        return identity;
    }

    async session(identity: Identity): Promise<void> {
        const challenge = await this.request("/v1/device-sessions/challenges", { deviceId: identity.id }, 201);
        const challengeId = textField(challenge.challengeId, "challenge identifier");
        const clientNonce = randomUUID();
        const message = ["loom.device-session.v1", identity.id, challengeId,
            textField(challenge.challenge, "challenge"), clientNonce].join("\n");
        const result = await this.request("/v1/device-sessions", {
            deviceId: identity.id, challengeId, clientNonce,
            signature: sign(null, Buffer.from(message), identity.key).toString("base64"),
        }, 201);
        identity.token = textField(result.token, "device session token");
    }

    invitation(identity: Identity, snapshot: Snapshot, expiresAtMs = Date.now() + 300_000): Envelope {
        const envelope: Envelope = {
            protocol: "neuro.qr-projection.v1", projectionId: `projection:${randomUUID().replaceAll("-", "")}`,
            serverOrigin: this.runtime.origin,
            source: { deviceId: identity.id, sessionId: "qr-smoke-session", unitId: "qr-smoke-source", revision: 1 },
            content: { kind: "sticker", digest: digest(snapshot) }, expiresAtMs,
            nonce: randomUUID().replaceAll("-", ""), signature: { algorithm: "ed25519", keyId: identity.id, value: "" },
        };
        const message = [envelope.protocol, envelope.projectionId, envelope.serverOrigin, identity.id,
            envelope.source.sessionId, envelope.source.unitId, 1, envelope.content.kind,
            envelope.content.digest, expiresAtMs, envelope.nonce].join("\n");
        envelope.signature.value = sign(null, Buffer.from(message), identity.key).toString("base64url");
        return envelope;
    }

    async setEnabled(identity: Identity, enabled: boolean): Promise<void> {
        await this.request(`/v1/devices/${identity.id}`, {
            name: identity.name, kind: "computer", address: "127.0.0.1", enabled,
        }, 200, "admin", "PUT");
    }
}

export function acceptance(envelope: Envelope, receiverUnitId = "qr-smoke-receiver") {
    return { envelope, expectedRevision: 1, expectedDigest: envelope.content.digest, receiverUnitId, confirmed: true };
}

export function update(envelope: Envelope, priorRevision: number, snapshot: Snapshot) {
    return { projectionId: envelope.projectionId, sourceSessionId: envelope.source.sessionId,
        priorRevision, revision: priorRevision + 1, digest: digest(snapshot), snapshot };
}

export function expectCode(reply: JsonObject, expected: string): void {
    assert.equal(object(reply.error).code, expected, "projection error code");
}
