const RETRYABLE_STEPS = new Set([
  "Checkout release ref",
  "Setup Node.js",
  "Setup Rust",
  "Cache Rust build",
  "Check release publication state",
]);

function failedSteps(jobs) {
  return jobs.flatMap((job) => (job.steps || [])
    .filter((step) => ["failure", "timed_out", "cancelled"].includes(step.conclusion))
    .map((step) => ({ job: job.name, step: step.name, conclusion: step.conclusion, url: job.html_url })));
}

function retryDecision(run, jobs) {
  const attempt = Number(run.run_attempt);
  const failures = failedSteps(jobs);
  if (run.conclusion !== "failure") return { retry: false, reason: `conclusion=${run.conclusion}`, failures };
  if (!Number.isSafeInteger(attempt) || attempt !== 1) {
    return { retry: false, reason: `run_attempt=${run.run_attempt || "unknown"}`, failures };
  }
  if (failures.length === 0) return { retry: false, reason: "no failed step was reported", failures };
  const unsafe = failures.filter((failure) => !RETRYABLE_STEPS.has(failure.step));
  if (unsafe.length) {
    return { retry: false, reason: `non-retryable step: ${unsafe.map((item) => item.step).join(", ")}`, failures };
  }
  return { retry: true, reason: "all failed steps are transient-boundary candidates", failures };
}

function code(value) {
  return `\`${String(value ?? "unknown").replaceAll("`", "'")}\``;
}

function issueBody(run, decision) {
  const lines = decision.failures.length
    ? decision.failures.map((failure) => `- ${code(failure.job)} / ${code(failure.step)}: ${code(failure.conclusion)} ([job](${failure.url}))`)
    : ["- GitHub did not report a failed step. Inspect the run annotations and logs."];
  const recovery = decision.retry
    ? "A single failed-jobs re-run will be requested automatically. A second failure requires human review."
    : "No automatic re-run is allowed for this failure. Fix deterministic source/build/security failures and create a new version tag; never move a published tag.";
  return [
    "Loom release automation recorded an unsuccessful Release Tag run.",
    "",
    `- Run: [${run.id}](${run.html_url})`,
    `- Attempt: ${code(run.run_attempt)}`,
    `- Conclusion: ${code(run.conclusion)}`,
    `- Commit: ${code(run.head_sha)}`,
    `- Ref: ${code(run.head_branch)}`,
    `- Retry decision: ${code(decision.reason)}`,
    "",
    "### Failed boundaries",
    ...lines,
    "",
    "### Recovery contract",
    recovery,
    "For an approved transient re-run use `gh run rerun <run-id> --failed`. Do not bypass dependency security, tests, smoke verification, attestations, or draft asset verification.",
  ].join("\n");
}

async function findIssue(github, owner, repo, title) {
  const issues = await github.paginate(github.rest.issues.listForRepo, {
    owner,
    repo,
    state: "all",
    per_page: 100,
  });
  return issues.find((issue) => !issue.pull_request && issue.title === title);
}

async function upsertIssue(github, owner, repo, title, body) {
  const existing = await findIssue(github, owner, repo, title);
  if (existing) {
    await github.rest.issues.update({ owner, repo, issue_number: existing.number, title, body, state: "open" });
    return existing.number;
  }
  const { data } = await github.rest.issues.create({ owner, repo, title, body });
  return data.number;
}

async function closeRecoveredIssue(github, owner, repo, title, run) {
  const existing = await findIssue(github, owner, repo, title);
  if (!existing || existing.state !== "open") return;
  await github.rest.issues.createComment({
    owner,
    repo,
    issue_number: existing.number,
    body: `Release run attempt ${code(run.run_attempt)} completed successfully: ${run.html_url}`,
  });
  await github.rest.issues.update({ owner, repo, issue_number: existing.number, state: "closed" });
}

async function run({ github, context, core }) {
  const eventRun = context.payload.workflow_run;
  const { owner, repo } = context.repo;
  const title = `[release-recovery] run ${eventRun.id}`;
  if (eventRun.conclusion === "success") {
    await closeRecoveredIssue(github, owner, repo, title, eventRun);
    core.info(`Release run ${eventRun.id} is green.`);
    return;
  }
  const [{ data: currentRun }, jobs] = await Promise.all([
    github.rest.actions.getWorkflowRun({ owner, repo, run_id: eventRun.id }),
    github.paginate(github.rest.actions.listJobsForWorkflowRun, {
      owner,
      repo,
      run_id: eventRun.id,
      filter: "latest",
      per_page: 100,
    }),
  ]);
  const decision = retryDecision(currentRun, jobs);
  const issueNumber = await upsertIssue(github, owner, repo, title, issueBody(currentRun, decision));
  core.warning(`Release run ${eventRun.id} needs recovery tracking in issue #${issueNumber}.`);
  if (!decision.retry) return;
  await github.rest.actions.reRunWorkflowFailedJobs({ owner, repo, run_id: eventRun.id });
  await github.rest.issues.createComment({
    owner,
    repo,
    issue_number: issueNumber,
    body: `GitHub accepted the single bounded failed-jobs re-run for attempt ${code(currentRun.run_attempt)}.`,
  });
}

module.exports = { failedSteps, issueBody, retryDecision, run };
