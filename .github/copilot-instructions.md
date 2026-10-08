# Completing changes

After completing and validating changes, always commit the task's changes and push
the commit to the current branch's remote. Do not include unrelated changes in the
commit. If committing or pushing is blocked, report the blocker explicitly.

# Rust/Tauri build-cache hygiene

Rust build artifacts in `src-tauri/target` can grow without a size limit,
especially incremental state and stale debug/test binaries. They are generated
build output, not the app's catalog, media, or runtime cache.

- Before and after local Rust/Tauri builds or tests, check
  `du -sh src-tauri/target` and available disk space (`df -h .` on macOS/Linux).
  Check `CARGO_TARGET_DIR` too if it is set; do not create per-task target
  directories or duplicate caches without a concrete need.
- For routine local validation, use consistent low-disk settings:
  `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=1`, and
  `CARGO_PROFILE_TEST_DEBUG=1`. Keep existing dependency optimization settings.
  Changing profiles, features, targets, or compiler flags repeatedly creates
  additional artifacts; reuse the same settings between runs.
- If the target directory exceeds 20 GiB or free space is below 10 GiB, stop
  before another build and request permission for scoped cleanup. Do not
  automatically delete artifacts or interrupt active builds/apps.
- With cleanup authorized and no debug build/test/dev app running, use
  `cargo clean --manifest-path src-tauri/Cargo.toml --profile dev` from the
  repository root to clear debug/test output while preserving release bundles.
  A full `cargo clean --manifest-path src-tauri/Cargo.toml` also removes release
  installers and bundles; only use it when those are disposable and not running.
  Never delete app data, media, `Cargo.lock`, or global Cargo/Rust caches as a
  substitute for build-cache cleanup. Verify size and free space afterward.
- Run the smallest relevant validation; do not regenerate the entire cache just
  to validate documentation-only changes.

# Cross-platform paths (macOS and Windows)

- Distinguish native filesystem paths from portable catalog paths. Keep native
  Rust paths as `Path`/`PathBuf`; use `join`, `parent`, `file_name`, `components`,
  and `strip_prefix`, not string concatenation, splitting, or prefix comparisons.
  Windows paths may contain drive letters, UNC shares, mixed separators, and
  verbatim prefixes such as `\\?\`. Do not assume every root is a drive letter.
- Catalog/device-relative paths use `/` on every OS. Reuse
  `src-tauri/src/paths::{join_relative, to_relative}` at the native/catalog
  boundary; do not introduce another normalizer or change stored path semantics.
  Native absolute paths must not be passed as catalog-relative paths.
- Never compare native paths through `to_string_lossy()` or assume
  `eq_ignore_ascii_case` handles filesystem identity. Separator differences,
  prefixes, and case sensitivity vary by filesystem, not just OS. For existing
  paths in tests, compare `fs::canonicalize` results when checking that lookup
  selected the same file. Propagate canonicalization errors; it requires the
  path to exist and is not a solution for planned/missing destination paths.
  For nonexistent paths, use component-based comparisons and the established
  resolver's semantics. Never lowercase all paths or strip prefixes globally.
- Use `to_str()` with an explicit error where a UTF-8 boundary is required;
  reserve lossy conversion for display/logging or existing documented catalog
  behavior, not path identity. Do not change unrelated path encoding behavior.
- In Node tooling, use `node:path` and `path.win32`/`path.posix` for explicitly
  simulated platform paths. Pass command arguments separately and quote/escape
  embedded script arguments using the existing runner helpers; cover spaces
  and backslashes. Frontend catalog paths remain portable `/` paths.
- For path-related changes, cover spaces, case differences, both separators,
  missing files, and relative traversal where relevant. Add Windows-specific
  coverage for drive roots, UNC paths, and verbatim paths when that surface
  accepts them. Keep filesystem tests in temporary directories; never touch
  real devices or media to test path handling.
- Run the relevant regression tests and desktop CI preflight. A macOS success
  does not confirm Windows behavior: require the Windows CI job to pass for
  cross-platform fixes, and distinguish local validation from remote CI and
  installer/release validation. Never declare a broken build fixed based only
  on typechecking or a subset of tests.

# UI implementation workflow

Implement UI changes directly using the app's existing components, patterns, and theme tokens. Validate behavior and update directly related documentation.

## Source card compact layout

Keep the **Browse media**, safe-copy, and mount-state controls together on the same compact row. Do not add explanatory, status, or comment text underneath that row. The only content permitted below it is the **Wipe card** button, and only when wiping is enabled and the source device is mounted. Put other status explanations in the relevant tooltip.

OpenPencil (`design/open-media-backup.fig`) is an optional design aid, not a prerequisite for implementation or approval. Do not block work on OpenPencil availability or design review without asking the user first. Use design prototyping when the user explicitly requests it.

The existing OpenPencil document is already in `design/open-media-backup.fig`.
When a design is requested, add clearly named proposal pages or frames there,
preserve existing designs, and save it in place. Export review previews to
`design/proposals/`; do not create a replacement design document. When the user
requests design approval before implementation, stop for approval after saving
the proposal and leave application code unchanged until approval.
