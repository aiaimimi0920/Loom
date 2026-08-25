const fs = require("node:fs");
const path = require("node:path");

const LABELS = {
  "dependencies:scope-supply-chain": ["B60205", "Build, CI, release, or scanner supply-chain boundary"],
  "dependencies:scope-runtime": ["D93F0B", "Dependency can affect shipped runtime behavior"],
  "dependencies:scope-tooling": ["0E8A16", "Development-only dependency tooling"],
  "dependencies:scope-unknown": ["6A737D", "Dependency scope needs manual classification"],
  "dependencies:update-major": ["B60205", "Major dependency update"],
  "dependencies:update-minor": ["FBCA04", "Minor dependency update"],
  "dependencies:update-patch": ["0E8A16", "Patch or prerelease-only dependency update"],
  "dependencies:update-grouped": ["5319E7", "Grouped dependency update"],
  "dependencies:update-unknown": ["6A737D", "Dependency update type needs manual classification"],
  "dependencies:needs-human": ["B60205", "Automatic merge is forbidden; human review required"],
  "dependencies:review-candidate": ["0E8A16", "Low-risk review candidate; automatic merge remains disabled"],
};

const SENSITIVE_DEPENDENCIES = new Set(["@tauri-apps/cli", "ort", "paddle-ocr-rs", "tauri", "zerocopy"]);

function dependencyName(title) {
  const match = title.match(/bump\s+(?:the\s+)?(.+?)\s+from\s+/i);
  return match ? match[1].trim() : "";
}

function updateType(title) {
  const match = title.match(/\bfrom\s+v?(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:-([^\s]+))?\s+to\s+v?(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:-([^\s]+))?/i);
  if (!match) return /\bgroup\b|\b\d+ updates?\b/i.test(title) ? "grouped" : "unknown";
  const from = [1, 2, 3].map((offset) => Number(match[offset] || 0));
  const to = [5, 6, 7].map((offset) => Number(match[offset] || 0));
  if (from[0] !== to[0]) return "major";
  if (from[1] !== to[1]) return "minor";
  if (from[2] !== to[2] || match[4] !== match[8]) return "patch";
  return "unknown";
}

function readDesktopDependencies(workspace) {
  const packagePath = path.join(workspace, "apps", "desktop", "package.json");
  const manifest = JSON.parse(fs.readFileSync(packagePath, "utf8"));
  return {
    development: new Set(Object.keys(manifest.devDependencies || {})),
    runtime: new Set(Object.keys(manifest.dependencies || {})),
  };
}

function scopeFor(title, files, desktopDependencies) {
  const names = files.map((file) => file.filename.replaceAll("\\", "/"));
  const supplyChain = names.some((name) => name === "Dockerfile"
    || name === ".dockerignore"
    || name === ".github/dependabot.yml"
    || name.startsWith(".github/workflows/")
    || name.startsWith(".github/scripts/")
    || name.startsWith("security/")
    || /^scripts\/(build-release|verify-release)/.test(name));
  if (supplyChain) return "supply-chain";
  if (names.some((name) => /(^|\/)Cargo\.(toml|lock)$/.test(name))) return "runtime";
  if (names.some((name) => /(^|\/)package(-lock)?\.json$/.test(name))) {
    const name = dependencyName(title);
    if (desktopDependencies.development.has(name)) return "tooling";
    if (desktopDependencies.runtime.has(name)) return "runtime";
  }
  return "unknown";
}

function classify(title, files, desktopDependencies) {
  const scope = scopeFor(title, files, desktopDependencies);
  const update = updateType(title);
  const name = dependencyName(title);
  const metadataOnly = files.every((file) => [
    "apps/desktop/package.json",
    "apps/desktop/package-lock.json",
  ].includes(file.filename.replaceAll("\\", "/")));
  const candidate = scope === "tooling"
    && ["minor", "patch"].includes(update)
    && metadataOnly
    && !SENSITIVE_DEPENDENCIES.has(name);
  return {
    dependency: name || "grouped-or-unknown",
    disposition: candidate ? "review-candidate" : "needs-human",
    scope,
    update,
  };
}

function managedLabels(classification) {
  return [
    `dependencies:scope-${classification.scope}`,
    `dependencies:update-${classification.update}`,
    `dependencies:${classification.disposition}`,
  ];
}

async function ensureLabels(github, owner, repo) {
  for (const [name, [color, description]] of Object.entries(LABELS)) {
    try {
      await github.rest.issues.getLabel({ owner, repo, name });
    } catch (error) {
      if (error.status !== 404) throw error;
      await github.rest.issues.createLabel({ owner, repo, name, color, description });
    }
  }
}

async function selectedPullRequests(github, context, selector) {
  if (selector === "all") {
    const pulls = await github.paginate(github.rest.pulls.list, {
      owner: context.repo.owner,
      repo: context.repo.repo,
      state: "open",
      per_page: 100,
    });
    return pulls.map((pull) => pull.number);
  }
  if (/^[1-9]\d*$/.test(selector || "")) return [Number(selector)];
  return (context.payload.workflow_run?.pull_requests || []).map((pull) => pull.number);
}

async function run({ github, context, core, workspace, selector = "" }) {
  const { owner, repo } = context.repo;
  const numbers = await selectedPullRequests(github, context, selector);
  if (!numbers.length) {
    core.info("No pull request is associated with this triage run.");
    return [];
  }
  await ensureLabels(github, owner, repo);
  const desktopDependencies = readDesktopDependencies(workspace);
  const results = [];
  for (const pullNumber of numbers) {
    const { data: pull } = await github.rest.pulls.get({ owner, repo, pull_number: pullNumber });
    if (pull.user?.login !== "dependabot[bot]" || pull.state !== "open") continue;
    const files = await github.paginate(github.rest.pulls.listFiles, { owner, repo, pull_number: pullNumber, per_page: 100 });
    const classification = classify(pull.title, files, desktopDependencies);
    const desired = managedLabels(classification);
    const current = pull.labels.map((label) => typeof label === "string" ? label : label.name);
    for (const label of current.filter((name) => name?.startsWith("dependencies:") && LABELS[name] && !desired.includes(name))) {
      await github.rest.issues.removeLabel({ owner, repo, issue_number: pullNumber, name: label });
    }
    await github.rest.issues.addLabels({ owner, repo, issue_number: pullNumber, labels: desired });
    results.push({ pull: `#${pullNumber}`, ...classification });
  }
  if (results.length) {
    core.summary.addHeading("Dependabot risk classification").addTable([
      ["PR", "Dependency", "Scope", "Update", "Disposition"],
      ...results.map((item) => [item.pull, item.dependency, item.scope, item.update, item.disposition]),
    ]);
    await core.summary.write();
  }
  return results;
}

module.exports = { classify, dependencyName, managedLabels, run, scopeFor, updateType };
