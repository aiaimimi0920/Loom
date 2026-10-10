import { open } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { MAX_SAMPLES, Measurement } from './live-relay-measurement.mjs';
import { validateOptions } from './measure-live-relay.mjs';

const HASH_FIELDS = ['hookSha256', 'loomSha256', 'contentFixtureSha256', 'captureSettingsSha256', 'networkConditionsSha256'];
const CONDITIONS = [...HASH_FIELDS, 'width', 'height', 'targetFps', 'scenario'];
const MAX_FILE_BYTES = 16 * 1024 * 1024;
const fail = () => { throw new Error('comparison_input_invalid'); };
const integer = value => { if (!Number.isSafeInteger(value) || value < 0) fail(); return value; };
const hash = (value, length) => typeof value === 'string' && new RegExp(`^[a-f0-9]{${length}}$`).test(value);

function validateConditions(value) {
  if (!value || HASH_FIELDS.some(key => !hash(value[key], 64))) fail();
  if (integer(value.width) < 1 || value.width > 16384 || integer(value.height) < 1 || value.height > 16384
    || integer(value.targetFps) < 1 || value.targetFps > 240
    || !['static-text', 'scrolling', 'motion'].includes(value.scenario)) fail();
  return Object.fromEntries(CONDITIONS.map(key => [key, value[key]]));
}

function summarize(report, codec) {
  if (report?.schemaVersion !== 1 || report.status !== 'completed' || report.stopReason !== 'duration'
    || report.evidence !== 'sampled-daemon-socket-writes' || !Array.isArray(report.samples)
    || report.samples.length < 2 || report.samples.length > MAX_SAMPLES) fail();
  const measurement = new Measurement('comparison');
  for (const key of ['durationMs', 'intervalMs', 'requestTimeoutMs']) integer(report.settings?.[key]);
  const { durationMs, intervalMs, requestTimeoutMs } = validateOptions({
    baseUrl: 'https://measurement.invalid', sessionId: 'comparison',
    durationMs: report.settings.durationMs, intervalMs: report.settings.intervalMs,
    requestTimeoutMs: report.settings.requestTimeoutMs,
  });
  let previous;
  let viewer;
  let observed = 0;
  for (const sample of report.samples) {
    if (!sample || !hash(sample.identity, 32) || sample.sourceConnected !== true || sample.closed !== false
      || sample.viewerConnections !== 1 || !sample.counters) fail();
    if (previous && (sample.identity !== previous.identity || sample.epoch !== previous.epoch
      || sample.elapsedMs <= previous.elapsedMs || sample.frameId < previous.frameId)) fail();
    const last = sample.lastForward;
    if (last && (!hash(last.viewer, 32) || last.wireCodec !== codec || last.epoch !== sample.epoch
      || last.frameId > sample.frameId || last.writeSucceeded !== true)) fail();
    if (previous?.lastForward && (!last || last.frameId < previous.lastForward.frameId
      || (sample.counters.forwardedFrames > previous.counters.forwardedFrames
        && last.frameId === previous.lastForward.frameId))) fail();
    if (last) {
      observed++;
      if (viewer && viewer !== last.viewer) fail();
      viewer = last.viewer;
    }
    // Reuse the sampler's bounds and delta calculations instead of trusting imported summaries.
    measurement.add({
      session: { sessionId: 'comparison', sourceDeviceId: sample.identity },
      epoch: sample.epoch, lastFrameId: sample.frameId, publishedFrames: sample.counters.publishedFrames,
      bufferedFrames: sample.bufferedFrames, sourceConnected: true, closed: false,
      viewerConnections: { viewer: 1 },
      mediaDiagnostics: { ...sample.counters, lastForward: last ? { ...last, viewerDeviceId: last.viewer } : null },
    }, sample.elapsedMs);
    previous = sample;
  }
  const result = measurement.report();
  if (!observed || !result.observedForwards.count || result.counterResets || result.epochTransitions
    || result.counterDelta.failedWrites || !result.counterDelta.publishedFrames
    || !result.counterDelta.forwardedFrames || !result.counterDelta.forwardedBinaryBytes) fail();
  const first = report.samples[0];
  return {
    codec, sampleCount: result.sampleCount, windowMs: result.comparableWindowMs,
    settings: { durationMs, intervalMs, requestTimeoutMs },
    identity: first.identity, epoch: first.epoch,
    counterDelta: result.counterDelta, rates: result.rates,
    binaryBytesPerSuccessfulWrite: result.counterDelta.forwardedBinaryBytes / result.counterDelta.forwardedFrames,
    observedForwards: result.observedForwards,
  };
}

// Conditions are operator declarations, not proof of visual quality or actual network equivalence.
export function compareLiveRelay(raw, jpeg) {
  const left = validateConditions(raw?.conditions), right = validateConditions(jpeg?.conditions);
  if (CONDITIONS.some(key => left[key] !== right[key])) throw new Error('comparison_conditions_mismatch');
  const baseline = summarize(raw?.report, 'raw_bgra'), candidate = summarize(jpeg?.report, 'jpeg');
  const { durationMs, intervalMs } = baseline.settings;
  // One scheduled interval allows boundary jitter, not a short run masquerading as a full pair.
  if (Object.keys(baseline.settings).some(key => baseline.settings[key] !== candidate.settings[key])
    || Math.abs(baseline.windowMs - candidate.windowMs) > intervalMs
    || Math.abs(baseline.sampleCount - candidate.sampleCount) > 1
    || [baseline, candidate].some(value => value.windowMs < Math.max(1, durationMs - 2 * intervalMs)
      || value.windowMs > durationMs + 1)) {
    throw new Error('comparison_sampling_mismatch');
  }
  const ratio = candidate.binaryBytesPerSuccessfulWrite / baseline.binaryBytesPerSuccessfulWrite;
  return {
    schemaVersion: 1, evidence: 'paired-sampled-daemon-socket-writes',
    conditions: left, conditionsVerified: false, viewerConnections: 1,
    baseline, candidate, jpegToRawBytesPerWriteRatio: ratio,
    bytesPerWriteReductionPercent: (1 - ratio) * 100,
    endToEndVerdict: 'not-established',
    limitations: ['operator-declared-conditions', 'single-viewer-only', 'sampled-not-all-frames',
      'socket-write-not-receiver-ack', 'no-visual-quality-comparison', 'no-cpu-gpu-measurement',
      'no-cross-device-latency', 'no-physical-presentation-proof', 'queue-age-and-adaptation-overlap'],
  };
}

async function readBounded(filePath) {
  const file = await open(filePath, 'r');
  try {
    const stat = await file.stat();
    if (!stat.isFile() || stat.size > MAX_FILE_BYTES) fail();
    const bytes = Buffer.alloc(stat.size + 1);
    let size = 0;
    while (size < bytes.length) {
      const read = await file.read(bytes, size, bytes.length - size, size);
      if (!read.bytesRead) break;
      size += read.bytesRead;
    }
    if (size !== stat.size) fail();
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, size)));
  } finally { await file.close(); }
}

export async function compareFiles(rawPath, jpegPath, outputPath) {
  const result = compareLiveRelay(await readBounded(rawPath), await readBounded(jpegPath));
  const output = await open(outputPath, 'wx', 0o600);
  try { await output.writeFile(`${JSON.stringify(result, null, 2)}\n`, 'utf8'); }
  finally { await output.close(); }
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  if (args.length !== 3) {
    console.error('Usage: node scripts/compare-live-relay.mjs <raw.json> <jpeg.json> <new-output.json>');
    process.exitCode = 1;
  } else {
    compareFiles(...args).then(() => console.log('comparison_written; endToEndVerdict=not-established')).catch(() => {
      // Do not print imported values, filesystem paths or raw JSON parse errors.
      console.error('comparison_failed: check matched conditions, complete samples and unused output path');
      process.exitCode = 1;
    });
  }
}
