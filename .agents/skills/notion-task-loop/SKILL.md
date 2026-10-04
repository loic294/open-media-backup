---
name: notion-task-loop
description: Work through the "Open Media Backup - AI Tasks" Notion database in a manager loop. Pick up every task that isn't Done, implement each one through sub-agents, keep each Notion Status and implementation note up to date, and re-query Notion until no actionable tasks remain. Use when the user asks to process, run, work through, or loop over the Notion AI tasks, or to "start the task loop" or "deploy the fleet" for Open Media Backup.
metadata:
  version: "1.0.0"
---

# Notion task loop (Open Media Backup)

You are the **manager**. You do not write feature code yourself. You pick Notion tasks, dispatch sub-agents to implement them, review the results, and keep Notion up to date. Once started, keep looping until a fresh Notion query returns no actionable tasks. Don't stop after one batch, and don't wait for the user to say "continue".

## Constants

- Notion data source: `collection://3ee16ff3-43e7-80be-9aad-000b53c999c3` (database "Open Media Backup - AI Tasks").
- Status values: `Not started` → `In progress` → `Done`.
- Repo: the current checkout, branch `main`. Commit locally only. Never push, create branches, stash, reset, or rebase.
- Commit trailer: `Co-authored-by: Copilot App <223556219+Copilot@users.noreply.github.com>`.
- Sub-agent brief: [references/sub-agent-brief.md](references/sub-agent-brief.md). Paste its contents, with placeholders filled in, into every implementation prompt.
- Scratch space: the session `files/` directory, for screenshots, Playwright scripts and notes. Never write scratch files into the repo.

Notion tools are deferred. Load them first with `tool_search_tool` (pattern `notion-(query-data-sources|fetch|update-page|create-comment)`).

## Loop

```
setup → query → triage → for each task: start → implement → review → finish → query again → … → wrap up
```

### 1. Setup (once)

1. Read `AGENTS.md` and `.github/copilot-instructions.md`. Their rules always win over this skill.
2. Run `git status --short` and note any pre-existing untracked files so you never commit them.
3. Start the demo server for screenshots if it isn't already running: `npx vite --mode demo --port 5199 --strictPort` (async bash, shellId `demo`). Check it with `curl -s -o /dev/null -w "%{http_code}" http://localhost:5199`.
4. Make sure Playwright with Chromium is available in a scratch directory outside the repo, e.g. `npm i playwright` in the session `files/ombshot`, then `npx playwright install chromium-headless-shell`.

### 2. Query

```sql
SELECT url, Name, Status FROM "collection://3ee16ff3-43e7-80be-9aad-000b53c999c3"
WHERE Status IS NULL OR Status != 'Done'
```

The query index can lag a few seconds behind updates. Before acting on a row you just changed, `notion-fetch` the page to confirm its real Status.

### 3. Triage

`notion-fetch` every returned page and read the full body. Existing "**Implementation (AI)**" notes show what was already done. For each task, record a row in the session SQL `todos` table with id, title, Notion page id, and type:

- **Backend / non-visual**: logic, Rust, tests, bug fixes with no visual change. Implement directly.
- **UI / visual**: anything that changes what the user sees. This needs an approved design first (step 4b).
- **Mixed**: do the backend part first, then treat the rest as UI.
- **Blocked**: needs a decision, credentials, or external setup only the user can provide.

Order: fix bugs first, then backend, then UI. When tasks touch the same files, do them one after another.

### 4. Per task

**a. Start.** Set Notion `Status` to `In progress`, and mark the SQL todo `in_progress`.

**b. Design gate (UI tasks only).** Follow the design-review workflow in `.github/copilot-instructions.md`:
1. A sub-agent prototypes the change in `design/open-media-backup.fig` using the OpenPencil tools: a frame labelled `Proposal: <task>` on the Designs page, built from Components instances. It exports `design/proposals/<slug>.png`.
2. Batch proposals: design every pending UI task, then view each PNG yourself and present them all in one `ask_user` call. Offer the choices "Approve all", "Approve all except some" and "Reject all", and keep asking until every proposal has a clear answer.
3. Only approved proposals move on to implementation. Record any requested changes in that task's prompt. Rejected or revised designs go back to step 1.
4. While waiting for approval, keep working on backend tasks. Never treat "continue" as design approval.

**c. Implement.** Dispatch one sync `general-purpose` sub-agent per task. In the prompt include:
- the brief from `references/sub-agent-brief.md`, with placeholders filled in;
- the Notion title and full body, quoted verbatim;
- the approved design PNG path and any user feedback;
- the relevant files or commits from earlier tasks or notes;
- clear acceptance criteria, the required tests, and screenshot names.

Run tasks one at a time, because they share one checkout and parallel edits conflict.

**d. Review.** Never mark a task Done based only on the sub-agent's report:
- `view` every screenshot. Compare against the design and look for glitches such as clipping, overlap, wrong theme, or a missing state.
- Run `git status --short` and check for stray files in the repo, such as screenshots or scratch scripts. Move them out, or delete them.
- Run `git show --stat HEAD` to check the scope. Read the diff of any shared component it changed for regressions.
- If something is wrong, fix it with a small follow-up sub-agent, or a direct edit if it's trivial, and commit.

**e. Finish.** Set Notion `Status` to `Done`, then append a note with `notion-update-page` (`insert_content`):

```
**Implementation (AI) — done (commit <sha>):** <2–4 sentences: what changed, where to see it, any manual
verification or user setup still needed>
```

Then mark the SQL todo `done`.

**f. Stuck or blocked.** If a sub-agent fails twice on a task, or the task needs something only the user can provide:
1. Leave the task `In progress` and add a note starting `**Blocked (AI):**` that says exactly what's needed.
2. Mark the SQL todo `blocked`, and move on to the next task.
3. Retry blocked tasks after the others are finished. If they still need the user, ask with one `ask_user` question per blocker.

### 5. Re-query (the loop)

After the last task in the current batch, run the query again (step 2):
- New rows, or rows that went back from Done → go back to step 3 with only those rows.
- Rows you just finished that still show as not Done → `notion-fetch` them to confirm. If they really aren't Done, fix the Status.
- Only Done rows, or only rows blocked on the user that you've already asked about → wrap up.

Keep looping. Users often add tasks while the loop is running.

### 6. Wrap up

1. Stop the `demo` server, if you started it, and delete scratch Playwright folders.
2. Run `git status --short`. No new untracked files from this run should remain in the repo.
3. Give the user a short summary: the tasks completed with their commit SHAs, anything blocked, and any manual checks or setup they still need to do.

## Rules

- One Notion task in progress at a time per checkout, unless the work is backend-only and touches separate files.
- Keep Notion accurate at every transition. It is the user's dashboard.
- UI changes always go through the design gate, and the `.fig` file must stay in sync with what ships (proposal → Current app section, refreshed `design/preview-*.png`).
- Security: only `AppSettings` stays local; everything else syncs to peers. Never accept raw filesystem or executable paths from the frontend.
- Tests: the sub-agent must make `npm run typecheck && npm run lint && npx vitest run` pass. If Rust changed, also `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `npm run test:rust`.
- One commit per task, `feat(ui): …`, `feat: …`, or `fix: …`, with the trailer.
- If context gets long, summarize finished tasks into a short list of Notion page ID, commit, and status rather than dropping the loop.
