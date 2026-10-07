import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { preflightCommands } from "./local-ci.mjs";
import {
  assertProvenance,
  githubClient,
  prepareRelease,
  publishRelease,
  releaseConfig,
  releaseVersion,
  validateManifest,
  validateRelease,
  validateVersion,
} from "./release.mjs";

const readWorkflow = (name) =>
  readFileSync(new URL(`../workflows/${name}`, import.meta.url), "utf8").replace(/\r\n?/g, "\n");

const sha = "a".repeat(40);
const version = "0.1.19";
const tag = `v${version}`;
const runUrl = "https://github.com/owner/repo/actions/runs/19";

test("macOS bundles receive a complete code signature independently of updater signing", () => {
  const config = JSON.parse(
    readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  );
  assert.equal(config.bundle.macOS.signingIdentity, "-");
  assert.equal(config.bundle.createUpdaterArtifacts, false);
});

function fixture(draft = true) {
  const names = [
    "Open.Media.Backup_0.1.19_universal.dmg",
    "Open.Media.Backup_universal.app.tar.gz",
    "Open.Media.Backup_universal.app.tar.gz.sig",
    "Open.Media.Backup_0.1.19_x64-setup.exe",
    "Open.Media.Backup_0.1.19_x64-setup.exe.sig",
    "Open.Media.Backup_0.1.19_x64_en-US.msi",
    "Open.Media.Backup_0.1.19_x64_en-US.msi.sig",
    "latest.json",
  ];
  const release = {
    id: 123,
    tag_name: tag,
    target_commitish: sha,
    draft,
    prerelease: false,
    body: `Build run: ${runUrl}`,
    assets: names.map((name, index) => ({
      id: index + 1,
      name,
      size: 100,
      state: "uploaded",
      browser_download_url: `https://github.com/owner/repo/releases/download/${tag}/${name}`,
    })),
  };
  const mac = { url: release.assets[1].browser_download_url, signature: "mac-signature\n" };
  const windows = { url: release.assets[5].browser_download_url, signature: "windows-signature\n" };
  const manifest = {
    version,
    platforms: { "darwin-aarch64": mac, "darwin-x86_64": mac, "windows-x86_64": windows },
  };
  return { release, manifest };
}

function mockApi(release, manifest) {
  const calls = [];
  const api = async (path, options = {}) => {
    calls.push({ path, ...options });
    if (options.method === "PATCH") return { ...release, ...options.body };
    if (options.method === "DELETE") return null;
    if (path.startsWith("/git/ref/")) return null;
    if (path.startsWith("/releases?")) return release ? [release] : [];
    if (path === "/releases" && options.method === "POST") return { id: 123, ...options.body };
    if (path === "/releases/123") return release;
    if (path === "/releases/assets/8") return JSON.stringify(manifest);
    if (path === "/releases/assets/3") return "mac-signature\n";
    if (path === "/releases/assets/7") return "windows-signature\n";
    throw new Error(`Unexpected API request ${path}`);
  };
  return { api, calls };
}

test("automatic versions use the caller CI run number, not the checked-in patch", () => {
  assert.equal(releaseVersion("0.1.0", 19, ""), version);
  assert.equal(releaseVersion("0.1.0", "19", ""), version);
  assert.equal(releaseVersion("2.3.4", 21, ""), "2.3.21");
  assert.equal(releaseVersion("0.1.0", 19, ""), releaseVersion("0.1.0", 19, ""));
  for (const value of [0, -1, 1.5, "01", undefined, "19\nversion=evil"]) {
    assert.throws(() => releaseVersion("0.1.0", value, ""), /build number/);
  }
});

test("manual stable tags determine the installer version independently of CI numbering", () => {
  assert.equal(releaseVersion("0.1.0", 0, "v0.2.3"), "0.2.3");
  for (const value of ["v0.2.3-beta.1", "v0.2.3+19", "v01.2.3", "version", "0.2.3"]) {
    assert.throws(() => releaseVersion("0.1.0", 0, value));
  }
});

test("versions respect Windows installer limits and reject malformed output", () => {
  assert.equal(validateVersion("255.255.65535"), "255.255.65535");
  for (const value of ["256.0.0", "0.256.0", "0.1.65536", "0.1.0\n", "1.2", undefined]) {
    assert.throws(() => validateVersion(value));
  }
});

test("the effective release config carries the version without altering updater settings", () => {
  const source = { bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: "key" } } };
  const configured = releaseConfig(source, version);
  assert.equal(configured.version, version);
  assert.deepEqual(configured.bundle, source.bundle);
  assert.deepEqual(configured.plugins, source.plugins);
  assert.equal(source.version, undefined);
  assert.throws(() => releaseConfig({ bundle: { createUpdaterArtifacts: false } }, version), /signed/);
});

test("configure CLI changes only the release overlay and preserves unsigned local builds", () => {
  const directory = mkdtempSync(join(tmpdir(), "omb-release-test-"));
  try {
    mkdirSync(join(directory, "src-tauri"));
    const appConfig = readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8");
    const overlay = readFileSync(new URL("../../src-tauri/tauri.release.conf.json", import.meta.url), "utf8");
    writeFileSync(join(directory, "src-tauri/tauri.conf.json"), appConfig);
    writeFileSync(join(directory, "src-tauri/tauri.release.conf.json"), overlay);
    const result = spawnSync(
      process.execPath,
      [fileURLToPath(new URL("./release.mjs", import.meta.url)), "configure"],
      { cwd: directory, env: { ...process.env, RELEASE_VERSION: version }, encoding: "utf8" },
    );
    assert.equal(result.status, 0, result.stderr);
    const configured = JSON.parse(readFileSync(join(directory, "src-tauri/tauri.release.conf.json"), "utf8"));
    assert.equal(configured.version, version);
    assert.equal(configured.bundle.createUpdaterArtifacts, true);
    assert.equal(readFileSync(join(directory, "src-tauri/tauri.conf.json"), "utf8"), appConfig);
    assert.equal(JSON.parse(appConfig).bundle.createUpdaterArtifacts, false);
  } finally {
    rmSync(directory, { recursive: true });
  }
});

test("prepare creates a stable draft at the exact tested SHA", async () => {
  const { api, calls } = mockApi(null);
  const outputs = await prepareRelease(api, {
    version,
    sha,
    runUrl: "https://github.com/owner/repo/actions/runs/19",
  });
  assert.deepEqual(outputs, { version, tag, "release-id": "123", published: "false" });
  const create = calls.find((call) => call.method === "POST");
  assert.equal(create.body.target_commitish, sha);
  assert.equal(create.body.draft, true);
  assert.equal(create.body.prerelease, false);
  assert.match(create.body.body, /actions\/runs\/19/);
  assert.throws(
    () => assertProvenance({ ...fixture().release, target_commitish: "b".repeat(40) }, tag, sha),
    /provenance/,
  );
  await assert.rejects(prepareRelease(api, { version, sha: "main" }), /full tested commit SHA/);
});

test("draft retries reuse the release and reset stale universal updater metadata", async () => {
  const { release, manifest } = fixture();
  const { api, calls } = mockApi(release, manifest);
  const outputs = await prepareRelease(api, { version, sha, runUrl });
  assert.equal(outputs["release-id"], "123");
  assert.equal(outputs.published, "false");
  assert.equal(
    calls.some((call) => call.method === "POST"),
    false,
  );
  assert.deepEqual(
    calls.filter((call) => call.method === "DELETE"),
    [{ path: "/releases/assets/8", method: "DELETE" }],
  );
});

test("published retries validate but never replace assets or change latest", async () => {
  const { release, manifest } = fixture(false);
  const { api, calls } = mockApi(release, manifest);
  assert.equal((await prepareRelease(api, { version, sha })).published, "true");
  await publishRelease(api, { id: 123, version, sha });
  assert.equal(
    calls.some((call) => ["POST", "PATCH", "DELETE"].includes(call.method)),
    false,
  );
});

test("colliding releases, prereleases and tags at another commit fail before mutation", async () => {
  const { release, manifest } = fixture();
  for (const override of [{ target_commitish: "b".repeat(40) }, { prerelease: true }]) {
    const { api, calls } = mockApi({ ...release, ...override }, manifest);
    await assert.rejects(prepareRelease(api, { version, sha }), /provenance/);
    assert.equal(
      calls.some((call) => call.method),
      false,
    );
  }
  const api = async () => ({ object: { type: "commit", sha: "b".repeat(40) } });
  await assert.rejects(prepareRelease(api, { version, sha }), /different commit/);
});

test("a draft from another run cannot share mutable assets even at the same commit", async () => {
  const { release, manifest } = fixture();
  const { api, calls } = mockApi(release, manifest);
  await assert.rejects(prepareRelease(api, { version, sha, runUrl: `${runUrl}0` }), /another workflow run/);
  assert.equal(
    calls.some((call) => call.method),
    false,
  );
});

test("annotated tags are resolved to their underlying tested commit", async () => {
  const { api } = mockApi(null);
  const taggedApi = (path, options) => {
    if (path.startsWith("/git/ref/")) return { object: { type: "tag", sha: "c".repeat(40) } };
    if (path.startsWith("/git/tags/")) return { object: { type: "commit", sha } };
    return api(path, options);
  };
  assert.equal((await prepareRelease(taggedApi, { version, sha })).published, "false");
});

test("draft assets with untagged URLs match the post-publication manifest URLs", async () => {
  const { release, manifest } = fixture();
  for (const asset of release.assets) {
    asset.browser_download_url = asset.browser_download_url.replace(
      `/download/${tag}/`,
      "/download/untagged-123/",
    );
  }
  const { api } = mockApi(release, manifest);
  await validateRelease(api, release, version);
});

test("only complete, correctly versioned signed manifests are accepted", () => {
  const { release, manifest } = fixture();
  validateManifest(manifest, release, version);
  const changes = [
    (value) => {
      value.version = "0.1.0";
    },
    (value) => {
      delete value.platforms["darwin-aarch64"];
    },
    (value) => {
      delete value.platforms["darwin-x86_64"];
    },
    (value) => {
      delete value.platforms["windows-x86_64"];
    },
    (value) => {
      value.platforms["windows-x86_64"].signature = " ";
    },
    (value) => {
      value.platforms["windows-x86_64"].url = "https://example.com/installer.exe";
    },
    (value) => {
      value.platforms["windows-x86_64"].url = release.assets[1].browser_download_url;
    },
  ];
  for (const change of changes) {
    const modified = structuredClone(manifest);
    change(modified);
    assert.throws(() => validateManifest(modified, release, version));
  }
  for (const asset of release.assets.filter((item) => item.name !== "latest.json")) {
    const missing = { ...release, assets: release.assets.filter((item) => item.id !== asset.id) };
    assert.throws(() => validateManifest(manifest, missing, version), /missing|signed/);
    const incomplete = structuredClone(release);
    incomplete.assets.find((item) => item.id === asset.id).state = "open";
    assert.throws(() => validateManifest(manifest, incomplete, version));
  }
});

test("publication fails without latest.json or matching signatures and leaves the draft private", async () => {
  const { release, manifest } = fixture();
  const broken = { ...release, assets: release.assets.filter((asset) => asset.name !== "latest.json") };
  const { api, calls } = mockApi(broken, manifest);
  await assert.rejects(publishRelease(api, { id: 123, version, sha }), /missing latest.json/);
  assert.equal(
    calls.some((call) => call.method === "PATCH"),
    false,
  );
  const signed = mockApi(release, manifest);
  const badSignatureApi = (path, options) =>
    path === "/releases/assets/3" ? "stale-signature" : signed.api(path, options);
  await assert.rejects(
    publishRelease(badSignatureApi, { id: 123, version, sha }),
    /signature.*does not match/,
  );
  assert.equal(
    signed.calls.some((call) => call.method === "PATCH"),
    false,
  );
});

test("publication is stable and uses version-aware latest selection for out-of-order builds", async () => {
  const { release, manifest } = fixture();
  const { api, calls } = mockApi(release, manifest);
  const published = await publishRelease(api, { id: 123, version, sha });
  assert.equal(published.draft, false);
  assert.deepEqual(calls.find((call) => call.method === "PATCH").body, {
    draft: false,
    prerelease: false,
    make_latest: "legacy",
  });
});

test("GitHub client only tolerates expected missing tags and handles empty DELETE responses", async () => {
  const env = { GITHUB_TOKEN: "test-token", GITHUB_REPOSITORY: "owner/repo" };
  const api = githubClient(env, async (_url, options) => {
    assert.equal(options.headers.Authorization, "Bearer test-token");
    return new Response(null, { status: 404 });
  });
  assert.equal(await api("/git/ref/tags/v0.1.19", { optional: true }), null);
  await assert.rejects(api("/releases"), /failed \(404\)/);
  const denied = githubClient(env, async () => new Response("denied", { status: 403 }));
  await assert.rejects(denied("/git/ref/tags/v0.1.19", { optional: true }), /403.*denied/);
  const deletion = githubClient(env, async () => new Response(null, { status: 204 }));
  assert.equal(await deletion("/releases/assets/8", { method: "DELETE" }), null);
  assert.throws(() => githubClient({}), /required/);
});

test("release lookup includes drafts beyond the first page", async () => {
  const { release, manifest } = fixture();
  const { api } = mockApi(release, manifest);
  const paginated = (path, options) =>
    path === "/releases?per_page=100&page=1"
      ? Array.from({ length: 100 }, (_, index) => ({ tag_name: `v9.0.${index}` }))
      : api(path, options);
  assert.equal((await prepareRelease(paginated, { version, sha, runUrl }))["release-id"], "123");
});

test("CI gates publishing on tests and main pushes, and passes the caller run number and SHA", () => {
  const ci = readWorkflow("ci.yml");
  assert.match(ci, /permissions:\n  contents: read/);
  assert.match(
    ci,
    /release:\n    needs: (?:test|\[test, hash-server\])\n    if: github.event_name == 'push' && github.ref == 'refs\/heads\/main'/,
  );
  assert.match(ci, /uses: \.\/\.github\/workflows\/release.yml/);
  assert.match(ci, /commit: \$\{\{ github.sha \}\}/);
  assert.match(ci, /build-number: \$\{\{ github.run_number \}\}/);
  assert.match(ci, /run: npm run ci:local/);
  assert.ok(preflightCommands()[0].includes(".github/scripts/release.test.mjs"));
  const workflow = readWorkflow("release.yml");
  assert.match(workflow, /tags:\n      - "v\*"/);
  assert.match(workflow, /workflow_call:/);
  assert.match(workflow, /if: needs.prepare.outputs.published != 'true'/);
  assert.match(workflow, /max-parallel: 1/);
  assert.match(workflow, /releaseDraft: true/);
  assert.match(workflow, /includeUpdaterJson: true/);
  assert.match(workflow, /publish:\n    needs: \[prepare, build, hash-server\]/);
  assert.match(workflow, /ref: \$\{\{ needs.prepare.outputs.commit \}\}/);
  assert.doesNotMatch(workflow, /continue-on-error|cancel-in-progress|workflow_run/);
});

test("hash-server images use the tested commit and release version on both architectures", () => {
  const workflow = readWorkflow("release.yml").split("  hash-server:\n")[1];
  assert.ok(workflow);
  assert.match(workflow, /needs: \[prepare, build\]/);
  assert.match(workflow, /ref: \$\{\{ needs.prepare.outputs.commit \}\}/);
  assert.match(workflow, /packages: write/);
  assert.match(workflow, /docker\/setup-buildx-action@v3/);
  assert.match(workflow, /docker\/build-push-action@v6/);
  assert.match(workflow, /platforms: linux\/amd64,linux\/arm64/);
  assert.match(workflow, /ghcr.io\/loic294\/omb-hash-server:\$\{\{ needs.prepare.outputs.version \}\}/);
  assert.match(workflow, /ghcr.io\/loic294\/omb-hash-server:latest/);
  assert.match(workflow, /OMB_HASH_VERSION=\$\{\{ needs.prepare.outputs.version \}\}/);
});
