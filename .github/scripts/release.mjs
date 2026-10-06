import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const requiredPlatforms = ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"];

export function validateVersion(version) {
  if (typeof version !== "string" || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error(`Expected a stable X.Y.Z version, received ${version}`);
  }
  const parts = version.split(".").map(Number);
  if (parts[0] > 255 || parts[1] > 255 || parts[2] > 65535) {
    throw new Error(`Version ${version} exceeds Windows MSI version limits (255.255.65535)`);
  }
  return version;
}

export function releaseVersion(baseVersion, buildNumber, tag) {
  validateVersion(baseVersion);
  if (tag) {
    if (!tag.startsWith("v")) throw new Error(`Expected a vX.Y.Z release tag, received ${tag}`);
    return validateVersion(tag.slice(1));
  }
  if (!/^[1-9]\d*$/.test(String(buildNumber))) {
    throw new Error(`Expected a positive CI build number, received ${buildNumber}`);
  }
  const [major, minor] = baseVersion.split(".");
  return validateVersion(`${major}.${minor}.${buildNumber}`);
}

export function releaseConfig(config, version) {
  if (config.bundle?.createUpdaterArtifacts !== true) {
    throw new Error("Release configuration must enable signed updater artifacts");
  }
  return { ...config, version: validateVersion(version) };
}

export function assertProvenance(release, tag, sha) {
  if (release.tag_name !== tag || release.target_commitish !== sha || release.prerelease) {
    throw new Error(`Release ${tag} already exists with different commit provenance or release type`);
  }
}

function assetUrl(asset, release) {
  return asset.browser_download_url.replace(
    /\/download\/untagged-[^/]+\//,
    `/download/${encodeURIComponent(release.tag_name)}/`,
  );
}

export function validateManifest(manifest, release, version) {
  if (manifest.version !== validateVersion(version)) {
    throw new Error(`Updater version ${manifest.version} does not match release version ${version}`);
  }
  const assets = release.assets.filter((asset) => asset.state === "uploaded" && asset.size > 0);
  for (const suffix of [".dmg", ".exe", ".msi", ".app.tar.gz", ".exe.sig", ".msi.sig", ".app.tar.gz.sig"]) {
    if (!assets.some((asset) => asset.name.endsWith(suffix))) {
      throw new Error(`Release is missing a complete ${suffix} asset`);
    }
  }
  for (const platform of requiredPlatforms) {
    const entry = manifest.platforms?.[platform];
    if (!entry || typeof entry.signature !== "string" || !entry.signature.trim()) {
      throw new Error(`Updater manifest is missing signed platform ${platform}`);
    }
    const asset = assets.find((item) => assetUrl(item, release) === entry.url);
    if (!asset || !assets.some((item) => item.name === `${asset.name}.sig`)) {
      throw new Error(`Updater platform ${platform} does not reference an uploaded signed asset`);
    }
    const extension = platform.startsWith("darwin-") ? /\.app\.tar\.gz$/ : /\.(msi|exe)$/;
    if (!extension.test(asset.name)) {
      throw new Error(`Updater platform ${platform} references the wrong installer type`);
    }
  }
}

export function githubClient(env = process.env, fetcher = fetch) {
  if (!env.GITHUB_TOKEN || !/^[\w.-]+\/[\w.-]+$/.test(env.GITHUB_REPOSITORY ?? "")) {
    throw new Error("GITHUB_TOKEN and a valid GITHUB_REPOSITORY are required");
  }
  const base = `${env.GITHUB_API_URL || "https://api.github.com"}/repos/${env.GITHUB_REPOSITORY}`;
  return async (path, { method = "GET", body, optional = false, text = false } = {}) => {
    const response = await fetcher(`${base}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${env.GITHUB_TOKEN}`,
        Accept: text ? "application/octet-stream" : "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "Content-Type": "application/json",
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    if (optional && response.status === 404) return null;
    if (!response.ok) {
      throw new Error(`GitHub ${method} ${path} failed (${response.status}): ${await response.text()}`);
    }
    if (response.status === 204) return null;
    return text ? response.text() : response.json();
  };
}

async function findRelease(api, tag) {
  for (let page = 1; ; page += 1) {
    const releases = await api(`/releases?per_page=100&page=${page}`);
    const release = releases.find((item) => item.tag_name === tag);
    if (release) return release;
    if (releases.length < 100) return null;
  }
}

async function checkTag(api, tag, sha) {
  const ref = await api(`/git/ref/tags/${encodeURIComponent(tag)}`, { optional: true });
  if (!ref) return;
  let object = ref.object;
  for (let depth = 0; object.type === "tag" && depth < 10; depth += 1) {
    object = (await api(`/git/tags/${object.sha}`)).object;
  }
  if (object.type !== "commit" || object.sha !== sha) {
    throw new Error(`Tag ${tag} already points to a different commit`);
  }
}

export async function validateRelease(api, release, version) {
  const manifestAsset = release.assets.find(
    (asset) => asset.name === "latest.json" && asset.state === "uploaded" && asset.size > 0,
  );
  if (!manifestAsset) throw new Error("Release is missing latest.json");
  const manifest = JSON.parse(await api(`/releases/assets/${manifestAsset.id}`, { text: true }));
  validateManifest(manifest, release, version);
  for (const platform of requiredPlatforms) {
    const entry = manifest.platforms[platform];
    const asset = release.assets.find((item) => assetUrl(item, release) === entry.url);
    const signature = release.assets.find((item) => item.name === `${asset.name}.sig`);
    const contents = await api(`/releases/assets/${signature.id}`, { text: true });
    if (contents.trim() !== entry.signature.trim()) {
      throw new Error(`Updater signature for ${platform} does not match its uploaded signature`);
    }
  }
}

export async function prepareRelease(api, { version, sha, runUrl }) {
  validateVersion(version);
  if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error("A full tested commit SHA is required");
  const tag = `v${version}`;
  await checkTag(api, tag, sha);
  let release = await findRelease(api, tag);
  if (release) {
    assertProvenance(release, tag, sha);
    if (!release.draft) {
      await validateRelease(api, release, version);
    } else {
      if (!runUrl || !release.body?.split("\n").includes(`Build run: ${runUrl}`)) {
        throw new Error(
          `Draft ${tag} belongs to another workflow run; refusing concurrent asset replacement`,
        );
      }
      // tauri-action preserves universal platform entries when merging latest.json.
      for (const asset of release.assets.filter((item) => item.name === "latest.json")) {
        await api(`/releases/assets/${asset.id}`, { method: "DELETE" });
      }
    }
  } else {
    release = await api("/releases", {
      method: "POST",
      body: {
        tag_name: tag,
        target_commitish: sha,
        name: `Open Media Backup ${tag}`,
        body: `See the assets below to download and install this version.\n\nBuilt from ${sha}.\nBuild run: ${runUrl}`,
        draft: true,
        prerelease: false,
      },
    });
  }
  return {
    version,
    tag,
    "release-id": String(release.id),
    published: String(!release.draft),
  };
}

export async function publishRelease(api, { id, version, sha }) {
  if (!/^[1-9]\d*$/.test(String(id))) throw new Error("A valid release ID is required");
  const release = await api(`/releases/${id}`);
  assertProvenance(release, `v${validateVersion(version)}`, sha);
  await checkTag(api, release.tag_name, sha);
  await validateRelease(api, release, version);
  if (!release.draft) return release;
  return api(`/releases/${id}`, {
    method: "PATCH",
    body: { draft: false, prerelease: false, make_latest: "legacy" },
  });
}

async function main(command, env) {
  if (command === "configure") {
    const path = "src-tauri/tauri.release.conf.json";
    const config = releaseConfig(JSON.parse(readFileSync(path, "utf8")), env.RELEASE_VERSION);
    writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`);
    return;
  }
  const api = githubClient(env);
  if (command === "prepare") {
    const baseVersion = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
    const version = releaseVersion(baseVersion, env.RELEASE_BUILD_NUMBER, env.RELEASE_TAG);
    const outputs = await prepareRelease(api, {
      version,
      sha: env.RELEASE_SHA,
      runUrl: `${env.GITHUB_SERVER_URL}/${env.GITHUB_REPOSITORY}/actions/runs/${env.GITHUB_RUN_ID}`,
    });
    if (!env.GITHUB_OUTPUT) throw new Error("GITHUB_OUTPUT is required");
    appendFileSync(
      env.GITHUB_OUTPUT,
      Object.entries(outputs)
        .map(([key, value]) => `${key}=${value}\n`)
        .join(""),
    );
  } else if (command === "publish") {
    const release = await publishRelease(api, {
      id: env.RELEASE_ID,
      version: env.RELEASE_VERSION,
      sha: env.RELEASE_SHA,
    });
    console.log(`Published ${release.tag_name}: ${release.html_url}`);
  } else {
    throw new Error(`Unknown release command: ${command}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main(process.argv[2], process.env);
}
