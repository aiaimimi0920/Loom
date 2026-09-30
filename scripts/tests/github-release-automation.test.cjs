const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const publication = require("../../.github/scripts/release-publication.cjs");
const recovery = require("../../.github/scripts/release-recovery.cjs");

test("automatic release trigger accepts only canonical numeric version tags", () => {
  const workflow = fs.readFileSync(path.join(__dirname, "../../.github/workflows/release-tag.yml"), "utf8");
  const pushTags = workflow.match(/^  push:\r?\n    tags:\r?\n((?:      .*\r?\n)+)/m);
  assert.ok(pushTags, "Release workflow must declare its automatic tag filter");
  const patterns = [...pushTags[1].matchAll(/^      - '([^']+)'\r?$/gm)].map((match) => match[1]);
  // GitHub's filter syntax supports [0-9]+ as one or more digits, not shell glob syntax.
  assert.deepEqual(patterns, ["V[0-9]+.[0-9]+.[0-9]+"]);
  const numericTag = new RegExp(`^${patterns[0].replaceAll(".", "\\.")}$`);
  for (const tag of ["V0.2.0", "V1.2.3", "V12.34.567"]) {
    assert.equal(numericTag.test(tag), true, tag);
  }
  for (const tag of ["V0.2.0-public", "V0.2.0-sdk", "V1.2.3-rc.1", "V1.2.3+build", "Vx.y.z", "V1.2", "v1.2.3", "V1/2/3"]) {
    assert.equal(numericTag.test(tag), false, tag);
  }
});

test("publication still rejects noncanonical tags before any GitHub request", async () => {
  let requests = 0;
  const github = {
    paginate: async () => { requests += 1; return []; },
    rest: { repos: { listReleases() {} } },
  };
  for (const tag of ["V0.2.0-public", "V0.2.0-sdk", "V1.2.3-rc.1", "v1.2.3"]) {
    await assert.rejects(publication.assertReleaseAbsent({ github, owner: "owner", repo: "repo", tag }), /Release tag must match/);
  }
  assert.equal(requests, 0);
  await publication.assertReleaseAbsent({ github, owner: "owner", repo: "repo", tag: "V12.34.567" });
  assert.equal(requests, 1);
});

function makePackage(tag) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "loom-release-"));
  for (const directory of ["packages", "sbom", "provenance"]) {
    fs.mkdirSync(path.join(root, directory));
  }
  const packageNames = [
    `Loom-${tag}-windows-x64.zip`,
    `Loom-${tag}-windows-x64.zip.sha256`,
    `Loom-CLI-${tag}-windows-x64.zip`,
    `Loom-CLI-${tag}-windows-x64.zip.sha256`,
    `Loom-Plugin-SDK-${tag}-windows-x64.zip`,
    `Loom-Plugin-SDK-${tag}-windows-x64.zip.sha256`,
  ];
  for (const name of packageNames) fs.writeFileSync(path.join(root, "packages", name), name);
  fs.writeFileSync(path.join(root, "sbom", `Loom-${tag}.cdx.json`), "{}");
  fs.writeFileSync(path.join(root, "provenance", "build-provenance.json"), "{}");
  fs.writeFileSync(path.join(root, "manifest.json"), "{}");
  fs.writeFileSync(path.join(root, "checksums.sha256"), "checksums");
  return root;
}

test("release asset contract rejects missing, extra, size, and digest drift", async () => {
  const root = makePackage("V1.2.3");
  const expected = publication.collectExpectedAssets(root, "V1.2.3");
  assert.deepEqual(expected.map((item) => item.name), [
    "Loom-V1.2.3-windows-x64.zip",
    "Loom-V1.2.3-windows-x64.zip.sha256",
    "Loom-Plugin-SDK-V1.2.3-windows-x64.zip",
    "Loom-Plugin-SDK-V1.2.3-windows-x64.zip.sha256",
  ]);
  const actual = expected.map((item, id) => ({
    id,
    name: item.name,
    size: item.bytes,
    digest: `sha256:${crypto.createHash("sha256").update(fs.readFileSync(item.path)).digest("hex")}`,
  }));
  await assert.doesNotReject(publication.compareAssets(expected, actual));
  await assert.rejects(publication.compareAssets(expected, actual.slice(1)), /missing=/);
  await assert.rejects(publication.compareAssets(expected, [...actual, { name: "extra.bin", size: 1 }]), /unexpected=/);
  await assert.rejects(publication.compareAssets(expected, actual.map((item, index) => index ? item : { ...item, size: item.size + 1 })), /size mismatch/);
  await assert.rejects(publication.compareAssets(expected, actual.map((item, index) => index ? item : { ...item, digest: "sha256:bad" })), /digest mismatch/);
  fs.rmSync(root, { recursive: true, force: true });
});

test("publication refuses an existing draft or published release", async () => {
  for (const draft of [true, false]) {
    const github = {
      paginate: async () => [{ tag_name: "V1.2.3", draft, html_url: "https://example.invalid/release" }],
      rest: { repos: { listReleases() {} } },
    };
    await assert.rejects(
      publication.assertReleaseAbsent({ github, owner: "owner", repo: "repo", tag: "V1.2.3" }),
      new RegExp(draft ? "draft" : "published"),
    );
  }
});

test("verified draft publishes only after exact asset comparison", async () => {
  const root = makePackage("V1.2.3");
  const expected = publication.collectExpectedAssets(root, "V1.2.3");
  let published = false;
  const github = {
    paginate: async () => expected.map((item, id) => ({ id, name: item.name, size: item.bytes })),
    rest: { repos: {
      getRelease: async () => ({ data: { draft: true, tag_name: "V1.2.3" } }),
      listReleaseAssets() {},
      updateRelease: async ({ draft }) => { published = draft === false; },
    } },
  };
  await publication.publishVerifiedDraft({ github, owner: "owner", repo: "repo", releaseId: "42", tag: "V1.2.3", packageDirectory: root });
  assert.equal(published, true);
  fs.rmSync(root, { recursive: true, force: true });
});

test("release recovery retries only the first transient-boundary failure", () => {
  const jobs = [{ name: "release", html_url: "https://example.invalid/job", steps: [{ name: "Setup Rust", conclusion: "failure" }] }];
  assert.equal(recovery.retryDecision({ conclusion: "failure", run_attempt: 1 }, jobs).retry, true);
  assert.equal(recovery.retryDecision({ conclusion: "failure", run_attempt: 2 }, jobs).retry, false);
  const buildJobs = [{ name: "release", steps: [{ name: "Build standalone Loom release", conclusion: "failure" }] }];
  assert.equal(recovery.retryDecision({ conclusion: "failure", run_attempt: 1 }, buildJobs).retry, false);
  assert.equal(recovery.retryDecision({ conclusion: "timed_out", run_attempt: 1 }, jobs).retry, false);
});
