import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { compareFiles, compareLiveRelay } from '../compare-live-relay.mjs';
import { Measurement } from '../live-relay-measurement.mjs';
import { snapshot } from './live-relay-measurement-fixture.mjs';

function input(codec, bytes) {
  const measurement = new Measurement('live:test');
  for (let index = 0; index < 3; index++) {
    const raw = snapshot({ publishedFrames: index + 1, lastFrameId: index + 1 });
    Object.assign(raw.mediaDiagnostics, { forwardedFrames: index + 1,
      receivedBinaryBytes: (index + 1) * bytes, forwardedBinaryBytes: (index + 1) * bytes });
    Object.assign(raw.mediaDiagnostics.lastForward, { frameId: index + 1, wireCodec: codec, binaryBytes: bytes });
    measurement.add(raw, index * 500);
  }
  return {
    conditions: {
      hookSha256: 'a'.repeat(64), loomSha256: 'b'.repeat(64), contentFixtureSha256: 'c'.repeat(64),
      captureSettingsSha256: 'd'.repeat(64), networkConditionsSha256: 'e'.repeat(64),
      width: 64, height: 32, targetFps: 30, scenario: 'static-text',
    },
    report: { schemaVersion: 1, status: 'completed', stopReason: 'duration',
      settings: { durationMs: 1100, intervalMs: 500, requestTimeoutMs: 1000 }, ...measurement.report() },
  };
}
const pair = () => [input('raw_bgra', 8256), input('jpeg', 2064)];

test('compares actual deltas, not imported summaries or unverified presentation claims', () => {
  const [raw, jpeg] = pair();
  jpeg.report.rates.forwardedBinaryBytesPerSecond = 999999;
  jpeg.report.counterDelta.forwardedBinaryBytes = 1;
  jpeg.conditions.privateToken = 'PRIVATE_SECRET';
  const result = compareLiveRelay(raw, jpeg);
  assert.equal(result.jpegToRawBytesPerWriteRatio, 0.25);
  assert.equal(result.bytesPerWriteReductionPercent, 75);
  assert.equal(result.candidate.rates.forwardedBinaryBytesPerSecond, 4128);
  assert.equal(result.candidate.observedForwards.population, 'observed-lastForward-only');
  assert.equal(result.candidate.rates.receiverFps, null);
  assert.equal(result.endToEndVerdict, 'not-established');
  assert.equal(result.conditionsVerified, false);
  assert.ok(!JSON.stringify(result).includes('PRIVATE_'));
});

test('rejects mismatched declared package, content, capture, network, size, rate and scenario', () => {
  for (const key of Object.keys(pair()[0].conditions)) {
    const [raw, jpeg] = pair();
    jpeg.conditions[key] = key.endsWith('Sha256') ? 'f'.repeat(64)
      : key === 'scenario' ? 'motion' : jpeg.conditions[key] + 1;
    assert.throws(() => compareLiveRelay(raw, jpeg), /comparison_conditions_mismatch/, key);
  }
});

test('rejects failed, closed, mixed-codec, reset, offline and multi-viewer evidence', () => {
  const mutations = [
    value => { value.report.status = 'failed'; },
    value => { value.report.stopReason = 'session-closed'; },
    value => { value.report.evidence = 'physical-display'; },
    value => { value.report.samples[1].viewerConnections = 2; },
    value => { value.report.samples[1].sourceConnected = false; },
    value => { value.report.samples[1].closed = true; },
    value => { value.report.samples[1].epoch = 2; },
    value => { value.report.samples[1].identity = 'f'.repeat(32); },
    value => { value.report.samples[1].lastForward.wireCodec = 'raw_bgra'; },
    value => { value.report.samples[1].lastForward.viewer = 'f'.repeat(32); },
    value => { value.report.samples[1].lastForward = null; value.report.samples[2].lastForward.viewer = 'f'.repeat(32); },
    value => { value.report.samples[1].lastForward.frameId = 0; },
    value => { value.report.samples[1].lastForward.frameId = 1; },
    value => { value.report.samples[1].lastForward.writeSucceeded = false; },
    value => { value.report.samples[2].counters.forwardedFrames = 0; },
    value => { value.report.samples[2].counters.failedWrites = 1; },
    value => { value.report.samples[1].elapsedMs = 0; },
    value => { value.report.samples[1].counters.forwardedBinaryBytes = Number.MAX_VALUE; },
    value => { value.report.samples = []; },
  ];
  for (const mutation of mutations) {
    const [raw, jpeg] = pair(); mutation(jpeg);
    assert.throws(() => compareLiveRelay(raw, jpeg), undefined, mutation.toString());
  }
});

test('rejects mismatched observation windows and samples', () => {
  const [raw, jpeg] = pair();
  jpeg.report.samples[2].elapsedMs += 600;
  assert.throws(() => compareLiveRelay(raw, jpeg), /comparison_sampling_mismatch/);
  jpeg.report.samples[2].elapsedMs = 1000;
  jpeg.report.settings.intervalMs = 400;
  assert.throws(() => compareLiveRelay(raw, jpeg), /comparison_sampling_mismatch/);
});

test('normalizes rates over actual windows and permits bounded scheduling jitter', () => {
  const [raw, jpeg] = pair();
  jpeg.report.samples[2].elapsedMs = 1003;
  const result = compareLiveRelay(raw, jpeg);
  assert.equal(result.candidate.windowMs, 1003);
  assert.equal(result.candidate.rates.forwardedBinaryBytesPerSecond, 4128 * 1000 / 1003);
  assert.equal(result.jpegToRawBytesPerWriteRatio, 0.25);
});

test('bounded files create UTF-8 evidence exclusively and preserve existing reports', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'loom-relay-comparison-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const paths = ['raw.json', 'jpeg.json', 'result.json'].map(name => join(directory, name));
  const [raw, jpeg] = pair();
  await writeFile(paths[0], JSON.stringify(raw)); await writeFile(paths[1], JSON.stringify(jpeg));
  const cli = fileURLToPath(new URL('../compare-live-relay.mjs', import.meta.url));
  const execution = await promisify(execFile)(process.execPath, [cli, paths[0], paths[1], join(directory, 'cli.json')],
    { windowsHide: true, timeout: 5000, maxBuffer: 16384 });
  assert.match(execution.stdout, /endToEndVerdict=not-established/);
  await compareFiles(...paths);
  const first = await readFile(paths[2]);
  assert.notDeepEqual([...first.subarray(0, 3)], [0xef, 0xbb, 0xbf]);
  assert.equal(JSON.parse(first).endToEndVerdict, 'not-established');
  await assert.rejects(compareFiles(...paths), /EEXIST/);
  assert.deepEqual(await readFile(paths[2]), first);
  await writeFile(paths[0], Buffer.alloc(16 * 1024 * 1024 + 1));
  await assert.rejects(compareFiles(...paths), /comparison_input_invalid/);
  await writeFile(paths[0], '{PRIVATE_JSON');
  await assert.rejects(promisify(execFile)(process.execPath, [cli, ...paths],
    { windowsHide: true, timeout: 5000, maxBuffer: 16384 }), error => {
    assert.equal(error.code, 1); assert.ok(!error.stderr.includes('PRIVATE_'));
    assert.match(error.stderr, /comparison_failed/); return true;
  });
});
