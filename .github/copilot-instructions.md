# UI implementation workflow

Implement UI changes directly using the app's existing components, patterns, and theme tokens. Validate behavior and update directly related documentation.

OpenPencil (`design/open-media-backup.fig`) is an optional design aid, not a prerequisite for implementation or approval. Do not block work on OpenPencil availability or design review without asking the user first. Use design prototyping when the user explicitly requests it.

The existing OpenPencil document is already in `design/open-media-backup.fig`.
When a design is requested, add clearly named proposal pages or frames there,
preserve existing designs, and save it in place. Export review previews to
`design/proposals/`; do not create a replacement design document. When the user
requests design approval before implementation, stop for approval after saving
the proposal and leave application code unchanged until approval.
