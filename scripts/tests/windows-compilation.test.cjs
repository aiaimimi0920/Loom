const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const root = path.resolve(__dirname, '../..');
const read = (name) => fs.readFileSync(path.join(root, name), 'utf8');
const source = read('.github/workflows/compile-windows.yml');

function validateCompilation(workflow) {
  assert.match(workflow, /run: cargo build --locked --release -p loom-daemon -p loom-cli -p loom-plugin-cli/);
  assert.match(workflow, /run: npm run tauri build -- --no-bundle -- --locked/);
  assert.match(workflow, /git diff --exit-code/);
  assert.match(workflow, /\$head -ne \$env:GITHUB_SHA/);
  assert.match(workflow, /compilationOnly = \$true/);
  assert.match(workflow, /packageVerified = \$false/);
  assert.match(workflow, /formalReleaseApproved = \$false/);
  assert.doesNotMatch(workflow, /continue-on-error|build-release\.ps1|action-gh-release|attest-|contents: write/);
  const uploads = [...workflow.matchAll(/^          path: (.+)$/gm)]
    .map((match) => match[1]).filter((value) => !['NeuroDesign', 'AI/GameEditor/Neuro/Loom'].includes(value));
  assert.deepEqual(uploads, ['AI/GameEditor/Neuro/Loom/artifacts/compilation.json']);
  for (const executable of ['loom-daemon.exe', 'loom.exe', 'loom-plugin.exe', 'loom-desktop.exe']) {
    assert.ok(workflow.includes(`/release/${executable}"`), `missing output: ${executable}`);
  }
}

test('compile-only workflow links real executables without publishing packages', () => {
  validateCompilation(source);
});

test('compilation cannot be relabeled as package approval or publish executables', () => {
  for (const mutated of [
    source.replace('formalReleaseApproved = $false', 'formalReleaseApproved = $true'),
    source.replace('--no-bundle -- --locked', '--no-bundle'),
    source.replace('path: AI/GameEditor/Neuro/Loom/artifacts/compilation.json', 'path: AI/GameEditor/Neuro/Loom/target/release/*.exe'),
  ]) {
    assert.notEqual(mutated, source);
    assert.throws(() => validateCompilation(mutated));
  }
});

test('existing package, security, and release approval gates remain mandatory', () => {
  assert.match(read('.github/workflows/build-windows.yml'), /build-release\.ps1/);
  assert.match(read('.github/workflows/ci.yml'), /Test-DependencySecurityContract\.ps1/);
  assert.match(read('.github/workflows/release-tag.yml'), /needs: dependency-security/);
  assert.match(read('scripts/build-release.ps1'), /if \(-not \$DevelopmentOnly -and -not \$aria2Policy\.releaseApproved\)/);
});
