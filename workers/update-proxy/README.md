# Open Media Backup update proxy

Cloudflare Worker that hides GitHub release URLs from Tauri clients and serves the dynamic updater response expected by `tauri-plugin-updater`.

The app uses `https://update-open-media-backup.loicba.me/:target/:arch/:current_version`. After deploying, configure that custom domain with HTTPS in Cloudflare so both routes below reach this Worker; deployment alone does not configure the custom domain.

## Routes

- `GET /:target/:arch/:current_version` checks the latest `loic294/open-media-backup` GitHub release. It returns `204` when the latest release is not newer or has no `${target}-${arch}` platform, otherwise it returns `{ version, notes, pub_date, url, signature }`.
- `GET /download/:assetName` streams the matching release asset through the Worker.

GitHub API responses are cached in the Worker Cache API for about five minutes.

## Development

```sh
npm install
npx vitest run
npx wrangler dev
```

Set an optional `GITHUB_TOKEN` secret to raise GitHub API limits:

```sh
npx wrangler secret put GITHUB_TOKEN
```

Deploy with:

```sh
npx wrangler deploy
```
