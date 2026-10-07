# Completing changes

After completing and validating changes, always commit the task's changes and push
the commit to the current branch's remote. Do not include unrelated changes in the
commit. If committing or pushing is blocked, report the blocker explicitly.

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
