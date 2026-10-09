# Open Media Backup

Desktop app (macOS and Windows) that copies photos and videos from memory cards to permanent storage. It follows every file from card to SSD to NAS and checks each copy with a hash. Once a card's files are safe on enough **final** destinations, it offers to wipe the card.

<img width="3204" height="2124" alt="CleanShot 2026-10-03 at 14 10 08@2x" src="https://github.com/user-attachments/assets/9de8f5bd-b0f2-44f1-a1ae-a512f4af3157" />

## This project is completely free to use!

Help me keep the lights on by making a donation via "Buy me a coffee". Any amount is appreciated!

<a href="https://www.buymeacoffee.com/loicba" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me a Coffee" style="height: 60px !important;width: 217px !important;" ></a>

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
- **Name when formatting.** In the device editor, set a separate volume name for **Quick format (exFAT)**. Leave it blank to use the device's display name, as before. The formatter removes everything except ASCII letters, numbers, spaces, and underscores, trims whitespace, keeps the first 11 characters, and converts them to uppercase; if nothing remains it uses `MEDIA`. This setting persists and syncs with the device. Saving it does not rename or format the current volume, change backup paths or task names, or affect **Delete backed-up files**.
- **Device filters.** Use the filter button beside **Add source** or **Add destination** to show **All devices**, **Mounted devices**, or one registered device. Each column has an independent selection; mounted filtering uses each folder or source card's current availability status. Manual app destinations count as available for **Mounted devices** visibility and also stay visible under individual device filters, without changing their mount or connection status. Filtering only changes the view, not tasks, connections, or transfers. Choose **All devices** to reset; removing the selected device also resets that column's filter.
- **Destination free space.** Online folder destinations show available disk space when reported. NAS destinations omit this value because the current disk lookup can report the computer's disk instead of the NAS's actual free space. Availability and transfer status remain visible.
- **Destination paths.** Folder destination cards show a device-relative path from an actual transfer-planner route. Variable values are badges; hover or focus a badge to see its variable name and other resolved values across connected sources and matching projects. Routes and tooltip alternatives prefer sources currently visible under the device filters, in displayed order. Selecting a visible source prefers its route without mixing values from different routes; a selection hidden by a filter does not override visible sources. Hidden routes remain available as alternatives and as a fallback when no visible source has a resolved route. Completed routes remain visible; variables without a resolved route stay marked as unresolved rather than showing example values. Per-source subfolders are included, and existing card backup markers take precedence over generated folder names.
- **Rules.** An ordered list of include/exclude rules, written as glob (`*.ARW`, `DCIM/`) or regex. Glob rules match case-insensitively. They apply to both folders and files, and the last match wins. A pattern ending in `/` matches folders only.
- **Backup marker.** If a destination has _use backup marker_ on, the backup folder name is read from `.openmediabackup/` on the card. When the card has no marker, the name is generated from the space's marker template and written to the card. Either way, every backup of the same card lands in the same folder.
- **Folder structure.** Each folder destination can preserve paths relative to the selected source folder or flatten files into its destination folder. With preservation on, selecting `DCIM` copies `DCIM/100MEDIA/photo.jpg` to `<destination>/100MEDIA/photo.jpg`, without adding another `DCIM` wrapper. With it off, the file goes to `<destination>/photo.jpg`. Destination templates and optional per-source-device subfolders still apply. Flattened files with the same name use the existing conflict choices. Empty folders and folders containing only excluded files are not copied.
- **Transfer preview.** The default folder view shows the planned destination hierarchy with file counts and sizes across all matching files, not just the first page. Expand folders to load their files; thumbnail and list views remain available. Ignored or unroutable files are grouped separately by source path, and app destinations show source folders. While a source is offline its capture times cannot be read, so files are matched to projects through copies already recorded at a project's destination path (counted as done); the rest show as ignored until the source is connected. Paths are planned: existing-file conflicts can cause a numeric filename suffix during transfer.
- **Safe copies.** All sources assigned to the same device within a space share one safe-copy count: the minimum effective copies across non-excluded files. Each file must be covered by enough distinct eligible backup devices, but different files may be backed up on different devices. Multiple folders or flows on the same device never add extra copies. Empty sources add no requirements; overlapping source folders do not duplicate files. Temporary backup devices contribute in the groups configured for the space; opted-in, confirmed app imports remain separate logical backup targets. Click or keyboard-activate the colored inline badge to inspect device-relative files, verified destinations, and actionable missing-coverage reasons under the **space** policy. Excluded files do not lower the completed count; when all known files are excluded, the requirement is satisfied without claiming physical backup evidence. An empty inventory shows zero copies. When the space enables deliberate Skip acknowledgements, those count separately and are explicitly **not byte-verified**. Recorded verified copies remain valid while their destinations are offline.
- **Safe-copy exclusions.** In the safe-copy dialog, use the existing ordered file-rule filters to exclude files or folders from this source's safety requirements (for example `PRIVATE/` or `*.THM`). Rules are relative to the source folder, persist in the synced source configuration, and can only be edited for devices mapped on the current computer. Transfers are unchanged. A physical file still required by an overlapping source task remains required. Exclusions affect the shared card count and wipe readiness; an entirely excluded source can meet readiness without copies. **Delete backed-up files** preserves excluded files, while **Quick format** still erases the whole volume.
- **Wipe.** Set **Required effective copies before wiping** in **Space settings**; projects never own or override this threshold. Existing spaces migrate to their highest previous project requirement, including archived projects (default 2 if none exists); an already-set space policy is preserved. Migration emits normal synced field-clock operations, so peers receive the policy, and later edits are not overwritten on restart. Every non-excluded file must meet this threshold and have verified identity and eligible rule coverage. **Wipe card** appears only when the source device is mounted and **Offer to wipe once safe** is enabled. Disabling that option also hides wipe-state messages without hiding copy evidence, policy details, or transfer facts. Wipe assessment uses space defaults, not project overrides; an unresolved path in any source on that device blocks wiping for all of them. File counts and delete-files operations remain scoped to the selected source; quick format erases the whole volume. Final devices are never wiped, and every required file on a temporary device still needs a final destination copy before wiping.
- **Manually wiped.** If you formatted a card or temporary device outside the app and its previous files are already safely backed up, right-click a source card and choose **Mark as manually wiped...**, then confirm. This retires all previous copy records on that device across every source, space, and project, including when it is offline or no project exists. The change persists and syncs to peers; existing backups on other devices remain intact. Safe-copy coverage is recalculated from zero for new contents, including reused filenames. Files still present on the device are treated as new, not hidden or assumed backed up. No physical files are deleted and no destination copies are fabricated. Finish or cancel jobs and finish pending app imports on the device first. Final devices cannot be marked manually wiped.
- **Job queue.** Up to three transfer, check, or wipe jobs run at once, including jobs sharing a source or destination device when their destination folders do not overlap. Transfers targeting the same destination folder are serialized, and a wipe waits for every other job using that device.
- **Keep awake.** **Settings > General > Keep computer awake during transfers** is enabled by default and saved separately on each computer. On macOS and Windows, it prevents automatic system sleep while transfers are running, including copy verification; the screen can still turn off. Protection is released when no transfer is active, including while all transfers are paused or waiting for conflict decisions, and restored when they resume. Changing the setting takes effect immediately. Standalone checks, wipes, peer sync, and external-app imports do not keep the computer awake. If sleep prevention fails, a warning appears and transfers continue. This does not prevent manual sleep, lid-close sleep, shutdown, device removal, or network disconnections.
- **Hashing.** Each space uses BLAKE3 by default (cryptographic and still very fast) or xxHash64 (fastest). The verify mode is also set per space. _Re-read_ (the default) reads every copy back and compares hashes. _Inline_ hashes the bytes while copying, which is faster.
- **Check destination.** **Check**, beside Run/Retry on folder destinations, compares source and destination hashes at the expected, template-expanded paths, including files already marked transferred. It does not search other filenames or change media files. Progress advances during both reads, including within large files; the percentage is based on source file sizes, not the combined bytes read from both devices. Results count matching, missing, different, and unreadable files and list paths needing attention; matching copies are recorded as verified, and demonstrably missing or changed copies no longer count as verified at that location. The destination device must be connected; offline source devices are skipped and listed in a job warning. **Scan all files in the destination** instead checks every file in the destination's existing folders, resolved from the database: the template expanded for each project, plus values that only a source card knows (such as `{backup_folder}`) taken from folders already recorded in the catalog or from connected cards' markers. Folders that do not exist are skipped, and unrelated folders on the same device are not scanned. Checks can be paused or cancelled; cancelled checks show partial results.
- **Speed analysis.** **Settings > Speed Analysis** compares source-to-destination pairs across spaces and projects, with live jobs, date filters, recorded averages, and individual job details. Average copy speed is total physical writes divided by active copy time (including retries); copied data counts only committed files, not adopted or skipped files. Inline source hashing is included in copy time. **Source checks** and **destination re-reads** measure bytes read by this computer, including SMB/NFS traffic. **NAS-side hash checks** measure server-hashed bytes and request time separately; those bytes never travel back to the desktop. Planning/finalization, queueing, pauses and conflict decisions are timed separately. Effective transfer throughput includes active transfer checks and other work, but excludes waiting and standalone check jobs. Live, failed, cancelled and interrupted jobs do not enter completed-job averages; their partial metrics remain inspectable. Durations are summed job time, not unique elapsed time when jobs overlap. History starts with this feature (old speed estimates cannot be backfilled), is kept on this computer without automatic deletion, survives restarts and clearing recent jobs, and is never synced to peers. An interrupted job shows its last durable checkpoint, not app downtime.
- **Existing-file conflicts.** Transfers also hash-check same-name destination files. Identical files are adopted without copying. Different files prompt for **Skip** (leave pending), **Keep both** (use a numbered filename), or **Replace** (verify a staged copy before replacing the existing file). **Apply to all remaining conflicts** affects only the current destination Run/Retry or Run all queue, never later runs. Cancelling the conflict dialog cancels that transfer.

### Spaces without projects

New projects get a random default from the project color palette, excluding the
colors of the last three projects in the same space (including archived projects).
Recency uses the tail of the snapshot's project order, where newly created projects
are appended—not the management page's alphabetical/archived display sorting.
Projects do not store creation timestamps, so restored/synced snapshots use their
existing order. An exhausted palette falls back to a generated hex color distinct
from those three. Existing colors and manually selected swatches are preserved.

You can browse, preview, and transfer files before creating a project. Files without
a matching project stay unassigned and can use project-independent destination paths.
App destinations still require a manual import and confirmation. **Mark as transferred**
queues a background import-confirmation job and closes the dialog after enqueueing.
Follow hashing and file progress in **Active jobs**, where you can pause, resume, or
cancel it. Completed files are recorded as the job progresses; cancellation or a
failure preserves those completed records without marking unfinished files.
Completion, cancellation, and failures produce a notification. These jobs do not
copy media or contribute to learned transfer speeds.

Paths needing project values are blocked until those values are available. An existing
backup-folder marker can resolve `{backup_folder}` without a project; a new marker
can only be generated when its template resolves without invented project values.
Card wiping uses the space's copy requirement, even without a project, provided
source paths and safety rules resolve and all required files are safe.

### Status scanning

Status routing reads capture-time metadata without loading RAW pixel payloads.
For videos it uses camera XML sidecars, then the MP4 creation header, and only
probes the codec when neither provides a capture date. Capture results are cached
for the current session and invalidated when file size, modification time, or
video sidecars change. Filesystem modification time is never used as a capture
date, and timestamps without a known timezone remain unassigned.

For project routing, non-media sidecars inherit capture times from media in the
same directory when their filename contains the media filename stem (without
its extension), case-insensitively. For example, `C7000M01.XML` follows
`C7000.MP4`. If several media names match, their capture times are combined so
the sidecar follows every matching project. Explicit sidecar capture times are
preserved.

## Peer-to-peer sync

Each computer stores its catalog in SQLite. Changes go into an operation log stamped with a hybrid logical clock. Peers exchange the log over HTTP+JSON on port `47821`, authenticated by a shared token, and conflicts resolve per field by last-writer-wins.

The app has no discovery and no encryption of its own. It relies on a private overlay network such as [Netbird](https://netbird.io):

1. Install Netbird on every computer and join them to the same network.
2. On computer A, open **Device sync** (top right). Copy its address, which should be the Netbird IP, and its token.
3. On computer B, click **Add peer** and paste A's address and token. Then do the same from B to A.

A shared server (for example Supabase) is planned as an alternative backend.

### Connecting a NAS on macOS

Offline NAS folder destination cards offer **Connect drive** on macOS. The app
opens the remembered SMB share with macOS; complete any system sign-in prompt.
The destination stays offline until the share and destination folder are available.
Authentication remains with macOS/Keychain; the app never stores NAS credentials.

To initialize an existing NAS, connect its SMB share once in Finder while the app
is running, and ensure the device is registered or linked on this Mac. The backend
remembers its credential-free share address in local AppSettings, not peer sync.
If no share has been remembered, Connect drive explains this setup requirement.
Other destinations and platforms do not show the button. AFP/NFS reconnection
is not currently supported.

## Remote hash server

Destination **NAS hash** badges are blue after a successful connection check and
red when the configured server is missing, has no successful connection, or its
latest check failed. Hover or focus the badge for the recorded connection state
and failure details. The badge does not guarantee remote verification; remote
hash failures still fall back to local re-reads.

`omb-hash-server` is a small, read-only NAS service. The desktop still copies files
over SMB/NFS; the server reads destination files locally and sends back only the
digest, size, and modification time. It has no Tauri runtime or SQLite catalog.

1. Edit `docker/hash-server/compose.yml`: replace `/volume1/photos` with your NAS
   share and set `OMB_HASH_BIND_IP` to a trusted LAN or Netbird IP (the default is
   loopback only). Mount every data share **read-only**. Start it with:

   ```sh
   OMB_HASH_BIND_IP=100.1.2.3 docker compose -f docker/hash-server/compose.yml up -d
   docker compose -f docker/hash-server/compose.yml logs hash-server
   ```

2. The startup logs show a pairing token and stable server id. Treat the logs as
   secret. The generated token and id persist in the `/config` volume; keep that
   volume when upgrading so synced mappings continue to match. The service runs
   as UID/GID 65532 and needs read/traverse permission on shares and write
   permission on `/config` only. Synology shared folders grant access only
   through DSM ACLs, so UID 65532 gets `Permission denied` at startup; on
   Synology, uncomment `user: "0:0"` and `cap_add: [DAC_READ_SEARCH]` in the
   Compose file (media mounts stay read-only).
3. In **Device sync > Hash servers**, add the NAS address (`100.1.2.3:47822`) and
   token. Registration authenticates `/v1/hello` and rejects ordinary catalog
   peers. Use **Test** to refresh reachability; **Paired** means a saved pairing,
   not a continuously monitored health check.
4. In a folder destination's settings, enable **Remote hash check**, select the
   server and its exposed folder, and optionally browse subfolders. The selected
   folder must correspond to the **device root**, not the destination's template.
   For example, if the local device mapping is `/Volumes/photos` and the container
   mount is `/data/photos`, select `/data/photos`, even if the destination template
   is `Archive/{project_name}`. Save and reopen to use **Test mapping**.
5. Every request is logged on one line, without tokens, for example
   `2026-01-02T03:04:05Z 100.1.2.4 POST /v1/hash 200 412ms hash Xxh64 backup_media/2026/A.MP4 71000000 bytes`.
   Unauthorized requests appear as `401`. Follow them with
   `docker compose -f docker/hash-server/compose.yml logs -f hash-server`; the
   Compose file rotates the log at 10 MB.
6. Test mapping compares one catalog-known file (or a backup marker) locally and
   remotely. If no such file exists, it reports only folder existence and
   explicitly warns that the mapping is not proven. It never creates probe files.

Only `{ server_id, root, enabled }` is catalog-synced. Addresses, tokens, and server
status stay in a separate local table; add the same server on each computer.
An unknown synced server shows **not available on this computer** and uses local
checks. Removing a pairing does not delete files or remove synced mappings.

Remote checks cover post-copy **Re-read** verification (after the `.omb-partial`
handle is flushed and closed), existing-file comparisons and replacement rechecks,
and **Check destination**. A full **Check destination** lists the destination's
existing folders through the server (`/v1/roots/{id}/list`) instead of walking it over
SMB, so large shares start hashing quickly and the job shows scan progress;
older servers without that endpoint fall back to a local walk. **Inline** copy verification is unchanged. A server
failure, missing remote file, wrong size, invalid digest, or changing local file
falls back to the normal local re-read, with a persistent job warning and a toast;
it never counts an unsuccessful request as verification. Speed Analysis separates
destination network re-reads from NAS-side hashed bytes and request time. Pause
holds desktop progress; cancel drops pending requests, and server workers check a
cancellation flag between chunks. Up to two server operations run concurrently;
additional requests return HTTP 429 and desktop checks safely fall back.

Configuration:

| Environment variable | Default / meaning                                                                                                                                          |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OMB_HASH_PORT`      | `47822`                                                                                                                                                    |
| `OMB_HASH_TOKEN`     | Otherwise generate and persist `/config/token`                                                                                                             |
| `OMB_HASH_NAME`      | `NAS hash server`                                                                                                                                          |
| `OMB_HASH_ROOTS`     | Optional JSON array of absolute server folders, e.g. `["/data/photos","/data/video"]`; otherwise expose top-level directories under `/data` (not symlinks) |
| `OMB_HASH_CONFIG`    | `/config`; override for native development                                                                                                                 |

Exposed root ids are stable hashes of canonical container paths. Keep those mount
paths stable across upgrades. A mapping's root is the exposed id, optionally
followed by a relative subfolder. `/v1/hash` accepts that root, a relative path,
and `blake3` or `xxh64`; traversal, absolute paths, and symlink escapes are
rejected. Capability-scoped file opens also guard against symlink-swap races.
No endpoint writes or deletes data files. Only startup pairing configuration is
written, under `/config`.

**Security:** all endpoints require a bearer token, compared in constant time.
HTTP is **unencrypted**: use a trusted LAN or Netbird, restrict port 47822 with
your firewall, and never publish it directly to the internet. Anyone holding the
token can hash or list exposed data. SMB permissions and server share permissions
must identify the same data; equal sizes alone do not prove a correct mapping.

Build locally without desktop dependencies:

```sh
cargo build --locked --release --manifest-path src-tauri/Cargo.toml -p omb-hash-server
cargo test --manifest-path src-tauri/Cargo.toml -p omb-hash -p omb-hash-server
docker build -f docker/hash-server/Dockerfile .
```

The release workflow builds a non-root musl/scratch image for Linux amd64 and
arm64 and publishes `ghcr.io/loic294/omb-hash-server:<version>` and `:latest` for
tested main builds and tagged releases, before publishing the desktop release.
GitHub Container Registry is the only publishing destination; nothing is pushed
to Docker Hub. Docker actions and downloads of build tooling/base images are
build dependencies, not publishing destinations.
CI tests the isolated server and builds the image. In browser demo
mode, use `demo-token` to try the dialogs; pairing is simulated and **Test mapping**
explicitly reports that no real files were hashed.

## Thumbnails

- Embedded previews are pulled from JPEG and RAW files.
- Videos first use the thumbnail the camera already saved on the card, which is read-only and never copied. Defaults cover Sony (`PRIVATE/M4ROOT/THMBNL/<clip>T01.JPG`), DJI (`MISC/THM/<folder>/<clip>.SCR` or `.THM`), and cameras that store `<clip>.THM` beside the clip, such as GoPro and Canon. Edit the list in **Settings > General > Camera thumbnails**. Paths are relative to the video's folder, with `{stem}` (file name without extension) and `{folder}` (the video's folder name); the first existing JPEG wins. Settings apply per computer.
- Otherwise, video thumbnails need `ffmpeg`. The app looks in `OMB_FFMPEG`, then `PATH`, then the Homebrew locations. Without ffmpeg, videos show a placeholder.
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

### Keeping Rust/Tauri build output small

Cargo keeps generated artifacts in `src-tauri/target` (or `CARGO_TARGET_DIR` if
set), with no size cap. Incremental state and old debug/test binaries can grow
to hundreds of GB after repeated builds. This is separate from the app's catalog
and media.

For routine local development and validation on macOS/Linux, set these once in
the shell used for builds and tests:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=1
export CARGO_PROFILE_TEST_DEBUG=1
du -sh src-tauri/target
df -h .
```

On PowerShell, use `$env:CARGO_INCREMENTAL="0"`,
`$env:CARGO_PROFILE_DEV_DEBUG="1"` and `$env:CARGO_PROFILE_TEST_DEBUG="1"`.
These settings trade incremental rebuild speed and full debug information for
less disk usage; release settings and optimized dependencies stay unchanged.
Reuse the same settings and target directory instead of creating per-task caches.
They reduce growth, but do not evict stale artifacts or enforce a size cap.

Check cache size and free space before and after Rust/Tauri builds. At 20 GiB of
build output or below 10 GiB of free space, clean disposable artifacts before
another build. First stop debug builds, tests, and `tauri dev`, then run from the
repository root:

```sh
cargo clean --manifest-path src-tauri/Cargo.toml --profile dev
```

This clears debug/test output and preserves release bundles. The next debug
build recompiles dependencies. Omit `--profile dev` only if release bundles and
installers are also disposable and no app is running from that directory.
Never clear the app's data or media to reclaim build space. Verify disk usage
afterward; Cargo's reported removed bytes can exceed space actually reclaimed
because of hard-linked artifacts.

### Before committing or pushing

Use the same desktop preflight as the macOS and Windows CI jobs:

```sh
npm ci
rustup component add clippy rustfmt
npm run ci:local
```

The preflight runs release/tooling regression tests, TypeScript checking, ESLint,
Vitest, the production frontend build, workspace Rust formatting checks, Clippy
on all workspace targets with warnings denied, and locked workspace Rust tests.
It stops at the first failure. Fix it and rerun the preflight; do not rely on
`npm test` alone. It does not format files or update lockfiles automatically.
Use `cargo fmt --manifest-path src-tauri/Cargo.toml --all` to fix Rust formatting.

Both `npm run ci:local` and `npm run test:rust` use the Windows test runner on
Windows x64 MSVC. It embeds the common-controls manifest using Windows SDK
`mt.exe` and removes stale API-set DLL directories from the child PATH. Install
the Tauri Windows prerequisites, including the Windows SDK. The launcher passes
an **absolute** PowerShell script path: Cargo runs workspace-member tests in
their package directories, so `.cargo/run-test.ps1` relative to the workspace
does not work. The tooling tests check path escaping/spaces and exercise Cargo
runner resolution from a workspace member on the host OS. To invoke Rust tests
from another directory, use `node /absolute/path/to/open-media-backup/.github/scripts/local-ci.mjs --rust-only`.
The runner is test-only and does not affect `cargo run` or desktop launches.

A green macOS preflight is **not** a Windows validation (or vice versa). Run this
preflight on both platforms, especially after runner, dependency, filesystem or
platform-specific changes; the Windows run must execute tests for `omb-hash`,
`omb-hash-server` and the desktop crate, not merely compile them. CI remains the
cross-platform gate: local tests cannot guarantee another OS, SDK or runner image
will pass. The separate Linux hash-server job also runs
`cargo test --locked --manifest-path src-tauri/Cargo.toml -p omb-hash -p omb-hash-server`
and `docker build -f docker/hash-server/Dockerfile .`; run those when changing the
server/container. Signed releases, universal builds and installers have separate
prerequisites and are not covered by this desktop preflight.

### Demo mode

Normal builds never contain sample data: the in-memory demo backend is only bundled when Vite runs in `demo` mode (`.env.demo` sets `VITE_OMB_DEMO=1`). Demo mode shows a "Demo data" badge and makes no changes on disk, so it's a safe way for people and LLM agents to try the UI.

```sh
npm run dev:demo     # UI in a browser at http://localhost:1420 with generic sample data
npm run tauri:demo   # desktop window with sample data (src-tauri/tauri.demo.conf.json)
npm run build:demo   # static demo build in dist/
```

Running `npm run dev` without demo mode in a plain browser only shows a hint to start the desktop app.

### Window header

The 64px app header is a Tauri deep drag region: its background, branding, icons,
and non-interactive status text can move the window. Buttons, links, inputs, and
other interactive descendants remain clickable, and double-click behavior is
handled by Tauri for each OS. Do not add self-only drag markers to descendants:
they prevent nested content from reaching the header's deep region.

On macOS, both normal and demo window configurations use a traffic-light inset
of `(20, 30)` logical pixels. AppKit's button frame is 16px tall with a 6px
bottom offset; Tauri's inset positions its container, so the button center is
`30 + 16 / 2 - 6 = 32`, aligned with the header. Windows uses the existing
vertically centered custom window buttons; Linux retains its native decorations.
Browser demo previews (`?ombChrome=macos`, `windows`, or `linux`) show layout only.
Verify actual dragging, native button alignment, double-clicking, and
fullscreen/restore on desktop hardware, including Retina/scaled displays.

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
6. Push to `main`. Once the complete macOS/Windows CI test matrix passes, CI calls `.github/workflows/release.yml` to build signed macOS universal and Windows bundles for the exact tested commit. Each build gets a stable `v<major>.<minor>.<CI run number>` release (for example `v0.1.19`); major/minor come from `src-tauri/tauri.conf.json`. Failed runs may leave version gaps. The generated version is embedded in installers and `latest.json` without committing version bumps to the repository. Pull requests never publish releases.

The release stays a draft until both platform builds finish and the workflow verifies the expected updater version, macOS Intel/Apple Silicon and Windows x64 entries, installer assets, and matching signatures. Platform uploads run sequentially to avoid overwriting each other's `latest.json`. Failed builds or validation leave the draft unpublished. Reruns reuse the same version and matching draft; already published releases are validated and left untouched. GitHub's version-aware latest selection keeps older reruns from replacing newer updates; the updater proxy caches GitHub responses for approximately five minutes.

Explicit `vX.Y.Z` tag pushes still publish signed stable releases, with the tag's version embedded in the build. Avoid tags reserved by automatic CI numbering: an existing tag/release for another commit causes an explicit failure rather than replacing its assets. Versions must fit Windows MSI limits (`255.255.65535`); advance the checked-in major/minor before CI numbering would collide with a manually tagged patch version.

`src-tauri/tauri.conf.json` leaves `bundle.createUpdaterArtifacts` disabled, so local `npx tauri build` runs do not need `TAURI_SIGNING_PRIVATE_KEY`. macOS bundles use ad-hoc code signing (`bundle.macOS.signingIdentity: "-"`) to seal the complete app, including its bundle identity. This is separate from updater signing and does not provide Developer ID signing or notarization. Leaving only the linker's executable signature can cause macOS to reject saved removable-volume permissions and repeatedly prompt even after **Allow**. Ad-hoc signatures can still require permission again after rebuilding or updating; Developer ID signing is needed for a stable trusted identity across versions.

If an older build loops on the removable-volume prompt, quit all running copies, rebuild with this configuration, and reopen just the new bundle before granting access. Do not overwrite an app bundle while that copy is running. Check the new bundle with `codesign --verify --deep --strict --verbose=2 "/path/to/Open Media Backup.app"`.

The release workflow merges `src-tauri/tauri.release.conf.json`, which enables updater artifacts, and signs them with the repository secrets. To build signed updater artifacts locally, export `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and run `npx tauri build --config src-tauri/tauri.release.conf.json`.

Run the release automation regression tests with `node --test .github/scripts/release.test.mjs`.

## License

MIT
