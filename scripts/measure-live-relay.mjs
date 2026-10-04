import { open } from 'node:fs/promises';
import { isIP } from 'node:net';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { MAX_SAMPLES, Measurement } from './live-relay-measurement.mjs';

export const MAX_RESPONSE_BYTES = 1024 * 1024;
const SAFE_ERRORS = new Set(['measurement_response_too_large', 'measurement_http_denied',
  'measurement_http_failed', 'measurement_redirect_denied', 'measurement_snapshot_invalid',
  'measurement_source_changed', 'measurement_sample_limit', 'measurement_time_invalid']);

export function validateOptions(options) {
  const { baseUrl, sessionId, durationMs = 60_000, intervalMs = 500, requestTimeoutMs = 3000 } = options;
  let base;
  try { base = new URL(baseUrl); } catch { throw new Error('measurement_options_invalid'); }
  const host = base.hostname.replace(/^\[|\]$/g, '');
  const loopback = host === '::1' || (isIP(host) === 4 && host.startsWith('127.'));
  if ((base.protocol !== 'https:' && !(base.protocol === 'http:' && loopback))
    || base.username || base.password || base.search || base.hash || base.pathname !== '/'
    || typeof sessionId !== 'string' || !/^[A-Za-z0-9_.:-]{1,160}$/.test(sessionId)
    || sessionId === '.' || sessionId === '..'
    || !Number.isInteger(durationMs) || durationMs < 100 || durationMs > 600_000
    || !Number.isInteger(intervalMs) || intervalMs < 100 || intervalMs > 10_000
    || !Number.isInteger(requestTimeoutMs) || requestTimeoutMs < 100 || requestTimeoutMs > 5000) {
    throw new Error('measurement_options_invalid');
  }
  return { baseUrl: base.origin, sessionId, durationMs, intervalMs, requestTimeoutMs };
}

function validateAuthorization(authorization) {
  if (typeof authorization !== 'string' || !/^(Bearer|Device) [\x21-\x7e]{1,4096}$/.test(authorization)) {
    throw new Error('measurement_authorization_invalid');
  }
  if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === '0') throw new Error('measurement_tls_verification_required');
}

async function readSnapshot(url, authorization, signal) {
  const response = await fetch(url, { method: 'GET', headers: { Authorization: authorization, Accept: 'application/json' },
    redirect: 'manual', signal });
  const reader = response.body?.getReader();
  try {
    if (response.status >= 300 && response.status < 400) throw new Error('measurement_redirect_denied');
    if (response.status === 401 || response.status === 403) throw new Error('measurement_http_denied');
    if (!response.ok) throw new Error('measurement_http_failed');
    const declared = response.headers.get('content-length');
    if (declared && (!/^\d+$/.test(declared) || Number(declared) > MAX_RESPONSE_BYTES)) {
      throw new Error('measurement_response_too_large');
    }
    if (!reader) throw new Error('measurement_snapshot_invalid');
    // 固定响应缓冲，避免恶意一字节分块制造无界 chunk 元数据。
    const bytes = Buffer.alloc(MAX_RESPONSE_BYTES);
    let size = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (size + value.byteLength > MAX_RESPONSE_BYTES) throw new Error('measurement_response_too_large');
      bytes.set(value, size);
      size += value.byteLength;
    }
    try { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, size))); }
    catch { throw new Error('measurement_snapshot_invalid'); }
  } finally {
    if (reader) { await reader.cancel().catch(() => {}); reader.releaseLock(); }
  }
}

// 仅访问操作者指定的已授权会话，不发现/配对设备，也不写入或停止任何产品会话。
export async function sampleLiveRelay(input, authorization, externalSignal) {
  const options = validateOptions(input);
  validateAuthorization(authorization);
  const measurement = new Measurement(options.sessionId);
  const deadline = new AbortController();
  const timer = setTimeout(() => deadline.abort(), options.durationMs);
  const started = performance.now();
  const startedAtUtc = new Date().toISOString();
  let status = 'completed';
  let stopReason = 'duration';
  let errorCode = null;
  try {
    for (let count = 0; count < MAX_SAMPLES && performance.now() - started < options.durationMs; count++) {
      const signal = AbortSignal.any([deadline.signal, AbortSignal.timeout(options.requestTimeoutMs),
        ...(externalSignal ? [externalSignal] : [])]);
      const requestStarted = performance.now();
      const raw = await readSnapshot(`${options.baseUrl}/v1/live/sessions/${options.sessionId}`, authorization, signal);
      const sample = measurement.add(raw, Math.round(performance.now() - started));
      if (sample.closed) { stopReason = 'session-closed'; break; }
      await delay(Math.max(1, options.intervalMs - (performance.now() - requestStarted)), undefined,
        { signal: AbortSignal.any([deadline.signal, ...(externalSignal ? [externalSignal] : [])]) });
    }
  } catch (error) {
    if (externalSignal?.aborted) { status = 'cancelled'; stopReason = 'operator-cancelled'; }
    else if (!deadline.signal.aborted) {
      status = 'failed'; stopReason = 'request-failed';
      errorCode = SAFE_ERRORS.has(error.message) ? error.message
        : error.name === 'TimeoutError' ? 'measurement_request_timeout' : 'measurement_request_failed';
    }
  } finally { clearTimeout(timer); }
  const report = measurement.report();
  if (status === 'completed' && report.sampleCount < 2) status = 'insufficient-data';
  return { schemaVersion: 1, status, stopReason, errorCode, startedAtUtc, finishedAtUtc: new Date().toISOString(),
    elapsedMs: Math.round(performance.now() - started),
    settings: { durationMs: options.durationMs, intervalMs: options.intervalMs, requestTimeoutMs: options.requestTimeoutMs,
      maxResponseBytes: MAX_RESPONSE_BYTES, maxSamples: MAX_SAMPLES }, ...report };
}

export function parseArguments(args) {
  const values = {};
  const names = new Set(['--base-url', '--session-id', '--duration-seconds', '--interval-ms', '--timeout-ms', '--output']);
  for (let i = 0; i < args.length; i += 2) {
    if (!names.has(args[i]) || values[args[i]] !== undefined || !args[i + 1]) throw new Error('measurement_options_invalid');
    values[args[i]] = args[i + 1];
  }
  if (!values['--output']) throw new Error('measurement_output_required');
  return { ...validateOptions({ baseUrl: values['--base-url'], sessionId: values['--session-id'],
    durationMs: values['--duration-seconds'] === undefined ? 60_000 : Number(values['--duration-seconds']) * 1000,
    intervalMs: values['--interval-ms'] === undefined ? 500 : Number(values['--interval-ms']),
    requestTimeoutMs: values['--timeout-ms'] === undefined ? 3000 : Number(values['--timeout-ms']) }), output: values['--output'] };
}

async function main() {
  if (process.argv.slice(2).join(' ') === '--help') {
    console.log('只读 LiveRelay 采样（Node.js 22）：--base-url <origin> --session-id <id> --output <new.json>');
    console.log('可选：--duration-seconds 60 --interval-ms 500 --timeout-ms 3000；凭证环境变量 LOOM_MEASURE_AUTHORIZATION。');
    return;
  }
  const options = parseArguments(process.argv.slice(2));
  const authorization = process.env.LOOM_MEASURE_AUTHORIZATION;
  validateAuthorization(authorization);
  // 独占创建：不覆盖既有证据，写入失败也不得宣称采样已交付。
  const file = await open(options.output, 'wx', 0o600);
  const cancel = new AbortController();
  const onSignal = () => cancel.abort();
  process.once('SIGINT', onSignal);
  process.once('SIGTERM', onSignal);
  try {
    const report = await sampleLiveRelay(options, authorization, cancel.signal);
    await file.writeFile(`${JSON.stringify(report, null, 2)}\n`, 'utf8');
    console.log(`measurement_${report.status}; samples=${report.sampleCount}; evidence=sampled-daemon-socket-writes`);
    if (report.status !== 'completed' || report.sampleCount < 2) process.exitCode = 1;
  } finally {
    process.removeListener('SIGINT', onSignal);
    process.removeListener('SIGTERM', onSignal);
    await file.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(() => {
    // 原始网络/文件错误可能包含 URL、凭证或私有响应；只保留固定失败类别。
    console.error('measurement_cli_failed: check options, credential environment, TLS and unused output path');
    process.exitCode = 1;
  });
}
