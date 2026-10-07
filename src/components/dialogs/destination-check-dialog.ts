import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { DestinationCheckScope } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { sourceStatus } from "../../state/derived";
import { deviceById } from "../../state/selectors";
import { destinationTaskName, sourceTaskName } from "../../utils/names";
import { DialogBase } from "./dialog-base";

const OPTIONS: [DestinationCheckScope["kind"], string][] = [
  ["allDestination", "Scan all files in the destination"],
  ["configuredSources", "Check all files matching configured sources"],
  ["selectedSources", "Choose specific configured sources to check"],
];

@customElement("omb-destination-check-dialog")
export class OmbDestinationCheckDialog extends DialogBase<
  Extract<DialogRequest, { type: "destination-check" }>
> {
  @state() private scope: DestinationCheckScope["kind"] = "configuredSources";
  @state() private selected: string[] = [];
  @state() private busy = false;

  /** Unknown status (not loaded yet) is not treated as offline. */
  #offline(sourceId: string) {
    return sourceStatus(this.store.status, sourceId)?.available === false;
  }

  #selectedOnline() {
    return this.selected.filter((id) => !this.#offline(id));
  }

  async #start() {
    this.busy = true;
    try {
      const scope: DestinationCheckScope =
        this.scope === "selectedSources"
          ? { kind: this.scope, sourceIds: this.#selectedOnline() }
          : { kind: this.scope };
      if (await this.store.checkDestination(this.request.destinationId, scope, this.request.context))
        this.dismiss();
    } finally {
      this.busy = false;
    }
  }

  override render() {
    const snapshot = this.store.snapshot;
    if (!snapshot) return nothing;
    const destination = snapshot.destinations.find((d) => d.id === this.request.destinationId);
    const incoming = new Set(
      snapshot.flows
        .filter(
          (f) =>
            f.space_id === this.request.context.spaceId && f.destination_id === this.request.destinationId,
        )
        .map((f) => f.source_id),
    );
    const sources = snapshot.sources.filter(
      (s) => s.space_id === this.request.context.spaceId && incoming.has(s.id),
    );
    return html`<omb-modal
      heading="Check destination"
      subheading=${
        destination
          ? destinationTaskName(destination, deviceById(snapshot, destination.device_id))
          : "Destination no longer configured"
      }
      icon="fingerprint"
      .closeable=${!this.busy}
      @close=${this.onClosed}
      .body=${html`
        <fieldset class="space-y-3" ?disabled=${this.busy}>
          <legend class="font-semibold mb-3">Which files should be checked?</legend>
          ${OPTIONS.map(
            ([kind, label]) => html`
              <label class="flex items-start gap-3 cursor-pointer">
                <input
                  type="radio"
                  class="radio radio-sm mt-0.5"
                  name=${`destination-check-${this.request.destinationId}`}
                  value=${kind}
                  .checked=${this.scope === kind}
                  @change=${() => {
                    this.scope = kind;
                  }}
                />
                <span class="text-sm">${label}</span>
              </label>
            `,
          )}
          ${
            this.scope === "selectedSources"
              ? html`<fieldset class="border border-base-300 rounded-box p-3 space-y-3">
                  <legend class="text-sm px-1">Configured sources</legend>
                  ${sources.map((source) => {
                    const offline = this.#offline(source.id);
                    return html`<label
                      class="flex items-start gap-3 ${offline ? "cursor-not-allowed opacity-60" : "cursor-pointer"}"
                    >
                      <input
                        type="checkbox"
                        class="checkbox checkbox-sm mt-0.5"
                        value=${source.id}
                        ?disabled=${offline}
                        .checked=${!offline && this.selected.includes(source.id)}
                        @change=${(event: Event) => {
                          this.selected = (event.target as HTMLInputElement).checked
                            ? [...this.selected, source.id]
                            : this.selected.filter((id) => id !== source.id);
                        }}
                      />
                      <span class="text-sm">
                        ${sourceTaskName(source, deviceById(snapshot, source.device_id))}
                        ${offline ? html`<span class="badge badge-ghost badge-sm ml-1">Offline</span>` : nothing}
                        <span class="block text-xs text-base-content/60">${source.path_template}</span>
                      </span>
                    </label>`;
                  })}
                  <p class="text-xs text-base-content/60">
                    ${
                      sources.length
                        ? "Select at least one connected source. Offline sources cannot be checked."
                        : "No sources are configured for this destination."
                    }
                  </p>
                </fieldset>`
              : nothing
          }
        </fieldset>
        <p class="text-sm text-base-content/70 mt-4">
          ${
            this.scope === "allDestination"
              ? html`Checks every file in the existing destination folders, resolved from projects and from
                folders already recorded in the catalog (such as previous backup folders). Tracked files are
                checked against catalog hashes, even with sources offline. Files without comparable catalog
                evidence are reported as untracked, not verified. Symlinks are not followed.`
              : html`Compares connected, valid sources with their expected destination paths using fresh
                hashes and configured file rules. Offline sources are skipped. Reports missing or different
                files.`
          }
        </p>
        <p class="text-xs text-base-content/60 mt-3">
          No media files are written or deleted. Only proven stale catalog claims are invalidated; source
          comparisons also record verified matches.
        </p>
      `}
      .actions=${html`
        <button class="btn" ?disabled=${this.busy} @click=${() => this.dismiss()}>Cancel</button>
        <button
          class="btn btn-primary"
          ?disabled=${
            this.busy || !destination || (this.scope === "selectedSources" && !this.#selectedOnline().length)
          }
          @click=${() => this.#start()}
        >
          ${this.busy ? "Starting..." : "Start check"}
        </button>
      `}
    ></omb-modal>`;
  }
}
