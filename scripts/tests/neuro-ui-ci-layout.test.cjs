const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const root = path.resolve(__dirname, '../..');
const loom = 'Project/Loom';
const design = 'nas_home/NeuroDesign';
const revision = '02420e8d129cf952c6bee3cda96ab7ebc95c2d67';
const workflows = [
  ['ci.yml', 'windows'],
  ['build-windows.yml', 'build'],
  ['release-tag.yml', 'release'],
  ['compile-windows.yml', 'compile'],
];
const read = (relative) => fs.readFileSync(path.join(root, relative), 'utf8');

// Restrict extraction to the named job/step, so another job cannot satisfy a check.
function jobBody(source, name) {
  const match = source.match(new RegExp(`^  ${name}:\\r?\\n([\\s\\S]*?)(?=^  [\\w-]+:|(?![\\s\\S]))`, 'm'));
  assert.ok(match, `missing job: ${name}`);
  return match[1];
}

function steps(job) {
  return [...job.matchAll(/^      - name: ([^\r\n]+)\r?\n([\s\S]*?)(?=^      - name:|(?![\s\S]))/gm)]
    .map((match) => ({ name: match[1], body: match[2] }));
}

function scalar(body, key, indent) {
  const match = body.match(new RegExp(`^${' '.repeat(indent)}${key}: ([^\\r\\n]+)\\r?$`, 'm'));
  assert.ok(match, `missing ${key}`);
  return match[1].replace(/^"(.*)"$/, '$1');
}

function blockList(body, key) {
  const match = body.match(new RegExp(`^          ${key}: \\|\\r?\\n((?:            [^\\r\\n]+\\r?\\n)+)`, 'm'));
  assert.ok(match, `missing ${key} list`);
  return match[1].trim().split(/\r?\n/).map((line) => line.trim());
}

function validateLayout(source, jobName, manifest, lock) {
  const job = jobBody(source, jobName);
  assert.match(job, new RegExp(`    defaults:\\r?\\n      run:\\r?\\n        working-directory: ${loom}\\r?\\n`));
  const ordered = steps(job);
  const step = (name) => {
    const found = ordered.find((entry) => entry.name === name);
    assert.ok(found, `missing step: ${name}`);
    return found.body;
  };
  const before = (left, right) => assert.ok(
    ordered.findIndex((entry) => entry.name === left) < ordered.findIndex((entry) => entry.name === right),
    `${left} must precede ${right}`,
  );
  const checkout = step(jobName === 'release' ? 'Checkout release ref' : 'Checkout');
  assert.equal(scalar(checkout, 'path', 10), loom);
  assert.equal(scalar(checkout, 'persist-credentials', 10), 'false');
  const dependency = step('Checkout Neuro UI source');
  assert.equal(scalar(dependency, 'repository', 10), 'aiaimimi0920/NeuroDesign');
  assert.equal(scalar(dependency, 'ref', 10), revision);
  assert.equal(scalar(dependency, 'path', 10), design);
  assert.equal(scalar(dependency, 'persist-credentials', 10), 'false');
  assert.equal(scalar(step('Setup Node.js'), 'node-version', 10), '22.22.2');
  assert.equal(scalar(step('Setup npm'), 'run', 8), 'npm install --global npm@11.21.1 --no-audit --no-fund');
  for (const [name, command] of [
    ['Install Neuro UI dependencies', 'npm ci --no-audit --no-fund'],
    ['Build Neuro UI library', 'node scripts/build.mjs --library --theme neuro'],
  ]) {
    assert.equal(scalar(step(name), 'working-directory', 8), design);
    assert.equal(scalar(step(name), 'run', 8), command);
  }
  before('Checkout Neuro UI source', 'Install Neuro UI dependencies');
  before('Setup Node.js', 'Setup npm');
  before('Setup npm', 'Install Neuro UI dependencies');
  before('Install Neuro UI dependencies', 'Build Neuro UI library');
  before('Build Neuro UI library', ['windows', 'compile'].includes(jobName) ? 'Install desktop dependencies' : 'Build standalone Loom release');
  const localPackage = manifest.dependencies['@neuro/ui'];
  assert.ok(localPackage.startsWith('file:'));
  assert.equal(lock.packages[''].dependencies['@neuro/ui'], localPackage);
  assert.equal(lock.packages['node_modules/@neuro/ui'].resolved, localPackage);
  assert.equal(
    path.posix.normalize(path.posix.join(loom, 'apps/desktop', localPackage.slice(5))),
    `${design}/packages/react/neuro-ui`,
    'checkout layout must resolve the committed local package',
  );
  assert.deepEqual(blockList(step('Cache Rust build'), 'workspaces'), [
    `${loom} -> target`,
    `${loom}/apps/desktop/src-tauri -> target`,
    ...(jobName === 'windows' ? [`${loom}/framework-packages/runtime-host -> target`] : []),
  ]);
  if (jobName === 'windows') {
    for (const name of ['Install desktop dependencies', 'Typecheck desktop frontend', 'Test desktop frontend services', 'Build desktop frontend']) {
      assert.equal(scalar(step(name), 'working-directory', 8), `${loom}/apps/desktop`);
    }
    assert.equal(scalar(step('Upload effective-code-line report'), 'path', 10), `${loom}/artifacts/effective-code-lines.json`);
    assert.equal(scalar(step('Retain windows quality evidence'), 'path', 10), `${loom}/artifacts/quality`);
  } else if (jobName === 'build') {
    assert.deepEqual(blockList(step('Upload bounded smoke phase diagnostics'), 'path'), [
      `${loom}/target/runtime-smoke/latest/mcp-smoke-diagnostic.json`,
      `${loom}/target/runtime-smoke/plugin-boundary/plugin-boundary-diagnostic.json`,
    ]);
    assert.equal(scalar(step('Upload Windows candidate'), 'path', 10), `${loom}/release/Loom/\${{ env.LOOM_VERSION }}`);
  } else if (jobName === 'compile') {
    for (const name of ['Install desktop dependencies', 'Compile desktop executable without bundling']) {
      assert.equal(scalar(step(name), 'working-directory', 8), `${loom}/apps/desktop`);
    }
    assert.equal(scalar(step('Retain compilation evidence only'), 'path', 10), `${loom}/artifacts/compilation.json`);
  } else {
    const packageRoot = `${loom}/release/Loom/\${{ env.LOOM_TAG }}`;
    assert.equal(scalar(step('Attest release build provenance'), 'subject-path', 10), `${packageRoot}/packages/*.zip`);
    assert.equal(scalar(step('Attest release SBOM'), 'subject-path', 10), `${packageRoot}/packages/Loom-\${{ env.LOOM_TAG }}-windows-x64.zip`);
    assert.equal(scalar(step('Attest release SBOM'), 'sbom-path', 10), `${packageRoot}/sbom/Loom-\${{ env.LOOM_TAG }}.cdx.json`);
    assert.equal(scalar(step('Create verified draft release'), 'body_path', 10), `${loom}/docs/GITHUB_RELEASE_BODY.md`);
    assert.deepEqual(blockList(step('Create verified draft release'), 'files'), [
      `${packageRoot}/packages/Loom-\${{ env.LOOM_TAG }}-windows-x64.zip`,
      `${packageRoot}/packages/Loom-\${{ env.LOOM_TAG }}-windows-x64.zip.sha256`,
      `${packageRoot}/packages/Loom-Plugin-SDK-\${{ env.LOOM_TAG }}-windows-x64.zip`,
      `${packageRoot}/packages/Loom-Plugin-SDK-\${{ env.LOOM_TAG }}-windows-x64.zip.sha256`,
    ]);
    assert.equal(scalar(step('Verify assets and publish draft release'), 'LOOM_PACKAGE_DIRECTORY', 10), packageRoot);
    for (const name of ['Check release publication state', 'Verify assets and publish draft release', 'Remove failed draft release']) {
      assert.ok(step(name).includes(`require(\`\${process.env.GITHUB_WORKSPACE}/${loom}/.github/scripts/release-publication.cjs\`)`));
    }
  }
}

const manifest = JSON.parse(read('apps/desktop/package.json'));
const lock = JSON.parse(read('apps/desktop/package-lock.json'));
for (const [file, job] of workflows) {
  test(`${file} resolves and builds the pinned local UI before consuming it`, () => {
    validateLayout(read(`.github/workflows/${file}`), job, manifest, lock);
  });
}

test('the layout guard rejects checkout, pin, order, and action-path regressions', () => {
  const source = read('.github/workflows/ci.yml');
  const reversedBuild = source.replace(
    /(^      - name: Install Neuro UI dependencies\r?\n[\s\S]*?)(^      - name: Build Neuro UI library\r?\n[\s\S]*?)(?=^      - name:)/m,
    '$2$1',
  );
  const mutations = [
    source.replace(`path: ${loom}`, 'path: Loom'),
    source.replace(`ref: ${revision}`, 'ref: main'),
    reversedBuild,
    source.replace(`${loom}/artifacts/effective-code-lines.json`, 'artifacts/effective-code-lines.json'),
  ];
  for (const changed of mutations) {
    assert.notEqual(changed, source, 'negative control must change the workflow');
    assert.throws(() => validateLayout(changed, 'windows', manifest, lock));
  }
});

test('non-UI jobs retain their independent root checkout', () => {
  const ci = read('.github/workflows/ci.yml');
  for (const name of ['linux', 'plugin-security', 'plugin-sdk-clean-host']) {
    const job = jobBody(ci, name);
    assert.doesNotMatch(job, /NeuroDesign|working-directory: Project\/Loom/);
    const checkout = steps(job).find((entry) => /uses: actions\/checkout@/.test(entry.body));
    assert.ok(checkout, `missing root checkout: ${name}`);
    assert.doesNotMatch(checkout.body, /^          path:/m);
  }
  const release = jobBody(read('.github/workflows/release-tag.yml'), 'dependency-security');
  assert.match(release, /uses: \.\/\.github\/workflows\/dependency-security\.yml/);
  assert.doesNotMatch(release, /NeuroDesign|Project\/Loom/);
  assert.equal(read('apps/desktop/.npmrc').trim(), 'install-links=true');
});
