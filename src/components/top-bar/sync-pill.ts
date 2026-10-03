import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import type { SyncStatus } from "../../api/types";
import { initials, percent } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";
import { PEER_TONE } from "./peer-tone";

export function syncSummary(sync: SyncStatus | null): { label: string; tone: string } {
  if (!sync || sync.peers.length === 0) return { label: "No peers", tone: "text-base-content/60" };
  const total = sync.peers.length;
  const online = sync.peers.filter((p) => p.state !== "offline").length;
  const errors = sync.peers.filter((p) => p.state === "error").length;
  if (errors)
    return { label: `Sync error on ${errors} ${errors === 1 ? "peer" : "peers"}`, tone: "text-error" };
  if (sync.syncing) return { label: `Syncing ${online} of ${total} peers`, tone: "" };
  if (online < total) return { label: `${online} of ${total} peers online`, tone: "text-base-content/70" };
  return { label: "All peers up to date", tone: "text-success" };
}

/** Top-right sync indicator. Opens the device sync dialog. */
@customElement("omb-sync-pill")
export class OmbSyncPill extends OmbElement {
  override render() {
    const sync = this.store.sync;
    const { label, tone } = syncSummary(sync);
    return html`
      <button
        class="btn btn-ghost h-11 gap-3 px-3 rounded-box border border-base-300 bg-base-100 font-normal whitespace-nowrap"
        @click=${() => this.store.open({ type: "device-sync" })}
      >
        <omb-icon
          name="refresh-cw"
          class="size-4 text-info ${sync?.syncing ? "animate-spin [animation-duration:2s]" : ""}"
        ></omb-icon>
        <span class="flex flex-col items-stretch gap-1 min-w-36 text-left">
          <span class="text-sm flex justify-between gap-2">
            <span class="font-medium ${tone}">${label}</span>
            ${sync?.syncing ? html`<span class="text-base-content/60">${percent(sync.progress, 1)}%</span>` : nothing}
          </span>
          ${sync?.syncing ? html`<progress class="progress progress-info h-1" value=${sync.progress * 100} max="100"></progress>` : nothing}
        </span>
        <span class="flex -space-x-1.5">
          ${(sync?.peers ?? [])
            .slice(0, 4)
            .map(
              (p) =>
                html`<span
                  title=${p.name}
                  class="grid place-items-center size-6 rounded-full text-[10px] font-bold ring-2 ring-base-100 ${PEER_TONE[p.state]}"
                  >${initials(p.name)}</span
                >`,
            )}
        </span>
        <omb-icon name="chevron-down" class="size-4 opacity-60"></omb-icon>
      </button>
    `;
  }
}
