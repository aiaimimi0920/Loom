import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { closeSync, constants, fstatSync, openSync, readSync, realpathSync } from "node:fs";
import { isAbsolute, join, relative } from "node:path";
import { object, textField } from "./runtime.ts";

// Check and consume one open file, so replacing its pathname cannot bypass the
// size check. Enforce the cap during reading as well, including concurrent growth.
export function scanBoundedFile(file: string, maximumBytes: number, consume: (chunk: Buffer) => void): number {
    assert.ok(Number.isSafeInteger(maximumBytes) && maximumBytes > 0, "invalid file budget");
    const descriptor = openSync(file, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0));
    try {
        const before = fstatSync(descriptor);
        assert.ok(before.isFile(), "candidate input must be a regular file");
        assert.ok(before.size <= maximumBytes, "candidate input is too large");
        const buffer = Buffer.allocUnsafe(Math.min(64 * 1024, maximumBytes + 1));
        let bytes = 0;
        while (true) {
            const count = readSync(descriptor, buffer, 0, Math.min(buffer.length, maximumBytes + 1 - bytes), null);
            if (count === 0) break;
            bytes += count;
            assert.ok(bytes <= maximumBytes, "candidate input exceeded its read budget");
            consume(buffer.subarray(0, count));
        }
        const after = fstatSync(descriptor);
        assert.ok(bytes === before.size && after.size === before.size
            && after.mtimeMs === before.mtimeMs, "candidate input changed during verification");
        return bytes;
    } finally {
        closeSync(descriptor);
    }
}

export function candidate(packageDir: string) {
    const chunks: Buffer[] = [];
    scanBoundedFile(join(packageDir, "manifest.json"), 2 * 1024 * 1024, (chunk) => chunks.push(Buffer.from(chunk)));
    const manifest = object(JSON.parse(Buffer.concat(chunks).toString("utf8").replace(/^\uFEFF/, "")));
    assert.equal(manifest.app, "Loom", "candidate application");
    assert.ok(Array.isArray(manifest.exes), "candidate executable list missing");
    const entry = manifest.exes.map(object).find((exe) => exe.name === "loom-daemon.exe");
    assert.ok(entry, "daemon metadata missing");
    const executable = realpathSync(join(packageDir, "runtime", "loom-daemon.exe"));
    const within = relative(packageDir, executable);
    assert.ok(!isAbsolute(within) && !within.startsWith(".."), "daemon escaped package directory");
    const hash = createHash("sha256");
    const bytes = scanBoundedFile(executable, 512 * 1024 * 1024, (chunk) => { hash.update(chunk); });
    assert.ok(bytes > 0, "invalid daemon size");
    assert.equal(bytes, entry.bytes, "candidate daemon size mismatch");
    const sha256 = hash.digest("hex");
    assert.equal(sha256, entry.sha256, "candidate daemon digest mismatch");
    return { executable, bytes, sha256, versionId: textField(manifest.versionId, "candidate version") };
}
