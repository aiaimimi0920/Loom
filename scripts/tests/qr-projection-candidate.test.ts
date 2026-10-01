import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync, mkdtempSync, renameSync, rmSync, symlinkSync, truncateSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { candidate, scanBoundedFile } from "../qr-projection-smoke/candidate.ts";

function fixture() {
    const directory = mkdtempSync(join(tmpdir(), "loom-qr-candidate-test-"));
    const executable = join(directory, "runtime", "loom-daemon.exe");
    const manifestPath = join(directory, "manifest.json");
    mkdirSync(join(directory, "runtime"));
    const data = Buffer.from("synthetic daemon bytes");
    writeFileSync(executable, data);
    const manifest = {
        app: "Loom", versionId: "test",
        exes: [{ name: "loom-daemon.exe", bytes: data.length, sha256: createHash("sha256").update(data).digest("hex") }],
    };
    const save = () => writeFileSync(manifestPath, "\uFEFF" + JSON.stringify(manifest));
    save();
    return { directory, executable, manifestPath, manifest, save, cleanup: () => rmSync(directory, { recursive: true, force: true }) };
}

test("candidate keeps manifest, BOM, digest and executable metadata behavior", () => {
    const f = fixture();
    try {
        assert.deepEqual(candidate(f.directory), {
            executable: f.executable, bytes: f.manifest.exes[0].bytes,
            sha256: f.manifest.exes[0].sha256, versionId: "test",
        });
    } finally { f.cleanup(); }
});

for (const failure of ["digest", "size", "empty", "manifest budget", "daemon budget", "directory"]) {
    test(`candidate rejects ${failure}`, () => {
        const f = fixture();
        try {
            if (failure === "digest") f.manifest.exes[0].sha256 = "0".repeat(64);
            if (failure === "size") f.manifest.exes[0].bytes += 1;
            f.save();
            if (failure === "empty") truncateSync(f.executable, 0);
            if (failure === "manifest budget") truncateSync(f.manifestPath, 2 * 1024 * 1024 + 1);
            if (failure === "daemon budget") truncateSync(f.executable, 512 * 1024 * 1024 + 1);
            if (failure === "directory") { rmSync(f.executable); mkdirSync(f.executable); }
            assert.throws(() => candidate(f.directory));
        } finally { f.cleanup(); }
    });
}

test("pathname replacement never redirects an already-open bounded read", { skip: process.platform === "win32" }, () => {
    const f = fixture();
    const original = Buffer.alloc(128 * 1024, 65);
    writeFileSync(f.executable, original);
    const chunks: Buffer[] = [];
    try {
        const bytes = scanBoundedFile(f.executable, original.length, (chunk) => {
            chunks.push(Buffer.from(chunk));
            if (chunks.length === 1) {
                renameSync(f.executable, f.executable + ".old");
                writeFileSync(f.executable, Buffer.alloc(original.length + 1, 66));
            }
        });
        assert.equal(bytes, original.length);
        assert.deepEqual(Buffer.concat(chunks), original);
    } finally { f.cleanup(); }
});

for (const change of ["growth beyond cap", "growth below cap", "truncate"]) {
    test(`mutation while reading fails closed: ${change}`, () => {
        const f = fixture();
        writeFileSync(f.executable, Buffer.alloc(128 * 1024, 65));
        let read = 0;
        try {
            assert.throws(() => scanBoundedFile(f.executable, 192 * 1024, (chunk) => {
                read += chunk.length;
                if (read === 64 * 1024) {
                    if (change === "truncate") truncateSync(f.executable, 0);
                    else appendFileSync(f.executable, Buffer.alloc(change === "growth beyond cap" ? 128 * 1024 : 1, 66));
                }
            }), /read budget|changed during verification/);
            assert.ok(read <= 192 * 1024, "never expose bytes beyond budget");
        } finally { f.cleanup(); }
    });
}

test("consumer errors propagate and leave input cleanup usable", () => {
    const f = fixture();
    try {
        assert.throws(() => scanBoundedFile(f.executable, 1024, () => { throw new Error("consumer failed"); }), /consumer failed/);
        // The failure must not prevent subsequent cleanup or bounded reads.
        rmSync(f.executable);
        writeFileSync(f.executable, "replacement");
        let data = "";
        scanBoundedFile(f.executable, 1024, (chunk) => { data += chunk.toString(); });
        assert.equal(data, "replacement");
    } finally { f.cleanup(); }
});

test("candidate rejects executable symlinks escaping its package", { skip: process.platform === "win32" }, () => {
    const f = fixture();
    const external = fixture();
    try {
        rmSync(f.executable);
        symlinkSync(external.executable, f.executable);
        assert.throws(() => candidate(f.directory), /escaped package directory/);
    } finally { f.cleanup(); external.cleanup(); }
});

test("regular file check rejects non-file input before it is consumed", () => {
    const f = fixture();
    try {
        let consumed = false;
        assert.throws(() => scanBoundedFile(f.directory, 1024, () => { consumed = true; }));
        assert.equal(consumed, false);
    } finally { f.cleanup(); }
});
