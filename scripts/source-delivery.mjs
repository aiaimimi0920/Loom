// 导出明确白名单中的工作区原字节；不把 HEAD 或旧二进制冒充快照构建身份。
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const roots = new Set(['apps', 'crates', 'native', 'scripts', 'protocol', 'docs',
  'security', 'framework-packages', 'capability-packages', 'mcp-server-packages']);
const top = new Set(['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'LICENSE',
  'LICENSE-MIT', 'LICENSE-APACHE', 'README.md', 'CONTRIBUTING.md', 'SOURCE_PROVENANCE.md']);
const blocked = /^(?:\.env(?:\..*)?|\.git|node_modules|target|dist|\.tmp|\.runtime|\.rsbuild|__pycache__|credentials?\.(?:json|toml|yaml|yml|txt)|auth\.json|.*\.(?:key|pfx|p12|pem|log|exe|dll|zip))$/i;
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');

function relativeName(name) {
  if (typeof name !== 'string' || name.includes('\\') || name.includes(':') ||
      name.includes('\0') || name.startsWith('/') || name.split('/').some(p => !p || p === '.' || p === '..')) {
    throw new Error('SOURCE_PATH_INVALID');
  }
  return name;
}

export function selected(name) {
  relativeName(name);
  const parts = name.split('/');
  return (parts.length === 1 ? top.has(name) : roots.has(parts[0])) && !parts.some(p => blocked.test(p));
}

function regularFile(root, name) {
  let current = root;
  for (const part of relativeName(name).split('/')) {
    current = path.join(current, part);
    if (fs.lstatSync(current).isSymbolicLink()) throw new Error('SOURCE_LINK_REJECTED');
  }
  if (!fs.lstatSync(current).isFile()) throw new Error('SOURCE_NOT_REGULAR');
  return current;
}

function safeRoot(root) {
  const absolute = path.resolve(root);
  let current = absolute;
  while (true) {
    if (fs.existsSync(current) && fs.lstatSync(current).isSymbolicLink()) throw new Error('SOURCE_LINK_REJECTED');
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  return absolute;
}

export function verifySnapshot(root) {
  root = safeRoot(root);
  const manifest = JSON.parse(fs.readFileSync(regularFile(root, 'source-manifest.json'), 'utf8'));
  if (manifest.schemaVersion !== 1 || !Array.isArray(manifest.files) || !manifest.files.length) {
    throw new Error('SOURCE_MANIFEST_INVALID');
  }
  const seen = new Set();
  for (const record of manifest.files) {
    if (!selected(record.path) || seen.has(record.path.toLowerCase())) throw new Error('SOURCE_MANIFEST_PATH');
    seen.add(record.path.toLowerCase());
    const bytes = fs.readFileSync(regularFile(root, record.path));
    if (bytes.length !== record.bytes || hash(bytes) !== record.sha256) throw new Error('SOURCE_DIGEST_MISMATCH');
  }
  // 不允许在验证后默默加入未列入清单的源码或配置。
  function walk(dir, prefix = '') {
    for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
      const name = prefix + item.name;
      if (item.isSymbolicLink()) throw new Error('SOURCE_LINK_REJECTED');
      if (item.isDirectory()) walk(path.join(dir, item.name), name + '/');
      else if (name !== 'source-manifest.json' && !seen.has(name.toLowerCase())) throw new Error('SOURCE_UNLISTED_FILE');
    }
  }
  walk(root);
  return manifest;
}

export function createSnapshot(repo, output, candidates, head = null) {
  repo = safeRoot(repo);
  output = safeRoot(output);
  const relation = path.relative(repo, output);
  if (!relation || (!relation.startsWith('..' + path.sep) && relation !== '..' && !path.isAbsolute(relation))) {
    throw new Error('SOURCE_OUTPUT_INSIDE_REPO');
  }
  if (fs.existsSync(output)) throw new Error('SOURCE_OUTPUT_EXISTS');
  const names = [...new Set(candidates.filter(selected))].sort();
  const folded = names.map(n => n.toLowerCase());
  if (new Set(folded).size !== names.length) throw new Error('SOURCE_PATH_COLLISION');
  for (const required of ['Cargo.toml', 'Cargo.lock', 'apps/desktop/package-lock.json',
    'apps/desktop/src-tauri/Cargo.lock', 'framework-packages/runtime-host/Cargo.lock']) {
    if (!names.includes(required)) throw new Error('SOURCE_REQUIRED_FILE_MISSING: ' + required);
  }
  for (const name of names) regularFile(repo, name);
  fs.mkdirSync(output, { recursive: true });
  const files = names.map(name => {
    const bytes = fs.readFileSync(regularFile(repo, name));
    const destination = path.join(output, name);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, bytes, { flag: 'wx' });
    return { path: name, bytes: bytes.length, sha256: hash(bytes) };
  });
  for (const record of files) {
    if (hash(fs.readFileSync(regularFile(repo, record.path))) !== record.sha256) throw new Error('SOURCE_CHANGED_DURING_EXPORT');
  }
  const manifest = { schemaVersion: 1, scope: 'loom-first-party-worktree-snapshot',
    gitHead: head, capturesUncommittedFiles: true, binaryBinding: null,
    combinedApplicationSourceComplete: false, releaseApproved: false, files };
  fs.writeFileSync(path.join(output, 'source-manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
  verifySnapshot(output);
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [mode, root, output] = process.argv.slice(2);
  if (mode === 'verify' && root && !output) {
    console.log(`Source snapshot verified: ${verifySnapshot(root).files.length} files`);
  } else if (mode === 'create' && root && output) {
    const git = args => execFileSync('rtk', ['proxy', 'git', '-C', path.resolve(root), ...args], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    const names = git(['ls-files', '-z', '--cached', '--others', '--exclude-standard']).split('\0').filter(Boolean);
    const result = createSnapshot(root, output, names, git(['rev-parse', 'HEAD']).trim());
    console.log(`Source snapshot created: ${result.files.length} files; not a complete corresponding-source release`);
  } else {
    throw new Error('Usage: source-delivery.mjs create REPO NEW_OUTPUT | verify SNAPSHOT');
  }
}
