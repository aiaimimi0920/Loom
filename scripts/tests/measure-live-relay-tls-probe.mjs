import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { createContext, SourceTextModule, SyntheticModule } from 'node:vm';

const url = new URL('../measure-live-relay.mjs', import.meta.url);
const source = await readFile(url, 'utf8');
const authorization = 'Bearer PRIVATE_TEST_TOKEN';
const previous = process.env.NODE_TLS_REJECT_UNAUTHORIZED;
for (const cli of [false, true]) {
  let requests = 0;
  let outputs = 0;
  const messages = [];
  // This environment belongs only to the VM; real Node TLS settings never change.
  const simulatedProcess = {
    env: { NODE_TLS_REJECT_UNAUTHORIZED: '0', LOOM_MEASURE_AUTHORIZATION: authorization },
    argv: cli ? ['node', fileURLToPath(url), '--base-url', 'https://example.test',
      '--session-id', 'live:test', '--output', 'never-created.json'] : [],
  };
  const context = createContext({ process: simulatedProcess,
    console: { error: (message) => messages.push(message) },
    fetch: () => { requests++; throw new Error('unexpected_request'); },
    URL, AbortController, AbortSignal, performance, setTimeout, clearTimeout,
  });
  const module = new SourceTextModule(source, { context, identifier: url.href,
    initializeImportMeta(meta) { meta.url = url.href; } });
  await module.link(async (specifier) => {
    const exports = specifier === 'node:fs/promises'
      ? { open: () => { outputs++; throw new Error('unexpected_output'); } }
      : await import(new URL(specifier, url).href);
    return new SyntheticModule(Object.keys(exports), function () {
      for (const [name, value] of Object.entries(exports)) this.setExport(name, value);
    }, { context });
  });
  await module.evaluate();
  if (cli) {
    assert.equal(simulatedProcess.exitCode, 1);
    assert.deepEqual(messages, ['measurement_cli_failed: check options, credential environment, TLS and unused output path']);
  } else {
    await assert.rejects(module.namespace.sampleLiveRelay({ baseUrl: 'https://example.test', sessionId: 'live:test' }, authorization),
      /measurement_tls_verification_required/);
  }
  assert.equal(requests, 0);
  assert.equal(outputs, 0);
}
assert.equal(process.env.NODE_TLS_REJECT_UNAUTHORIZED, previous);
