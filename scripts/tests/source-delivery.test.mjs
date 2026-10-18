import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createSnapshot, selected, verifySnapshot } from '../source-delivery.mjs';

const required = ['Cargo.toml', 'Cargo.lock', 'apps/desktop/package-lock.json',
  'apps/desktop/src-tauri/Cargo.lock', 'framework-packages/runtime-host/Cargo.lock'];
function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'loom-source-delivery-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const repo = path.join(root, 'repo');
  for (const name of required) {
    const file = path.join(repo, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, 'uncommitted 中文\r\n');
  }
  return { root, repo, output: path.join(root, 'snapshot') };
}

test('explicit source selection excludes credentials, generated files and arbitrary roots', () => {
  for (const name of ['apps/daemon/.env', 'apps/auth.json', 'native/key.pem',
    'apps/desktop/node_modules/a.js', 'apps/desktop/dist/a.js', 'crates/target/a.rs',
    'private/passwords.txt', '.git/config']) assert.equal(selected(name), false, name);
  assert.equal(selected('apps/daemon/src/new.rs'), true);
  assert.equal(selected('crates/loom_tool_registry/src/credentials.rs'), true);
  assert.equal(selected('crates/loom_tool_registry/src/credentials/store.rs'), true);
  assert.equal(selected('apps/credentials.json'), false);
  for (const name of ['../Cargo.toml', 'apps/../Cargo.toml', 'C:/secret', 'apps\\secret']) {
    assert.throws(() => selected(name), /SOURCE_PATH_INVALID/);
  }
});

test('snapshot preserves dirty bytes, verifies hashes and refuses overwrite', t => {
  const { repo, output } = fixture(t);
  const result = createSnapshot(repo, output, required, 'example-head');
  assert.equal(result.binaryBinding, null);
  assert.equal(result.combinedApplicationSourceComplete, false);
  assert.deepEqual(fs.readFileSync(path.join(output, 'Cargo.toml')), fs.readFileSync(path.join(repo, 'Cargo.toml')));
  assert.equal(verifySnapshot(output).files.length, 5);
  assert.throws(() => createSnapshot(repo, output, required), /SOURCE_OUTPUT_EXISTS/);
  fs.appendFileSync(path.join(output, 'Cargo.toml'), 'tamper');
  assert.throws(() => verifySnapshot(output), /SOURCE_DIGEST_MISMATCH/);
});

test('rejects missing required locks, nested output, duplicate case and unlisted files', t => {
  const { repo, output } = fixture(t);
  assert.throws(() => createSnapshot(repo, output, required.slice(1)), /SOURCE_REQUIRED_FILE_MISSING/);
  assert.throws(() => createSnapshot(repo, path.join(repo, 'snapshot'), required), /SOURCE_OUTPUT_INSIDE_REPO/);
  assert.throws(() => createSnapshot(repo, output, [...required, 'apps/A.rs', 'apps/a.rs']), /SOURCE_PATH_COLLISION/);
  createSnapshot(repo, output, required);
  fs.writeFileSync(path.join(output, 'injected.txt'), 'extra');
  assert.throws(() => verifySnapshot(output), /SOURCE_UNLISTED_FILE/);
});

test('rejects manifest traversal before reading outside snapshot', t => {
  const { repo, output } = fixture(t);
  const manifest = createSnapshot(repo, output, required);
  manifest.files[0].path = '../secret';
  fs.writeFileSync(path.join(output, 'source-manifest.json'), JSON.stringify(manifest));
  assert.throws(() => verifySnapshot(output), /SOURCE_PATH_INVALID/);
});

test('rejects directory links instead of copying their targets', t => {
  const { root, repo, output } = fixture(t);
  const target = path.join(root, 'private');
  fs.mkdirSync(target);
  fs.writeFileSync(path.join(target, 'data.rs'), 'not source');
  fs.symlinkSync(target, path.join(repo, 'apps', 'linked'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => createSnapshot(repo, output, [...required, 'apps/linked/data.rs']), /SOURCE_LINK_REJECTED/);
  assert.equal(fs.existsSync(output), false);
});
