const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

function assertTag(tag) {
  if (!/^V\d+\.\d+\.\d+$/.test(tag)) {
    throw new Error(`Release tag must match Vx.y.z: ${tag}`);
  }
}

function fileRecord(filePath) {
  const bytes = fs.statSync(filePath).size;
  return { name: path.basename(filePath), path: filePath, bytes };
}

function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = crypto.createHash("sha256");
    const stream = fs.createReadStream(filePath);
    stream.on("error", reject);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("end", () => resolve(hash.digest("hex")));
  });
}

function jsonFiles(directory) {
  if (!fs.existsSync(directory)) {
    throw new Error(`Release metadata directory is missing: ${directory}`);
  }
  const files = fs.readdirSync(directory)
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map((name) => path.join(directory, name));
  if (files.length === 0) {
    throw new Error(`Release metadata directory contains no JSON files: ${directory}`);
  }
  return files;
}

function collectExpectedAssets(packageDirectory, tag) {
  assertTag(tag);
  const packageRoot = path.resolve(packageDirectory);
  const packageNames = [
    `Loom-${tag}-windows-x64.zip`,
    `Loom-${tag}-windows-x64.zip.sha256`,
    `Loom-CLI-${tag}-windows-x64.zip`,
    `Loom-CLI-${tag}-windows-x64.zip.sha256`,
    `Loom-Plugin-SDK-${tag}-windows-x64.zip`,
    `Loom-Plugin-SDK-${tag}-windows-x64.zip.sha256`,
  ];
  const files = [
    ...packageNames.map((name) => path.join(packageRoot, "packages", name)),
    ...jsonFiles(path.join(packageRoot, "sbom")),
    ...jsonFiles(path.join(packageRoot, "provenance")),
    path.join(packageRoot, "manifest.json"),
    path.join(packageRoot, "checksums.sha256"),
  ];
  const records = files.map((filePath) => {
    if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
      throw new Error(`Expected release asset is missing: ${filePath}`);
    }
    return fileRecord(filePath);
  });
  if (new Set(records.map((record) => record.name)).size !== records.length) {
    throw new Error("Release asset names must be unique across published directories.");
  }
  return records;
}

async function compareAssets(expected, actual) {
  const actualByName = new Map();
  for (const asset of actual) {
    if (actualByName.has(asset.name)) {
      throw new Error(`GitHub draft contains duplicate asset name: ${asset.name}`);
    }
    actualByName.set(asset.name, asset);
  }
  const expectedNames = new Set(expected.map((asset) => asset.name));
  const unexpected = actual.filter((asset) => !expectedNames.has(asset.name)).map((asset) => asset.name);
  const missing = expected.filter((asset) => !actualByName.has(asset.name)).map((asset) => asset.name);
  if (missing.length || unexpected.length) {
    throw new Error(`GitHub draft asset set mismatch. missing=${missing.join(",") || "none"} unexpected=${unexpected.join(",") || "none"}`);
  }
  for (const record of expected) {
    const asset = actualByName.get(record.name);
    if (Number(asset.size) !== record.bytes) {
      throw new Error(`GitHub draft asset size mismatch: ${record.name}`);
    }
    if (asset.digest) {
      const sha256 = await sha256File(record.path);
      if (asset.digest !== `sha256:${sha256}`) {
        throw new Error(`GitHub draft asset digest mismatch: ${record.name}`);
      }
    }
  }
}

async function assertReleaseAbsent({ github, owner, repo, tag }) {
  assertTag(tag);
  const releases = await github.paginate(github.rest.repos.listReleases, { owner, repo, per_page: 100 });
  const existing = releases.find((release) => release.tag_name === tag);
  if (existing) {
    const state = existing.draft ? "draft" : "published";
    throw new Error(`Release ${tag} already exists in ${state} state: ${existing.html_url}`);
  }
}

async function publishVerifiedDraft({ github, owner, repo, releaseId, tag, packageDirectory }) {
  assertTag(tag);
  const id = Number(releaseId);
  if (!Number.isSafeInteger(id) || id <= 0) {
    throw new Error(`Draft release ID is invalid: ${releaseId}`);
  }
  const { data: release } = await github.rest.repos.getRelease({ owner, repo, release_id: id });
  if (!release.draft || release.tag_name !== tag) {
    throw new Error(`Release ${id} is not the expected unpublished draft for ${tag}.`);
  }
  const actual = await github.paginate(github.rest.repos.listReleaseAssets, {
    owner,
    repo,
    release_id: id,
    per_page: 100,
  });
  await compareAssets(collectExpectedAssets(packageDirectory, tag), actual);
  return github.rest.repos.updateRelease({
    owner,
    repo,
    release_id: id,
    draft: false,
    make_latest: "legacy",
  });
}

async function deleteFailedDraft({ github, owner, repo, releaseId, tag }) {
  const id = Number(releaseId);
  if (!Number.isSafeInteger(id) || id <= 0) return false;
  const { data: release } = await github.rest.repos.getRelease({ owner, repo, release_id: id });
  if (!release.draft || release.tag_name !== tag) return false;
  await github.rest.repos.deleteRelease({ owner, repo, release_id: id });
  return true;
}

module.exports = {
  assertReleaseAbsent,
  collectExpectedAssets,
  compareAssets,
  deleteFailedDraft,
  publishVerifiedDraft,
};
