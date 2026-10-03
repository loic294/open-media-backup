import { html } from "lit";
import { customElement } from "lit/decorators.js";
import { DEMO, inDesktopShell } from "../../api";
import { installDropdownDismiss } from "../ui/dropdown";
import { OmbElement } from "../ui/omb-element";
import "../top-bar/omb-top-bar";
import "../workspace/omb-workspace";
import "../workspace/projects-page";
import "../footer/omb-footer";
import "../dialogs/omb-dialog-host";
import "./omb-toasts";

@customElement("omb-app")
export class OmbApp extends OmbElement {
  override connectedCallback(): void {
    super.connectedCallback();
    this.classList.add("flex", "flex-col", "h-screen", "bg-base-200", "text-base-content");
    installDropdownDismiss();
    if (DEMO || inDesktopShell()) void this.store.init();
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this.store.dispose();
  }

  override render() {
    if (!DEMO && !inDesktopShell()) {
      return html`<div class="m-auto max-w-md alert">
        <omb-icon name="circle-alert"></omb-icon>
        <span>Open Media Backup runs in its desktop app (<code>npx tauri dev</code>). To try the interface in a browser with sample data, start <code>npm run dev:demo</code>.</span>
      </div>`;
    }
    const { snapshot, error } = this.store;
    if (error) {
      return html`<div class="m-auto max-w-md alert alert-error"><omb-icon name="circle-alert"></omb-icon><span>Could not start: ${error}</span></div>`;
    }
    if (!snapshot) return html`<span class="m-auto loading loading-spinner loading-lg text-primary"></span>`;
    if (this.store.projectsPageOpen) {
      return html`
        <omb-projects-page></omb-projects-page>
        <omb-dialog-host></omb-dialog-host>
        <omb-toasts></omb-toasts>
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
