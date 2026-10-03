# Required design-review workflow

For every UI or product-design change, update the OpenPencil design first and present the updated design to the user. Do not implement the corresponding code until the user explicitly approves that design. A request to continue autonomously, a promise to review later, or approval of an earlier design is not approval of a new or revised design. Keep the related task in progress until design approval, implementation, and verification are complete.

# Keep the OpenPencil design in sync

The OpenPencil file `design/open-media-backup.fig` is the source of truth for the app's UI so visual changes can be prototyped there before touching code. It must always reflect what the app actually ships.

- Before any UI change, open `design/open-media-backup.fig` with the OpenPencil tools, prototype the change there, and get explicit user approval (see the design-review workflow above).
- When implementing, match the approved design: layout, spacing, copy, colors, and states. If the implementation has to diverge, update the design to match and call out the difference to the user.
- After shipping any change that affects the UI (new or changed components, dialogs, menus, settings, copy, theme colors, tooltips, empty/error states), update the design in the same change so the file never drifts from the app.
- Reuse the design's existing components, variables, and color tokens; keep theme colors in the design aligned with `src/styles/app.css`.
- Save the `.fig` file and refresh the affected `design/preview-*.png` exports so reviewers can see the current design without opening OpenPencil.
- Non-visual changes (backend logic, tests, refactors) don't need a design update.
