export interface Env {
  GITHUB_TOKEN?: string;
}

interface GitHubAsset {
  name: string;
  url: string;
  browser_download_url: string;
}

interface GitHubRelease {
  body?: string | null;
  published_at?: string | null;
  assets: GitHubAsset[];
}

interface PlatformInfo {
  url: string;
  signature: string;
}

interface LatestManifest {
  version: string;
  notes?: string | null;
  pub_date?: string | null;
  platforms: Record<string, PlatformInfo>;
}

const REPO = "loic294/open-media-backup";
const API_BASE = "https://api.github.com";
const USER_AGENT = "open-media-backup-update-proxy";
const CACHE_SECONDS = 300;

function normalizeVersion(version: string): string {
  return version.trim().replace(/^v/i, "");
}

function parseVersion(version: string): { core: number[]; pre: string[] } {
  const [coreText, preText = ""] = normalizeVersion(version).split("-", 2);
  const core = coreText.split(".").map((part) => {
    const value = Number.parseInt(part, 10);
    return Number.isFinite(value) ? value : 0;
  });
  while (core.length < 3) core.push(0);
  return { core, pre: preText ? preText.split(".") : [] };
}

function compareIdentifiers(left: string, right: string): number {
  const leftNumber = /^\d+$/.test(left) ? Number.parseInt(left, 10) : null;
  const rightNumber = /^\d+$/.test(right) ? Number.parseInt(right, 10) : null;
  if (leftNumber !== null && rightNumber !== null) return Math.sign(leftNumber - rightNumber);
  if (leftNumber !== null) return -1;
  if (rightNumber !== null) return 1;
  return left.localeCompare(right);
}

export function compareSemver(left: string, right: string): number {
  const a = parseVersion(left);
  const b = parseVersion(right);
  for (let i = 0; i < Math.max(a.core.length, b.core.length); i += 1) {
    const diff = (a.core[i] ?? 0) - (b.core[i] ?? 0);
    if (diff !== 0) return Math.sign(diff);
  }
  if (a.pre.length === 0 && b.pre.length === 0) return 0;
  if (a.pre.length === 0) return 1;
  if (b.pre.length === 0) return -1;
  for (let i = 0; i < Math.max(a.pre.length, b.pre.length); i += 1) {
    if (a.pre[i] === undefined) return -1;
    if (b.pre[i] === undefined) return 1;
    const diff = compareIdentifiers(a.pre[i], b.pre[i]);
    if (diff !== 0) return diff;
  }
  return 0;
}

export function assetNameFromUrl(url: string): string {
  const parsed = new URL(url);
  const name = parsed.pathname.split("/").filter(Boolean).pop();
  if (!name) throw new Error("Platform URL does not include an asset name");
  return decodeURIComponent(name);
}

export function selectPlatform(manifest: LatestManifest, target: string, arch: string): PlatformInfo | null {
  return manifest.platforms[`${target}-${arch}`] ?? null;
}

function githubHeaders(env: Env, accept = "application/vnd.github+json"): HeadersInit {
  const headers: Record<string, string> = {
    Accept: accept,
    "User-Agent": USER_AGENT,
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (env.GITHUB_TOKEN) headers.Authorization = `Bearer ${env.GITHUB_TOKEN}`;
  return headers;
}

async function fetchWithCache(url: string, env: Env, accept?: string): Promise<Response> {
  const cache = caches.default;
  const request = new Request(url, { headers: githubHeaders(env, accept) });
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (!response.ok) return response;
  const cacheable = new Response(response.body, response);
  cacheable.headers.set("Cache-Control", `public, max-age=${CACHE_SECONDS}`);
  await cache.put(request, cacheable.clone());
  return cacheable;
}

async function latestRelease(env: Env): Promise<GitHubRelease> {
  const response = await fetchWithCache(`${API_BASE}/repos/${REPO}/releases/latest`, env);
  if (!response.ok) throw new Error(`GitHub latest release request failed: ${response.status}`);
  return response.json();
}

// browser_download_url is not subject to the REST API rate limit, which shared Worker IPs exhaust quickly.
async function fetchReleaseAsset(asset: GitHubAsset, env: Env): Promise<Response> {
  return fetchWithCache(asset.browser_download_url, env, "application/octet-stream");
}

async function latestManifest(release: GitHubRelease, env: Env): Promise<LatestManifest> {
  const asset = release.assets.find((item) => item.name === "latest.json");
  if (!asset) throw new Error("latest.json asset not found on latest release");
  const response = await fetchReleaseAsset(asset, env);
  if (!response.ok) throw new Error(`latest.json download failed: ${response.status}`);
  return response.json();
}

function json(data: unknown, status = 200): Response {
  return Response.json(data, {
    status,
    headers: { "Cache-Control": "no-store" },
  });
}

async function updateResponse(request: Request, env: Env, parts: string[]): Promise<Response> {
  const [target, arch, currentVersion] = parts.map(decodeURIComponent);
  if (!target || !arch || !currentVersion) return new Response("Not found", { status: 404 });

  const release = await latestRelease(env);
  const manifest = await latestManifest(release, env);
  if (compareSemver(manifest.version, currentVersion) <= 0) return new Response(null, { status: 204 });

  const platform = selectPlatform(manifest, target, arch);
  if (!platform) return new Response(null, { status: 204 });

  const assetName = assetNameFromUrl(platform.url);
  const downloadUrl = new URL(`/download/${encodeURIComponent(assetName)}`, request.url);
  return json({
    version: manifest.version,
    notes: manifest.notes ?? release.body ?? null,
    pub_date: manifest.pub_date ?? release.published_at ?? null,
    url: downloadUrl.toString(),
    signature: platform.signature,
  });
}

async function downloadResponse(assetName: string, env: Env): Promise<Response> {
  const release = await latestRelease(env);
  const asset = release.assets.find((item) => item.name === assetName);
  if (!asset) return new Response("Asset not found", { status: 404 });
  const response = await fetch(asset.browser_download_url, {
    headers: { "User-Agent": USER_AGENT },
    redirect: "follow",
  });
  if (!response.ok || !response.body)
    return new Response("Asset download failed", { status: response.status });
  const headers = new Headers({
    "Content-Type": response.headers.get("Content-Type") ?? "application/octet-stream",
    "Content-Disposition":
      response.headers.get("Content-Disposition") ?? `attachment; filename="${asset.name}"`,
    "Cache-Control": "private, max-age=60",
  });
  const contentLength = response.headers.get("Content-Length");
  if (contentLength) headers.set("Content-Length", contentLength);
  return new Response(response.body, { status: 200, headers });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (request.method !== "GET") return new Response("Method not allowed", { status: 405 });
      const parts = url.pathname.split("/").filter(Boolean);
      if (parts[0] === "download" && parts[1]) return await downloadResponse(decodeURIComponent(parts[1]), env);
      if (parts.length === 3) return await updateResponse(request, env, parts);
      return new Response("Not found", { status: 404 });
    } catch (error) {
      return new Response(error instanceof Error ? error.message : String(error), { status: 500 });
    }
  },
};
