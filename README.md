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
- **Rules.** An ordered list of include/exclude rules, written as glob (`*.ARW`, `DCIM/`) or regex. They apply to both folders and files, and the last match wins. A pattern ending in `/` matches folders only.
- **Backup marker.** If a destination has _use backup marker_ on, the backup folder name is read from `.openmediabackup/` on the card. When the card has no marker, the name is generated from the space's marker template and written to the card. Either way, every backup of the same card lands in the same folder.
- **Wipe.** A card can be wiped once each of its files is verified on at least _N_ final destinations (_N_ is set per project). Wiping either deletes the files or does a quick format.
- **Hashing.** Each space uses BLAKE3 by default (cryptographic and still very fast) or xxHash64 (fastest). The verify mode is also set per space. _Re-read_ (the default) reads every copy back and compares hashes. _Inline_ hashes the bytes while copying, which is faster.

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

Open Media Backup uses `tauri-plugin-updater` and a Cloudflare Worker proxy. The app checks `https://updates.openmediabackup.app/{{target}}/{{arch}}/{{current_version}}`, configured in `src-tauri/tauri.conf.json` under `plugins.updater.endpoints`; change that endpoint there if the update domain changes. The committed updater public key is a placeholder, and no private key should be committed.

To publish signed releases:

1. Generate an updater keypair:

   ```sh
   npx tauri signer generate -w ~/.tauri/omb-updater.key
   ```

2. Replace `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY` in `src-tauri/tauri.conf.json` with the generated public key.
3. Add GitHub repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The private key can be the contents of `~/.tauri/omb-updater.key`.
4. Deploy `workers/update-proxy` with Cloudflare Wrangler:

   ```sh
   cd workers/update-proxy
   npm install
   npx wrangler secret put GITHUB_TOKEN # optional, raises GitHub API limits
   npx wrangler deploy
   ```

5. Point `updates.openmediabackup.app` at the Worker.
6. Push a `vX.Y.Z` tag. `.github/workflows/release.yml` builds macOS universal and Windows bundles with `tauri-apps/tauri-action@v0`, signs updater artifacts from the secrets, and uploads `latest.json`.

`src-tauri/tauri.conf.json` keeps `bundle.createUpdaterArtifacts` enabled for releases. The regular CI bundle job intentionally passes a `--config` override that sets `createUpdaterArtifacts` to `false`, because CI does not have updater signing secrets and should still verify unsigned installers on pushes.

## License

MIT
