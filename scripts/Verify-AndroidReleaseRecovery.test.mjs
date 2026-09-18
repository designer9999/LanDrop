import assert from "node:assert/strict";
import test from "node:test";
import {
  loadDraftRelease,
  selectDraftRelease,
  validateInputs,
  validateRecovery,
} from "./Verify-AndroidReleaseRecovery.mjs";

function fixture() {
  const repository = "designer9999/LanDrop";
  const tagSha = "a".repeat(40);
  const names = [
    "LanDrop_1.8.2_x64-setup.exe",
    "LanDrop_1.8.2_x64-setup.exe.sig",
    "LanDrop_1.8.2_aarch64.dmg",
    "LanDrop_1.8.2_x64.dmg",
    "LanDrop_1.8.2_amd64.AppImage",
    "LanDrop_1.8.2_amd64.AppImage.sig",
    "LanDrop_1.8.2_amd64.deb",
    "LanDrop_1.8.2_amd64.deb.sig",
    "LanDrop_aarch64.app.tar.gz",
    "LanDrop_aarch64.app.tar.gz.sig",
    "LanDrop_x64.app.tar.gz",
    "LanDrop_x64.app.tar.gz.sig",
    "latest.json",
  ];
  const jobNames = [
    "Verify release source",
    "Create draft release",
    "Windows",
    "macOS (arm64)",
    "macOS (x64)",
    "Linux",
    "Android (arm64)",
    "Publish completed release",
  ];
  return {
    repository,
    tag: "v1.8.2",
    runId: "35363003429",
    tagSha,
    workflow: { id: 12, path: ".github/workflows/release.yml" },
    run: {
      id: 35363003429,
      repository: { full_name: repository },
      head_repository: { full_name: repository },
      path: ".github/workflows/release.yml",
      workflow_id: 12,
      event: "push",
      head_branch: "v1.8.2",
      head_sha: tagSha,
      run_attempt: 1,
      status: "completed",
      conclusion: "failure",
    },
    jobs: jobNames.map((name, index) => ({
      name,
      run_id: 35363003429,
      head_sha: tagSha,
      status: "completed",
      conclusion: index < 6 ? "success" : index === 6 ? "failure" : "skipped",
      steps: [
        { name: "Checkout", status: "completed", conclusion: "success" },
        { name: "Set up Android SDK", status: "completed", conclusion: "failure" },
        { name: "Build Android APK", status: "completed", conclusion: "skipped" },
      ],
    })),
    release: { id: 20, tag_name: "v1.8.2", draft: true, prerelease: false },
    assets: names.map((name, index) => ({
      id: index + 1,
      name,
      size: 1024,
      state: "uploaded",
      digest: `sha256:${"b".repeat(64)}`,
    })),
  };
}

test("draft selection requires exactly one stable unpublished matching tag", () => {
  const { release } = fixture();
  assert.equal(
    selectDraftRelease([{ ...release, tag_name: "v1.8.1" }, release], "v1.8.2"),
    release,
  );
  for (const releases of [
    [],
    [release, release],
    [{ ...release, draft: false }],
    [{ ...release, prerelease: true }],
    [{ ...release, id: 0 }],
  ]) {
    assert.throws(() => selectDraftRelease(releases, "v1.8.2"));
  }
});

test("draft lookup paginates and verifies exact release ID without by-tag fallback", async () => {
  const { release } = fixture();
  const calls = [];
  const result = await loadDraftRelease(async (path) => {
    calls.push(path);
    if (path === "releases?per_page=100&page=1")
      return Array.from({ length: 100 }, (_, i) => ({ tag_name: `v0.0.${i}` }));
    if (path === "releases?per_page=100&page=2") return [release];
    if (path === "releases/20") return release;
    throw new Error("Unexpected API path");
  }, "v1.8.2");
  assert.equal(result, release);
  assert.deepEqual(calls, [
    "releases?per_page=100&page=1",
    "releases?per_page=100&page=2",
    "releases/20",
  ]);
});

test("draft lookup rejects publication, tag change or ID change between requests", async () => {
  const { release } = fixture();
  for (const change of [{ draft: false }, { tag_name: "v1.8.1" }, { id: 99 }]) {
    await assert.rejects(() =>
      loadDraftRelease(
        async (path) => (path.startsWith("releases?") ? [release] : { ...release, ...change }),
        "v1.8.2",
      ),
    );
  }
});

test("draft lookup fails closed on pagination bound and API failure", async () => {
  let calls = 0;
  await assert.rejects(
    () =>
      loadDraftRelease(async () => {
        calls++;
        return Array(100).fill({ tag_name: "v0.0.0" });
      }, "v1.8.2"),
    /five-page/,
  );
  assert.equal(calls, 5);
  await assert.rejects(
    () =>
      loadDraftRelease(async () => {
        throw new Error("HTTP 403");
      }, "v1.8.2"),
    /403/,
  );
  await assert.rejects(() => loadDraftRelease(async () => ({}), "v1.8.2"), /inventory page/);
});

test("valid original SDK-only failure preserves every desktop asset", () => {
  const result = validateRecovery(fixture());
  assert.equal(result.source_sha, "a".repeat(40));
  assert.equal(result.release_id, "20");
  const snapshot = JSON.parse(result.asset_snapshot);
  assert.equal(snapshot.length, 13);
  assert.deepEqual(Object.keys(snapshot[0]).sort(), ["digest", "id", "name", "size"]);
  assert.deepEqual(
    snapshot.map((asset) => asset.name),
    snapshot.map((asset) => asset.name).sort(),
  );
});

test("invalid repository, tag and run inputs are rejected", () => {
  for (const change of [
    { repository: "attacker/LanDrop" },
    { tag: "v1.8.2-beta.1" },
    { tag: "v1.8.2\nsource_sha=x" },
    { runId: "1/attempts/2" },
    { runId: "0" },
  ]) {
    assert.throws(() => validateInputs({ ...fixture(), ...change }));
  }
});

const failures = {
  "moved tag": (f) => {
    f.tagSha = "c".repeat(40);
  },
  "different run": (f) => {
    f.run.id++;
  },
  "different tag": (f) => {
    f.run.head_branch = "v1.8.1";
  },
  "different workflow": (f) => {
    f.run.path = ".github/workflows/ci.yml";
  },
  "different workflow ID": (f) => {
    f.run.workflow_id++;
  },
  "fork run": (f) => {
    f.run.head_repository.full_name = "attacker/LanDrop";
  },
  "manual run": (f) => {
    f.run.event = "workflow_dispatch";
  },
  "rerun attempt": (f) => {
    f.run.run_attempt = 2;
  },
  "still running": (f) => {
    f.run.status = "in_progress";
  },
  "successful run": (f) => {
    f.run.conclusion = "success";
  },
  "desktop failure": (f) => {
    f.jobs[2].conclusion = "failure";
  },
  "missing desktop job": (f) => {
    f.jobs.splice(2, 1);
  },
  "duplicate job": (f) => {
    f.jobs[2] = f.jobs[1];
  },
  "unknown job": (f) => {
    f.jobs[2].name = "Other";
  },
  "job from another run": (f) => {
    f.jobs[2].run_id++;
  },
  "job from another commit": (f) => {
    f.jobs[2].head_sha = "c".repeat(40);
  },
  "publish already ran": (f) => {
    f.jobs[7].conclusion = "success";
  },
  "wrong Android failure": (f) => {
    f.jobs[6].steps[1].name = "Sign and verify Android APK";
  },
  "multiple Android failures": (f) => {
    f.jobs[6].steps[2].conclusion = "failure";
  },
  "cancelled Android step": (f) => {
    f.jobs[6].steps[2].conclusion = "cancelled";
  },
  "published release": (f) => {
    f.release.draft = false;
  },
  prerelease: (f) => {
    f.release.prerelease = true;
  },
  "wrong release tag": (f) => {
    f.release.tag_name = "v1.8.1";
  },
  "missing desktop asset": (f) => {
    f.assets.pop();
  },
  "duplicate desktop asset": (f) => {
    f.assets.push({ ...f.assets[0], id: 100 });
  },
  "ambiguous desktop asset": (f) => {
    f.assets.push({ ...f.assets[0], id: 100, name: "Another_x64-setup.exe" });
  },
  "Android already uploaded": (f) => {
    f.assets.push({ ...f.assets[0], id: 100, name: "landrop-android-arm64-release.apk" });
  },
  "checksums already published": (f) => {
    f.assets.push({ ...f.assets[0], id: 100, name: "SHA256SUMS.txt" });
  },
  "empty asset": (f) => {
    f.assets[0].size = 0;
  },
  "incomplete asset": (f) => {
    f.assets[0].state = "new";
  },
  "missing digest": (f) => {
    f.assets[0].digest = null;
  },
};
for (const [name, mutate] of Object.entries(failures)) {
  test(`rejects ${name}`, () => {
    const data = fixture();
    mutate(data);
    assert.throws(() => validateRecovery(data));
  });
}
