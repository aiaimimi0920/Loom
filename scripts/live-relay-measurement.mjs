import { createHash } from 'node:crypto';

export const MAX_SAMPLES = 6000;
const COUNTERS = ['publishedFrames', 'receivedBinaryBytes', 'sourceSequenceGaps', 'bufferEvictions',
  'forwardedFrames', 'forwardedBinaryBytes', 'viewerSkippedFrames', 'failedWrites'];
const DURATIONS = ['queueAgeMs', 'adaptationMs', 'socketWriteMs'];
const fingerprint = (value) => createHash('sha256').update(value).digest('hex').slice(0, 32);
const validId = (value) => typeof value === 'string' && /^[A-Za-z0-9_.:/-]{1,160}$/.test(value);

function integer(value, max = Number.MAX_SAFE_INTEGER) {
  if (!Number.isSafeInteger(value) || value < 0 || value > max) throw new Error('measurement_snapshot_invalid');
  return value;
}

function boolean(value) {
  if (typeof value !== 'boolean') throw new Error('measurement_snapshot_invalid');
  return value;
}

// 只在内存接触完整会话；报告白名单不保留窗口、观察值、凭证或原始设备标识。
export function selectSnapshot(raw, sessionId, elapsedMs) {
  const diagnostics = raw?.mediaDiagnostics;
  if (raw?.session?.sessionId !== sessionId || !validId(raw.session.sourceDeviceId) || !diagnostics
    || !raw.viewerConnections || typeof raw.viewerConnections !== 'object' || Array.isArray(raw.viewerConnections)) {
    throw new Error('measurement_snapshot_invalid');
  }
  const connections = Object.values(raw.viewerConnections);
  if (connections.length > 32) throw new Error('measurement_snapshot_invalid');
  const selected = {
    elapsedMs: integer(elapsedMs), identity: fingerprint(JSON.stringify([sessionId, raw.session.sourceDeviceId])),
    epoch: integer(raw.epoch), frameId: integer(raw.lastFrameId),
    bufferedFrames: integer(raw.bufferedFrames, 3), sourceConnected: boolean(raw.sourceConnected),
    closed: boolean(raw.closed), viewerConnections: integer(connections.reduce((n, v) => n + integer(v, 65), 0), 65),
    counters: Object.fromEntries(COUNTERS.map((name) => [name, integer(name === 'publishedFrames'
      ? raw.publishedFrames : diagnostics[name])])), lastForward: null,
  };
  if (selected.epoch < 1) throw new Error('measurement_snapshot_invalid');
  const last = diagnostics.lastForward;
  if (last !== null && last !== undefined) {
    if (!validId(last.viewerDeviceId) || !['raw_bgra', 'jpeg', 'h264', 'unknown'].includes(last.wireCodec)) {
      throw new Error('measurement_snapshot_invalid');
    }
    selected.lastForward = {
      viewer: fingerprint(last.viewerDeviceId), epoch: integer(last.epoch), frameId: integer(last.frameId),
      binaryBytes: integer(last.binaryBytes), skippedFrames: integer(last.skippedFrames), wireCodec: last.wireCodec,
      writeSucceeded: boolean(last.writeSucceeded),
      ...Object.fromEntries(DURATIONS.map((key) => [key, integer(last[key])])),
    };
  }
  return selected;
}

function distribution(values) {
  if (!values.length) return { count: 0, min: null, p50: null, p95: null, max: null };
  const sorted = [...values].sort((a, b) => a - b);
  return { count: sorted.length, min: sorted[0], p50: sorted[Math.ceil(sorted.length * 0.5) - 1],
    p95: sorted[Math.ceil(sorted.length * 0.95) - 1], max: sorted.at(-1) };
}

export class Measurement {
  #sessionId;
  #limit;
  #samples = [];

  constructor(sessionId, limit = MAX_SAMPLES) {
    if (!validId(sessionId) || !Number.isInteger(limit) || limit < 1 || limit > MAX_SAMPLES) {
      throw new Error('measurement_options_invalid');
    }
    this.#sessionId = sessionId;
    this.#limit = limit;
  }

  add(raw, elapsedMs) {
    if (this.#samples.length >= this.#limit) throw new Error('measurement_sample_limit');
    const sample = selectSnapshot(raw, this.#sessionId, elapsedMs);
    const previous = this.#samples.at(-1);
    if (previous && elapsedMs < previous.elapsedMs) throw new Error('measurement_time_invalid');
    if (previous && sample.identity !== previous.identity) throw new Error('measurement_source_changed');
    this.#samples.push(sample);
    return sample;
  }

  report() {
    const total = Object.fromEntries(COUNTERS.map((key) => [key, 0]));
    let comparableWindowMs = 0;
    let counterResets = 0;
    let epochTransitions = 0;
    const observed = [];
    // 比较首个样本之后的增量；跨 epoch 或任何计数回退均重新建立基线。
    for (let i = 1; i < this.#samples.length; i++) {
      const before = this.#samples[i - 1];
      const after = this.#samples[i];
      if (after.epoch !== before.epoch) { epochTransitions++; continue; }
      if (COUNTERS.some((key) => after.counters[key] < before.counters[key])) { counterResets++; continue; }
      comparableWindowMs += after.elapsedMs - before.elapsedMs;
      for (const key of COUNTERS) total[key] = integer(total[key] + (after.counters[key] - before.counters[key]));
      const attempts = after.counters.forwardedFrames - before.counters.forwardedFrames
        + after.counters.failedWrites - before.counters.failedWrites;
      // 单槽覆盖意味着至多观察到该区间最后一次写入；重复轮询不重复计算。
      if (attempts > 0 && after.lastForward?.epoch === after.epoch) observed.push(after.lastForward);
    }
    const rate = (value) => comparableWindowMs > 0 ? value * 1000 / comparableWindowMs : null;
    const attempts = integer(total.forwardedFrames + total.failedWrites);
    return {
      evidence: 'sampled-daemon-socket-writes', sampleCount: this.#samples.length,
      comparableWindowMs, counterResets, epochTransitions, counterDelta: total,
      rates: { publishedFramesPerSecond: rate(total.publishedFrames), socketWritesPerSecond: rate(total.forwardedFrames),
        receivedBinaryBytesPerSecond: rate(total.receivedBinaryBytes),
        forwardedBinaryBytesPerSecond: rate(total.forwardedBinaryBytes), receiverFps: null },
      observedForwards: {
        population: 'observed-lastForward-only', count: observed.length,
        successful: observed.filter((sample) => sample.writeSucceeded).length,
        failed: observed.filter((sample) => !sample.writeSucceeded).length,
        unobservedWriteAttempts: attempts - observed.length, queueAgeIncludesAdaptation: true,
        ...Object.fromEntries(DURATIONS.map((key) => [key, distribution(observed.map((sample) => sample[key]))])),
      },
      limitations: ['sampled-not-all-frames', 'socket-write-not-receiver-ack', 'no-cross-device-latency',
        'no-physical-presentation-proof', 'queue-age-and-adaptation-overlap', 'no-cpu-gpu-measurement'],
      samples: this.#samples,
    };
  }
}
