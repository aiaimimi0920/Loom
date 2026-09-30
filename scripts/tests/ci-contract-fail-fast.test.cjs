const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

// Windows PowerShell 5.1 does not turn a child process exit code into a
// terminating error, even with ErrorActionPreference=Stop. A later successful
// command can otherwise hide a failed security or release contract.
function unguardedNativeCommands(workflow) {
  const lines = workflow.split(/\r?\n/);
  const missing = [];
  for (let index = 0; index < lines.length; index += 1) {
    const command = lines[index].trim();
    if (!command.startsWith('powershell -NoProfile ')
      && !command.startsWith('.\\target\\debug\\loom-plugin.exe keygen ')) {
      continue;
    }
    const start = index + 1;
    while (lines[index].trimEnd().endsWith('`')) index += 1;
    if (lines[index + 1]?.trim()
      !== 'if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }') {
      missing.push(start);
    }
  }
  return missing;
}

test('rejects an early failing contract hidden by a later successful command', () => {
  const workflow = [
    '          powershell -NoProfile -File failing-contract.ps1',
    '          powershell -NoProfile -File successful-contract.ps1',
    '          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }',
  ].join('\n');
  assert.deepEqual(unguardedNativeCommands(workflow), [1]);
});

test('requires guards after complete multiline commands and native key generation', () => {
  const workflow = [
    '          powershell -NoProfile -File contract.ps1 `',
    '            -ArtifactRoot $root | Out-Null',
    '          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }',
    '          .\\target\\debug\\loom-plugin.exe keygen $key publisher',
    '          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }',
  ].join('\n');
  assert.deepEqual(unguardedNativeCommands(workflow), []);
  assert.deepEqual(unguardedNativeCommands(workflow.replace(
    '          if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n', '',
  )), [1]);
});

test('every external PowerShell CI contract preserves its exit status immediately', () => {
  const workflow = fs.readFileSync(
    path.resolve(__dirname, '../../.github/workflows/ci.yml'), 'utf8',
  );
  assert.deepEqual(unguardedNativeCommands(workflow), []);
  assert.ok(workflow.includes('node --test .\\scripts\\tests\\ci-contract-fail-fast.test.cjs'));
});
