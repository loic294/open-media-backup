import { html, nothing } from "lit";
import { customElement } from "lit/decorators.js";
import { projectTotals } from "../../state/derived";
import { spaceFlows } from "../../state/selectors";
import { formatCount, plural } from "../../utils/format";
import { OmbElement } from "../ui/omb-element";
import { HASH_LABEL } from "../ui/hash-info";
import "./transfer-progress";

@customElement("omb-footer")
export class OmbFooter extends OmbElement {
  override render() {
    const { snapshot, space, status, project } = this.store;
    const totals = projectTotals(status, snapshot && space ? spaceFlows(snapshot, space.id) : []);
    return html`
      <footer class="flex items-center gap-5 px-5 py-3 border-t border-base-300 bg-base-200">
        <omb-transfer-progress></omb-transfer-progress>
        ${space
          ? html`<span class="flex items-center gap-2 text-sm text-base-content/60">
              <omb-icon name="fingerprint" class="text-success"></omb-icon>
              ${HASH_LABEL[space.hash_algo]} ${space.verify_mode === "reread" ? "re-read verification" : "verification"} on ·
              ${totals.errors ? html`<span class="text-error">${plural(totals.errors, "failed file")}</span>` : "0 mismatches"}
            </span>`
          : nothing}
        <span class="flex-1"></span>
        <button class="btn gap-2" ?disabled=${!project} @click=${() => this.store.open({ type: "preview", flowId: null })}>
          <omb-icon name="images"></omb-icon>Preview
        </button>
        <button class="btn btn-primary gap-2" ?disabled=${!totals.runnable} @click=${() => this.store.runAll()}>
          <omb-icon name="play"></omb-icon>Run all transfers
          ${totals.runnable ? html`<span class="badge badge-sm bg-primary-content/20 border-0 text-primary-content">${formatCount(totals.runnable)}</span>` : nothing}
        </button>
      </footer>
    `;
  }
}
