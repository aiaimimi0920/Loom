import assert from 'node:assert/strict';
import test from 'node:test';
import { Measurement, selectSnapshot } from '../live-relay-measurement.mjs';
import { snapshot } from './live-relay-measurement-fixture.mjs';

test('only fixed numeric/boolean fields and identity fingerprints leave the snapshot', () => {
  const raw = snapshot();
  raw.mediaDiagnostics.extra = 'PRIVATE_TOKEN';
  const selected = selectSnapshot(raw, 'live:test', 10);
  const json = JSON.stringify(selected);
  for (const value of ['PRIVATE_TITLE', 'PRIVATE_OCR', 'PRIVATE_TOKEN', 'source:a', 'viewer:a']) {
    assert.ok(!json.includes(value));
  }
  assert.equal(selected.viewerConnections, 1);
  assert.equal(selected.lastForward.frameId, 1);
  assert.match(selected.identity, /^[a-f0-9]{32}$/);
});

test('reject wrong identity, missing diagnostics, unsafe integers and invalid sample data', () => {
  for (const raw of [snapshot({ session: { sessionId: 'other' } }),
    snapshot({ mediaDiagnostics: undefined }), snapshot({ publishedFrames: 2 ** 53 }),
    snapshot({ bufferedFrames: 4 }), snapshot({ viewerConnections: { x: 66 } })]) {
    assert.throws(() => selectSnapshot(raw, 'live:test', 0), /measurement_snapshot_invalid/);
  }
  const raw = snapshot();
  raw.mediaDiagnostics.lastForward.wireCodec = 'PRIVATE_PAYLOAD';
  assert.throws(() => selectSnapshot(raw, 'live:test', 0), /measurement_snapshot_invalid/);
});

test('measure window deltas without counting pre-existing totals or repeated lastForward', () => {
  const m = new Measurement('live:test');
  m.add(snapshot(), 0);
  m.add(snapshot(), 100);
  const next = snapshot({ lastFrameId: 4, publishedFrames: 4 });
  Object.assign(next.mediaDiagnostics, { receivedBinaryBytes: 400, forwardedFrames: 4, forwardedBinaryBytes: 400 });
  next.mediaDiagnostics.lastForward.frameId = 4;
  m.add(next, 200);
  m.add(next, 300);
  const report = m.report();
  assert.equal(report.counterDelta.publishedFrames, 3);
  assert.equal(report.counterDelta.forwardedFrames, 3);
  assert.equal(report.observedForwards.count, 1);
  assert.equal(report.observedForwards.unobservedWriteAttempts, 2);
  assert.equal(report.observedForwards.queueAgeMs.p95, 12);
  assert.equal(report.observedForwards.queueAgeIncludesAdaptation, true);
  assert.equal(report.rates.publishedFramesPerSecond, 10);
  assert.equal(report.rates.receiverFps, null);
  assert.equal(report.comparableWindowMs, 300);
});

test('counter rollback and epoch changes start a new comparison segment', () => {
  const m = new Measurement('live:test');
  m.add(snapshot({ publishedFrames: 20 }), 0);
  m.add(snapshot(), 100);
  m.add(snapshot({ epoch: 2 }), 200);
  m.add(snapshot({ epoch: 2, publishedFrames: 2 }), 300);
  const report = m.report();
  assert.equal(report.counterResets, 1);
  assert.equal(report.epochTransitions, 1);
  assert.equal(report.counterDelta.publishedFrames, 1);
  assert.equal(report.comparableWindowMs, 100);
  assert.equal(report.observedForwards.count, 0);
});

test('a replaced source or non-monotonic sampling time fails rather than joining unrelated runs', () => {
  const m = new Measurement('live:test');
  m.add(snapshot(), 10);
  assert.throws(() => m.add(snapshot(), 9), /measurement_time_invalid/);
  assert.throws(() => m.add(snapshot({ session: { sessionId: 'live:test', sourceDeviceId: 'other' } }), 20),
    /measurement_source_changed/);
});

test('failed writes are observed without turning them into successful bytes or delivery', () => {
  const m = new Measurement('live:test');
  m.add(snapshot(), 0);
  const failed = snapshot();
  failed.mediaDiagnostics.failedWrites = 1;
  failed.mediaDiagnostics.lastForward.writeSucceeded = false;
  m.add(failed, 100);
  assert.equal(m.report().counterDelta.forwardedBinaryBytes, 0);
  assert.equal(m.report().observedForwards.failed, 1);
  assert.equal(m.report().observedForwards.successful, 0);
});

test('the retained sample count is bounded and one snapshot does not establish a rate', () => {
  const m = new Measurement('live:test', 2);
  m.add(snapshot(), 0);
  assert.equal(m.report().rates.publishedFramesPerSecond, null);
  m.add(snapshot(), 1);
  assert.throws(() => m.add(snapshot(), 2), /measurement_sample_limit/);
  assert.equal(m.report().samples.length, 2);
});

test('small deltas remain exact near the safe integer boundary', () => {
  const m = new Measurement('live:test');
  for (let i = 0; i < 4; i++) m.add(snapshot({ publishedFrames: Number.MAX_SAFE_INTEGER - 3 + i }), i * 100);
  assert.equal(m.report().counterDelta.publishedFrames, 3);
});
