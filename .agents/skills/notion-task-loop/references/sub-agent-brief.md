# Sub-agent brief (paste into every implementation prompt)

Replace `<SCRATCH>` with the session `files/` directory, which must be outside the repo.

---

Repo: `<repo path>`. It's an in-place checkout on `main`. Do NOT create branches, stash, reset, rebase, or push.
Commit your work once it's validated, as one commit: `<type>(<scope>): <summary>`, then a blank line, then the trailer
`Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>`. Only commit files you changed for this task.

Read `AGENTS.md` and `.github/copilot-instructions.md` first and follow them.

**Stack:**
- Tauri 2, with the Rust code in `src-tauri/`.
- Lit light-DOM components in `src/components/**`.
- Tailwind v4 + daisyUI 5. Use daisyUI components such as btn, dropdown/menu, modal, badge, tooltip, join and select.
- vitest + happy-dom for tests.
- Demo mode runs on the mock backend in `src/api/mock` at http://localhost:5199. It's already running, so don't start or stop it. If your feature needs data to be visible in a screenshot, extend the mock.

**Design (UI tasks):** follow the approved design in `<design/proposals/x.png>` (open it with the view tool) and use proper daisyUI components. Mockups can contain small rendering glitches, so don't copy those. If you have to diverge from the design, update `design/open-media-backup.fig` with the OpenPencil tools: move the proposal frame into the Current app section, and refresh `design/preview-*.png`.

**Security:** only `AppSettings` stays local; everything else syncs to peers. Never accept raw filesystem or executable paths from the frontend. The backend resolves paths from stored config.

**Validation (all must pass):**
- `npm run typecheck && npm run lint && npx vitest run`
- If Rust changed: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && npm run test:rust`
- `npx prettier --write <changed files>`
- Add vitest tests (and Rust tests where relevant) for new logic. Put the logic in pure helpers so it can be tested.

**Screenshot check (mandatory for visual changes):**
1. Write a Playwright script in `<SCRATCH>/ombshot/` (playwright is installed there; `timeout` isn't available on macOS).
2. Open the demo at 1440x900 and capture every new state: default, hover/open, error, and empty.
3. Save the PNGs to `<SCRATCH>/shots/<task>-*.png`.
4. VIEW each one and fix any glitches.

Never save screenshots or scripts inside the repo.

**Final report (concise):** root cause (for bugs), files changed, commit hash, screenshot paths, and anything that needs manual verification on real hardware or user setup.
