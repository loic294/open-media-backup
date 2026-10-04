import { html } from "lit";
import { customElement } from "lit/decorators.js";
import { DEMO, inDesktopShell } from "../../api";
import "../footer/active-jobs-panel";
import { installDropdownDismiss } from "../ui/dropdown";
import { OmbElement } from "../ui/omb-element";
import "../top-bar/omb-top-bar";
import "../workspace/omb-workspace";
import "../footer/omb-footer";
import "../dialogs/omb-dialog-host";
import "./omb-toasts";

const activeJobsWindow = () => new URLSearchParams(window.location.search).get("ombWindow") === "active-jobs";

@customElement("omb-app")
export class OmbApp extends OmbElement {
  override connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("flex", "flex-col", "h-screen", "bg-base-200", "text-base-content");
    installDropdownDismiss();
    if (DEMO || inDesktopShell()) {
      void (activeJobsWindow() ? this.store.initActiveJobs() : this.store.init());
    }
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this.store.dispose();
  }

  override render() {
    if (!DEMO && !inDesktopShell()) {
      return html`<div class="m-auto max-w-md alert">
        <omb-icon name="circle-alert"></omb-icon>
        <span
          >Open Media Backup runs in its desktop app (<code>npx tauri dev</code>). To try the interface in a
          browser with sample data, start <code>npm run dev:demo</code>.</span
        >
      </div>`;
    }
    const { snapshot, error } = this.store;
    if (error) {
      return html`<div class="m-auto max-w-md alert alert-error">
        <omb-icon name="circle-alert"></omb-icon><span>Could not start: ${error}</span>
      </div>`;
    }
    if (!snapshot) return html`<span class="m-auto loading loading-spinner loading-lg text-primary"></span>`;
    if (activeJobsWindow()) {
      return html`
        <main class="h-screen bg-base-200 text-base-content">
          <omb-active-jobs-panel data-omb-block standalone></omb-active-jobs-panel>
          <omb-toasts></omb-toasts>
        </main>
      `;
    }
    return html`
      <omb-top-bar></omb-top-bar>
      <omb-workspace class="flex-1 min-h-0"></omb-workspace>
      <omb-footer></omb-footer>
      <omb-dialog-host></omb-dialog-host>
      <omb-toasts></omb-toasts>
    `;
  }
}
