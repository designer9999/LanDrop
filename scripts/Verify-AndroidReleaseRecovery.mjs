import assert from "node:assert/strict";
import { appendFileSync } from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

const REPOSITORY = "designer9999/LanDrop";
const WORKFLOW = ".github/workflows/release.yml";
const SHA = /^[0-9a-f]{40}$/;
const JOBS = new Map([
  ["Verify release source", "success"],
  ["Create draft release", "success"],
  ["Windows", "success"],
  ["macOS (arm64)", "success"],
  ["macOS (x64)", "success"],
  ["Linux", "success"],
  ["Android (arm64)", "failure"],
  ["Publish completed release", "skipped"],
]);

export function validateInputs({ repository, tag, runId }) {
  assert.equal(repository, REPOSITORY, "Recovery repository must be designer9999/LanDrop");
  assert.match(
    tag ?? "",
    /^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/,
    "Stable release tag required",
  );
  assert.match(runId ?? "", /^[1-9]\d*$/, "Numeric failed run ID required");
}

export function selectDraftRelease(releases, tag) {
  assert(Array.isArray(releases), "Invalid release inventory");
  const matches = releases.filter((release) => release?.tag_name === tag);
  assert.equal(matches.length, 1, "Expected exactly one release for the recovery tag");
  const release = matches[0];
  assert(Number.isSafeInteger(release.id) && release.id > 0, "Invalid draft release ID");
  assert.equal(release.draft, true, "Recovery requires an existing draft release");
  assert.equal(release.prerelease, false, "Recovery requires a stable release");
  return release;
}

export async function loadDraftRelease(api, tag) {
  // The by-tag endpoint may omit unpublished drafts. Enumerate authenticated
  // metadata explicitly, fail closed on truncation, then re-read the exact ID.
  const releases = [];
  for (let page = 1; page <= 5; page++) {
    const batch = await api(`releases?per_page=100&page=${page}`);
    assert(Array.isArray(batch) && batch.length <= 100, "Invalid release inventory page");
    releases.push(...batch);
    if (batch.length < 100) {
      const selected = selectDraftRelease(releases, tag);
      const current = await api(`releases/${selected.id}`);
      assert.equal(current.id, selected.id, "Draft release identity changed");
      return selectDraftRelease([current], tag);
    }
  }
  throw new Error("Release inventory exceeds the five-page recovery bound");
}

export function validateRecovery({
  repository,
  tag,
  runId,
  tagSha,
  workflow,
  run,
  jobs,
  release,
  assets,
}) {
  validateInputs({ repository, tag, runId });
  assert.match(tagSha ?? "", SHA, "Tag must resolve to a commit SHA");
  assert.equal(String(run.id), runId, "Wrong workflow run");
  assert.equal(run.repository?.full_name, repository, "Wrong run repository");
  assert.equal(run.head_repository?.full_name, repository, "Fork runs cannot be recovered");
  assert.equal(run.path, WORKFLOW, "Wrong run workflow path");
  assert.equal(workflow.path, WORKFLOW, "Wrong workflow definition");
  assert(Number.isSafeInteger(workflow.id) && workflow.id > 0, "Invalid workflow ID");
  assert.equal(run.workflow_id, workflow.id, "Wrong workflow ID");
  assert.equal(run.event, "push", "Only original tag-push runs can be recovered");
  assert.equal(run.head_branch, tag, "Run does not belong to this tag");
  assert.equal(run.head_sha, tagSha, "Tag moved or run belongs to a different source revision");
  assert.equal(run.run_attempt, 1, "Recovery is limited to the original run attempt");
  assert.equal(run.status, "completed", "Failed run is not complete");
  assert.equal(run.conclusion, "failure", "Run did not fail");
  assert.equal(jobs.length, JOBS.size, "Unexpected workflow jobs");
  assert.equal(new Set(jobs.map((job) => job.name)).size, JOBS.size, "Duplicate workflow jobs");
  for (const job of jobs) {
    assert(JOBS.has(job.name), "Unknown workflow job");
    assert.equal(String(job.run_id), runId, "Job belongs to another run");
    assert.equal(job.head_sha, tagSha, "Job belongs to another source revision");
    assert.equal(job.status, "completed", "Workflow job is incomplete");
    assert.equal(job.conclusion, JOBS.get(job.name), "Unexpected workflow job outcome");
  }
  const android = jobs.find((job) => job.name === "Android (arm64)");
  assert(Array.isArray(android.steps) && android.steps.length > 0, "Android steps are missing");
  const failures = android.steps.filter((step) => step.conclusion === "failure");
  assert.equal(failures.length, 1, "Android must have exactly one failed step");
  assert.equal(failures[0].name, "Set up Android SDK", "Android failed outside SDK setup");
  for (const step of android.steps) {
    assert.equal(step.status, "completed", "Android step is incomplete");
    assert(
      ["success", "skipped", "failure"].includes(step.conclusion),
      "Unexpected Android step outcome",
    );
  }

  assert(Number.isSafeInteger(release.id) && release.id > 0, "Invalid release ID");
  assert.equal(release.tag_name, tag, "Wrong release tag");
  assert.equal(release.draft, true, "Recovery requires an existing draft release");
  assert.equal(release.prerelease, false, "Recovery requires a stable release");
  assert(Array.isArray(assets) && assets.length <= 100, "Unexpected release asset inventory");
  assert.equal(
    new Set(assets.map((asset) => asset.name)).size,
    assets.length,
    "Duplicate asset names",
  );
  assert.equal(new Set(assets.map((asset) => asset.id)).size, assets.length, "Duplicate asset IDs");
  for (const asset of assets) {
    assert(Number.isSafeInteger(asset.id) && asset.id > 0, "Invalid asset ID");
    assert(
      typeof asset.name === "string" && /^[A-Za-z0-9_.+-]+$/.test(asset.name),
      "Invalid asset name",
    );
    assert(!/\.apk$/i.test(asset.name), "An Android APK already exists");
    assert(!/^SHA256SUMS(?:\..*)?$/i.test(asset.name), "Release finalization has already started");
    assert.equal(asset.state, "uploaded", "An existing asset is not fully uploaded");
    assert(Number.isSafeInteger(asset.size) && asset.size > 0, "Empty or invalid release asset");
    assert.match(asset.digest ?? "", /^sha256:[0-9a-f]{64}$/, "Asset SHA-256 digest is missing");
  }
  const patterns = [
    /_x64-setup\.exe$/,
    /_x64-setup\.exe\.sig$/,
    /_aarch64\.dmg$/,
    /_x64\.dmg$/,
    /_amd64\.AppImage$/,
    /_amd64\.AppImage\.sig$/,
    /_amd64\.deb$/,
    /_amd64\.deb\.sig$/,
    /^LanDrop_aarch64\.app\.tar\.gz$/,
    /^LanDrop_aarch64\.app\.tar\.gz\.sig$/,
    /^LanDrop_x64\.app\.tar\.gz$/,
    /^LanDrop_x64\.app\.tar\.gz\.sig$/,
    /^latest\.json$/,
  ];
  for (const pattern of patterns) {
    assert.equal(
      assets.filter((asset) => pattern.test(asset.name)).length,
      1,
      `Required desktop asset missing or ambiguous: ${pattern}`,
    );
  }
  return {
    source_sha: tagSha,
    release_id: String(release.id),
    asset_snapshot: JSON.stringify(
      assets
        .map(({ id, name, digest, size }) => ({ id, name, digest, size }))
        .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)),
    ),
  };
}

async function main() {
  const repository = process.env.GH_REPO;
  const tag = process.env.RELEASE_TAG;
  const runId = process.env.FAILED_RUN_ID;
  validateInputs({ repository, tag, runId });
  const token = process.env.GH_TOKEN;
  assert(token && !/[\r\n]/.test(token), "GH_TOKEN is required");
  assert(process.env.GITHUB_OUTPUT, "GITHUB_OUTPUT is required");
  const api = async (path) => {
    const response = await fetch(`https://api.github.com/repos/${repository}/${path}`, {
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
      },
      redirect: "error",
      signal: AbortSignal.timeout(20_000),
    });
    assert(response.ok, `GitHub recovery metadata request failed (HTTP ${response.status})`);
    const body = await response.text();
    assert(body.length <= 2_000_000, "GitHub metadata response too large");
    try {
      return JSON.parse(body);
    } catch {
      throw new Error("GitHub metadata response is not valid JSON");
    }
  };
  const ref = await api(`git/ref/tags/${encodeURIComponent(tag)}`);
  assert.equal(ref.ref, `refs/tags/${tag}`, "Wrong remote tag reference");
  let object = ref.object;
  for (let depth = 0; object?.type === "tag" && depth < 5; depth++) {
    assert.match(object.sha ?? "", SHA, "Invalid annotated tag SHA");
    const annotated = await api(`git/tags/${object.sha}`);
    assert.equal(annotated.sha, object.sha, "Annotated tag identity mismatch");
    object = annotated.object;
  }
  assert.equal(object?.type, "commit", "Tag must resolve to a commit within five annotations");
  const [workflow, run, jobPage, release] = await Promise.all([
    api("actions/workflows/release.yml"),
    api(`actions/runs/${runId}`),
    api(`actions/runs/${runId}/attempts/1/jobs?per_page=100`),
    loadDraftRelease(api, tag),
  ]);
  assert.equal(jobPage.total_count, jobPage.jobs?.length, "Incomplete workflow job inventory");
  assert(Number.isSafeInteger(release.id) && release.id > 0, "Invalid release ID");
  const assets = await api(`releases/${release.id}/assets?per_page=100`);
  assert(assets.length < 100, "Release assets require unsupported pagination");
  const result = validateRecovery({
    repository,
    tag,
    runId,
    tagSha: object.sha,
    workflow,
    run,
    jobs: jobPage.jobs,
    release,
    assets,
  });
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    Object.entries(result)
      .map(([key, value]) => `${key}=${value}\n`)
      .join(""),
    "utf8",
  );
  console.log(
    `Verified Android SDK-only recovery for ${tag}: source ${result.source_sha}, draft ${result.release_id}, ${assets.length} preserved assets`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    // Never print response bodies, headers, credentials, or nested fetch causes.
    console.error(`Android release recovery blocked: ${error.message}`);
    process.exitCode = 1;
  });
}
