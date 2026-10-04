# Open Media Backup

Desktop app (macOS and Windows) that copies photos and videos from memory cards to permanent storage. It follows every file from card to SSD to NAS and checks each copy with a hash. Once a card's files are safe on enough **final** destinations, it offers to wipe the card.

<img width="3204" height="2124" alt="CleanShot 2026-10-03 at 14 10 08@2x" src="https://github.com/user-attachments/assets/9de8f5bd-b0f2-44f1-a1ae-a512f4af3157" />


## Concepts

| Term                     | Meaning                                                                                                                                                     |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Space**                | A workspace such as _Travel_, _Home_ or _Backup_. It holds sources, destinations, the flows between them, and its own variables.                            |
| **Project**              | One shoot or trip, such as _Trip 2026_. It supplies values for the space variables.                                                                         |
| **Device**               | A card, SSD, HDD, NAS or computer. Each device has a role: `original`, `temporary` or `final`. Its name is shared by every peer.                            |
| **Mapping**              | Where a device lives on a particular computer, for example `/Volumes/NAS/photos` or `\\nas\photos`.                                                         |
| **Source / Destination** | A device plus a folder. Destination folders are templates such as `{backup_folder}/{project_name}`.                                                         |
| **Flow**                 | A connection from a source to a destination. Its line is green when transferred, orange when files are pending, red on an error, and grey when unavailable. |

- **Templates.** Built-in variables are `project`, `project_name`, `source_name`, `backup_folder`, `date`, `year`, `month` and `day`. Space variables come after these, and project values override space defaults.
- **Names.** The _Device name_ identifies physical hardware and is shared by every source/destination using it. Each source can override its _Device name for backup_: this supplies `{source_name}` and _Subfolder per source_, using the existing folder-name sanitization. Each source/destination also has a display-only _Task name_ for its card and transfer labels, including app destinations. Blank task names use the device/app name; blank backup names use the physical device name. Names sync between computers, but overrides belong to individual tasks, so tasks on the same device can be named differently. Changing a task name never changes paths. Changing a backup name affects future resolved paths (including source folders containing `{source_name}`) without moving existing files, changing recorded copy identities, or replacing existing backup-folder markers.
- **Device management.** Open **Settings > Devices** to add, edit, or remove any registered device, including offline devices and devices not assigned to a task. Edit the shared name, description, type, and role; choose a mounted volume or folder to locate it on the current computer, or register it offline and locate it later. Hardware identifiers and capacity are read-only. Removing a device clears its assignment from every source and destination across all spaces and removes its mappings on all computers. Tasks, connections, physical files, and backup history stay intact. Affected tasks show **No device selected** and cannot transfer or wipe until you choose a replacement in their settings. Device changes sync to peers.
- **Rules.** An ordered list of include/exclude rules, written as glob (`*.ARW`, `DCIM/`) or regex. Glob rules match case-insensitively. They apply to both folders and files, and the last match wins. A pattern ending in `/` matches folders only.
- **Backup marker.** If a destination has _use backup marker_ on, the backup folder name is read from `.openmediabackup/` on the card. When the card has no marker, the name is generated from the space's marker template and written to the card. Either way, every backup of the same card lands in the same folder.
- **Folder structure.** Each folder destination can preserve paths relative to the selected source folder or flatten files into its destination folder. With preservation on, selecting `DCIM` copies `DCIM/100MEDIA/photo.jpg` to `<destination>/100MEDIA/photo.jpg`, without adding another `DCIM` wrapper. With it off, the file goes to `<destination>/photo.jpg`. Destination templates and optional per-source-device subfolders still apply. Flattened files with the same name use the existing conflict choices. Empty folders and folders containing only excluded files are not copied.
- **Transfer preview.** The default folder view shows the planned destination hierarchy with file counts and sizes across all matching files, not just the first page. Expand folders to load their files; thumbnail and list views remain available. Ignored or unroutable files are grouped separately by source path, and app destinations show source folders. Paths are planned: existing-file conflicts can cause a numeric filename suffix during transfer.
- **Safe copies.** All sources assigned to the same device within a space share one safe-copy count. A backup device counts once, only when all files required by its destination rules across those sources have verified copies on it. Multiple folders or flows on the same backup device never add extra copies. Empty sources add no requirements; overlapping source folders do not duplicate files. Temporary backup devices contribute in the groups configured for the space; opted-in, confirmed app imports remain separate logical backup targets.
- **Wipe.** A card can be wiped once its shared device-level safe-copy count reaches _N_ (_N_ is set per project). An unresolved path in any source on that device blocks wiping for all of them. File counts and delete-files operations remain scoped to the selected source; quick format erases the whole volume. Final devices are never wiped, and temporary devices still need a complete final copy before wiping.
- **Hashing.** Each space uses BLAKE3 by default (cryptographic and still very fast) or xxHash64 (fastest). The verify mode is also set per space. _Re-read_ (the default) reads every copy back and compares hashes. _Inline_ hashes the bytes while copying, which is faster.
- **Check destination.** **Check**, beside Run/Retry on folder destinations, compares source and destination hashes at the expected, template-expanded paths, including files already marked transferred. It does not search other filenames or change media files. Results count matching, missing, different, and unreadable files and list paths needing attention; matching copies are recorded as verified, and demonstrably missing or changed copies no longer count as verified at that location. Both devices must be connected. Checks can be paused or cancelled; cancelled checks show partial results.
- **Existing-file conflicts.** Transfers also hash-check same-name destination files. Identical files are adopted without copying. Different files prompt for **Skip** (leave pending), **Keep both** (use a numbered filename), or **Replace** (verify a staged copy before replacing the existing file). **Apply to all remaining conflicts** affects only the current destination Run/Retry or Run all queue, never later runs. Cancelling the conflict dialog cancels that transfer.

### Spaces without projects

You can browse, preview, and transfer files before creating a project. Files without
a matching project stay unassigned and can use project-independent destination paths.
App destinations still require a manual import and confirmation.

Paths needing project values are blocked until those values are available. An existing
backup-folder marker can resolve `{backup_folder}` without a project; a new marker
can only be generated when its template resolves without invented project values.
Card wiping remains disabled without a project to define the required safe-copy count.

### Status scanning

Status routing reads capture-time metadata without loading RAW pixel payloads.
For videos it uses camera XML sidecars, then the MP4 creation header, and only
probes the codec when neither provides a capture date. Capture results are cached
for the current session and invalidated when file size, modification time, or
video sidecars change. Filesystem modification time is never used as a capture
date, and timestamps without a known timezone remain unassigned.

## Peer-to-peer sync

Each computer stores its catalog in SQLite. Changes go into an operation log stamped with a hybrid logical clock. Peers exchange the log over HTTP+JSON on port `47821`, authenticated by a shared token, and conflicts resolve per field by last-writer-wins.

The app has no discovery and no encryption of its own. It relies on a private overlay network such as [Netbird](https://netbird.io):

1. Install Netbird on every computer and join them to the same network.
2. On computer A, open **Device sync** (top right). Copy its address, which should be the Netbird IP, and its token.
3. On computer B, click **Add peer** and paste A's address and token. Then do the same from B to A.

A shared server (for example Supabase) is planned as an alternative backend.

## Thumbnails

- Embedded previews are pulled from JPEG and RAW files.
- Video thumbnails need `ffmpeg`. The app looks in `OMB_FFMPEG`, then `PATH`, then the Homebrew locations. Without ffmpeg, videos show a placeholder.
- FFmpeg must be able to start and decode the clip. Extraction failures are shown in the thumbnail's error tooltip. If `ffmpeg -version` fails with a missing-library error on macOS, repair the Homebrew installation (for example, `brew upgrade ffmpeg`) and reopen the media view after restarting the app. `OMB_FFMPEG` can point to an alternative working executable.

## Development

Requirements: Node 24+, Rust stable, and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/).

```sh
npm install
npx tauri dev        # full desktop app on your real catalog
npm test             # frontend unit tests (vitest)
npm run test:rust    # backend tests, incl. IPC end-to-end tests on real folders
npm run lint && npm run typecheck
npx tauri build      # installers (.dmg / .msi / .exe)
```

### Demo mode

Normal builds never contain sample data: the in-memory demo backend is only bundled when Vite runs in `demo` mode (`.env.demo` sets `VITE_OMB_DEMO=1`). Demo mode shows a "Demo data" badge and makes no changes on disk, so it's a safe way for people and LLM agents to try the UI.

```sh
npm run dev:demo     # UI in a browser at http://localhost:1420 with generic sample data
npm run tauri:demo   # desktop window with sample data (src-tauri/tauri.demo.conf.json)
npm run build:demo   # static demo build in dist/
```

Running `npm run dev` without demo mode in a plain browser only shows a hint to start the desktop app.

### Layout

```
src/                     Lit + Tailwind 4 + daisyUI 5 frontend
  api/                   Backend interface: Tauri implementation + in-memory demo backend (demo mode only)
  state/                 AppStore, selectors, derived stats, factories, dialog requests
  components/
    app/ top-bar/ workspace/ footer/   main layout (workspace holds the flow canvas)
    dialogs/             one file per dialog
    form/                template input, rules editor, device picker
    ui/                  icons, modal, thumbnail and other primitives
  utils/                 formatting and template helpers
src-tauri/src/           Rust core
  domain/                entities (space, project, device, flow, rule…)
  store/                 SQLite catalog, op-log, HLC
  sync/                  HTTP sync server, client and auto-sync
  devices/               volume detection, identification, markers, mappings
  plan/ rules/ scan/     path templates, rule matching, file scanning and planning
  transfer/ hashing/     copy + verify job engine
  thumbnails/ wipe/      previews, safe card wipe
  app/ commands/         application core and Tauri command layer
```

## Releases & auto-update

Open Media Backup uses `tauri-plugin-updater` and a Cloudflare Worker proxy. The app checks `https://update-open-media-backup.loicba.me/{{target}}/{{arch}}/{{current_version}}`, configured in `src-tauri/tauri.conf.json` under `plugins.updater.endpoints`; change that endpoint there if the update domain changes. HTTPS is required for secure updates. `tauri.conf.json` holds the updater public key. Never commit the private key.

To publish signed releases:

1. Generate an updater keypair:

   ```sh
   npx tauri signer generate -w ~/.tauri/omb-updater.key
   ```

2. Put the generated public key in `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.
3. Add GitHub repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The private key can be the contents of `~/.tauri/omb-updater.key`.
4. Deploy `workers/update-proxy` with Cloudflare Wrangler:

   ```sh
   cd workers/update-proxy
   npm install
   npx wrangler secret put GITHUB_TOKEN # optional, raises GitHub API limits
   npx wrangler deploy
   ```

5. `wrangler.toml` declares `update-open-media-backup.loicba.me` as the Worker's custom domain, so `wrangler deploy` provisions DNS and TLS for it. The `loicba.me` zone must be in the same Cloudflare account. It must serve both `/:target/:arch/:current_version` update checks and `/download/:assetName` downloads. Changing the app's endpoint does not provision DNS, TLS, or Worker routing.
6. Push a `vX.Y.Z` tag. `.github/workflows/release.yml` builds macOS universal and Windows bundles with `tauri-apps/tauri-action@v0`, signs updater artifacts from the secrets, and uploads `latest.json`.

`src-tauri/tauri.conf.json` leaves `bundle.createUpdaterArtifacts` disabled, so local `npx tauri build` runs and the CI bundle job produce unsigned installers without needing `TAURI_SIGNING_PRIVATE_KEY`. The release workflow merges `src-tauri/tauri.release.conf.json`, which enables updater artifacts, and signs them with the repository secrets. To build signed updater artifacts locally, export `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and run `npx tauri build --config src-tauri/tauri.release.conf.json`.

## License

MIT
