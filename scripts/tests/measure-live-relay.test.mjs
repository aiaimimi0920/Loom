import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { MAX_RESPONSE_BYTES, parseArguments, sampleLiveRelay, validateOptions } from '../measure-live-relay.mjs';
import { snapshot } from './live-relay-measurement-fixture.mjs';

const authorization = 'Bearer PRIVATE_TEST_TOKEN';
const defaults = { sessionId: 'live:test', durationMs: 400, intervalMs: 100, requestTimeoutMs: 200 };

async function serve(t, handler) {
  const requests = [];
  const server = createServer((req, res) => { requests.push({ method: req.method, path: req.url }); handler(req, res); });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(async () => {
    const closed = new Promise((resolve) => server.close(resolve));
    server.closeAllConnections();
    await closed;
  });
  return { baseUrl: `http://127.0.0.1:${server.address().port}`, requests };
}

test('only exact origins, bounded options, IP loopback HTTP and path-safe session IDs are accepted', () => {
  for (const baseUrl of ['http://192.168.0.1', 'http://localhost', 'file:///tmp/x', 'https://u:p@example.test',
    'https://example.test/path', 'https://example.test/?token=secret', 'https://example.test/#secret']) {
    assert.throws(() => validateOptions({ ...defaults, baseUrl }), /measurement_options_invalid/);
  }
  for (const change of [{ sessionId: '../close' }, { sessionId: '..' }, { sessionId: 'live:x?secret=x' },
    { durationMs: 600001 }, { intervalMs: 99 }, { requestTimeoutMs: 5001 }]) {
    assert.throws(() => validateOptions({ ...defaults, baseUrl: 'http://127.0.0.1', ...change }), /measurement_options_invalid/);
  }
  assert.equal(validateOptions({ ...defaults, baseUrl: 'http://[::1]:3000' }).baseUrl, 'http://[::1]:3000');
  assert.equal(validateOptions({ ...defaults, baseUrl: 'https://loom.example.test' }).durationMs, 400);
  assert.throws(() => parseArguments(['--token', 'secret']), /measurement_options_invalid/);
});

test('real HTTP sampling is authenticated, GET-only, bounded and redacted', async (t) => {
  let id = 0;
  const server = await serve(t, (req, res) => {
    assert.equal(req.headers.authorization, authorization);
    const raw = snapshot({ publishedFrames: ++id, lastFrameId: id, closed: id === 3 });
    Object.assign(raw.mediaDiagnostics, { forwardedFrames: id, receivedBinaryBytes: id * 100, forwardedBinaryBytes: id * 100 });
    raw.mediaDiagnostics.lastForward.frameId = id;
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify(raw));
  });
  const report = await sampleLiveRelay({ ...defaults, durationMs: 3000, requestTimeoutMs: 1000, baseUrl: server.baseUrl }, authorization);
  assert.equal(report.status, 'completed');
  assert.equal(report.sampleCount, 3);
  assert.equal(report.counterDelta.publishedFrames, report.sampleCount - 1);
  assert.equal(report.observedForwards.count, report.sampleCount - 1);
  assert.ok(server.requests.every((r) => r.method === 'GET' && r.path === '/v1/live/sessions/live:test'));
  for (const secret of [authorization, server.baseUrl, 'PRIVATE_TITLE', 'PRIVATE_OCR']) {
    assert.ok(!JSON.stringify(report).includes(secret));
  }
});

for (const [status, code] of [[302, 'measurement_redirect_denied'], [401, 'measurement_http_denied'],
  [403, 'measurement_http_denied'], [500, 'measurement_http_failed']]) {
  test(`HTTP ${status} stops without retries, redirect following or private error bodies`, async (t) => {
    const server = await serve(t, (_req, res) => {
      res.writeHead(status, { Location: '/PRIVATE_LOCATION' });
      res.end('PRIVATE_ERROR');
    });
    const report = await sampleLiveRelay({ ...defaults, baseUrl: server.baseUrl }, authorization);
    assert.equal(report.status, 'failed');
    assert.equal(report.errorCode, code);
    assert.equal(server.requests.length, 1);
    assert.ok(!JSON.stringify(report).includes('PRIVATE_'));
  });
}

for (const declared of [true, false]) {
  test(`oversized ${declared ? 'declared' : 'chunked'} responses fail before JSON is retained`, async (t) => {
    const server = await serve(t, (_req, res) => {
      if (declared) res.setHeader('Content-Length', MAX_RESPONSE_BYTES + 1);
      else res.setHeader('Transfer-Encoding', 'chunked');
      res.end('x'.repeat(MAX_RESPONSE_BYTES + 1));
    });
    const report = await sampleLiveRelay({ ...defaults, baseUrl: server.baseUrl }, authorization);
    assert.equal(report.errorCode, 'measurement_response_too_large');
    assert.equal(report.sampleCount, 0);
  });
}

test('slow response bodies, invalid JSON and operator cancellation terminate the request', async (t) => {
  const server = await serve(t, (_req, res) => { res.writeHead(200); res.write('{'); });
  const timeout = await sampleLiveRelay({ ...defaults, baseUrl: server.baseUrl, requestTimeoutMs: 100 }, authorization);
  assert.equal(timeout.errorCode, 'measurement_request_timeout');
  const cancel = new AbortController();
  const timer = setTimeout(() => cancel.abort(), 30);
  try {
    const stopped = await sampleLiveRelay({ ...defaults, baseUrl: server.baseUrl }, authorization, cancel.signal);
    assert.equal(stopped.status, 'cancelled');
  } finally { clearTimeout(timer); }
  const malformed = await serve(t, (_req, res) => res.end('{PRIVATE_JSON'));
  const invalid = await sampleLiveRelay({ ...defaults, baseUrl: malformed.baseUrl }, authorization);
  assert.equal(invalid.errorCode, 'measurement_snapshot_invalid');
});

test('closed sessions stop sampling without sending a product close request', async (t) => {
  const server = await serve(t, (_req, res) => res.end(JSON.stringify(snapshot({ closed: true }))));
  const report = await sampleLiveRelay({ ...defaults, baseUrl: server.baseUrl }, authorization);
  assert.equal(report.stopReason, 'session-closed');
  assert.equal(report.status, 'insufficient-data');
  assert.equal(server.requests.length, 1);
});

test('the overall deadline bounds an unfinished body and reports insufficient data', async (t) => {
  const server = await serve(t, (_req, res) => { res.writeHead(200); res.write('{'); });
  const report = await sampleLiveRelay({ ...defaults, baseUrl: server.baseUrl, durationMs: 100, requestTimeoutMs: 1000 }, authorization);
  assert.equal(report.status, 'insufficient-data');
  assert.equal(report.stopReason, 'duration');
  assert.equal(report.sampleCount, 0);
  assert.equal(server.requests.length, 1);
});

test('credentials and TLS bypass are rejected before making a request', async () => {
  const options = { ...defaults, baseUrl: 'http://127.0.0.1:1' };
  for (const auth of [undefined, '', 'secret', 'Bearer secret\r\nX: test']) {
    await assert.rejects(sampleLiveRelay(options, auth), /measurement_authorization_invalid/);
  }
  const previous = process.env.NODE_TLS_REJECT_UNAUTHORIZED;
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';
  try { await assert.rejects(sampleLiveRelay(options, authorization), /measurement_tls_verification_required/); }
  finally {
    if (previous === undefined) delete process.env.NODE_TLS_REJECT_UNAUTHORIZED;
    else process.env.NODE_TLS_REJECT_UNAUTHORIZED = previous;
  }
});

test('CLI writes UTF-8 evidence once and never overwrites an existing file', async (t) => {
  const directory = await mkdtemp(join(tmpdir(), 'loom-measure-test-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const output = join(directory, 'report.json');
  let responses = 0;
  const server = await serve(t, (_req, res) => res.end(JSON.stringify(snapshot({ closed: ++responses === 3 }))));
  const run = async () => {
    const child = spawn(process.execPath, [fileURLToPath(new URL('../measure-live-relay.mjs', import.meta.url)),
      '--base-url', server.baseUrl, '--session-id', 'live:test', '--duration-seconds', '3', '--interval-ms', '100',
      '--output', output], { env: { ...process.env, LOOM_MEASURE_AUTHORIZATION: authorization }, windowsHide: true, timeout: 5000 });
    t.after(() => { if (child.exitCode === null && !child.killed) child.kill(); });
    let text = '';
    child.stdout.on('data', (chunk) => { text = (text + chunk).slice(0, 16384); });
    child.stderr.on('data', (chunk) => { text = (text + chunk).slice(0, 16384); });
    const [code] = await once(child, 'close');
    return { code, text };
  };
  const first = await run();
  assert.equal(first.code, 0, first.text);
  assert.ok(!first.text.includes('PRIVATE_'));
  const bytes = await readFile(output);
  assert.notDeepEqual([...bytes.subarray(0, 3)], [0xef, 0xbb, 0xbf]);
  assert.equal(JSON.parse(bytes.toString('utf8')).evidence, 'sampled-daemon-socket-writes');
  await writeFile(output, 'KEEP_EXISTING', 'utf8');
  const before = server.requests.length;
  assert.equal((await run()).code, 1);
  assert.equal(server.requests.length, before);
  assert.equal(await readFile(output, 'utf8'), 'KEEP_EXISTING');
});
