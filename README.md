# Open Media Backup

Desktop app (macOS and Windows) that copies photos and videos from memory cards to permanent storage. It follows every file from card to SSD to NAS and checks each copy with a hash. Once a card's files are safe on enough **final** destinations, it offers to wipe the card.

## Concepts

| Term | Meaning |
| --- | --- |
| **Space** | A workspace such as *Travel*, *Home* or *Backup*. It holds sources, destinations, the flows between them, and its own variables. |
| **Project** | One shoot or trip, such as *Iceland 2026*. It supplies values for the space variables. |
| **Device** | A card, SSD, HDD, NAS or computer. Each device has a role: `original`, `temporary` or `final`. Its name is shared by every peer. |
| **Mapping** | Where a device lives on a particular computer, for example `/Volumes/NAS/photos` or `\\nas\photos`. |
| **Source / Destination** | A device plus a folder. Destination folders are templates such as `{backup_folder}/{project_name}`. |
| **Flow** | A connection from a source to a destination. Its line is green when transferred, orange when files are pending, red on an error, and grey when unavailable. |

* **Templates.** Built-in variables are `project`, `project_name`, `source_name`, `backup_folder`, `date`, `year`, `month` and `day`. Space variables come after these, and project values override space defaults.
* **Rules.** An ordered list of include/exclude rules, written as glob (`*.ARW`, `DCIM/`) or regex. They apply to both folders and files, and the last match wins. A pattern ending in `/` matches folders only.
* **Backup marker.** If a destination has *use backup marker* on, the backup folder name is read from `.openmediabackup/` on the card. When the card has no marker, the name is generated from the space's marker template and written to the card. Either way, every backup of the same card lands in the same folder.
* **Wipe.** A card can be wiped once each of its files is verified on at least *N* final destinations (*N* is set per project). Wiping either deletes the files or does a quick format.
* **Hashing.** Each space uses xxHash64 (fastest) or BLAKE3 (cryptographic). The verify mode is also set per space. *Re-read* (the default) reads every copy back and compares hashes. *Inline* hashes the bytes while copying, which is faster.

## Peer-to-peer sync

Each computer stores its catalog in SQLite. Changes go into an operation log stamped with a hybrid logical clock. Peers exchange the log over HTTP+JSON on port `47821`, authenticated by a shared token, and conflicts resolve per field by last-writer-wins.

The app has no discovery and no encryption of its own. It relies on a private overlay network such as [Netbird](https://netbird.io):

1. Install Netbird on every computer and join them to the same network.
2. On computer A, open **Device sync** (top right). Copy its address, which should be the Netbird IP, and its token.
3. On computer B, click **Add peer** and paste A's address and token. Then do the same from B to A.

A shared server (for example Supabase) is planned as an alternative backend.

## Thumbnails

* Embedded previews are pulled from JPEG and RAW files.
* Video thumbnails need `ffmpeg`. The app looks in `OMB_FFMPEG`, then `PATH`, then the Homebrew locations. Without ffmpeg, videos show a placeholder.

## Development

Requirements: Node 24+, Rust stable, and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/).

```sh
npm install
npm run dev          # UI only, in the browser, with a simulated backend (add ?mock inside Tauri)
npx tauri dev        # full desktop app
npm test             # frontend unit tests (vitest)
npm run test:rust    # backend tests
npm run lint && npm run typecheck
npx tauri build      # installers (.dmg / .msi / .exe)
```

### Layout

```
src/                     Lit + Tailwind 4 + daisyUI 5 frontend
  api/                   Backend interface: Tauri implementation + in-memory mock
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

## License

MIT
