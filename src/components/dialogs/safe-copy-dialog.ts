import { html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { FileRule, SafeCopyFile, SourceSafeCopyDetails, WorkspaceContext } from "../../api/types";
import type { DialogRequest } from "../../state/dialogs";
import { ruleError } from "../form/rules-editor";
import "../form/rules-editor";
import { DialogBase } from "./dialog-base";

@customElement("omb-safe-copy-dialog")
export class OmbSafeCopyDialog extends DialogBase<Extract<DialogRequest, { type: "safe-copy" }>> {
  @state() private details: SourceSafeCopyDetails | null = null;
  @state() private error = "";
  @state() private loading = true;
  @state() private busy = false;
  @state() private rules: FileRule[] = [];
  @state() private dirty = false;
  @state() private filter: "all" | SafeCopyFile["state"] = "all";
  @state() private search = "";
  @state() private page = 0;
  @state() private projectId: string | null = null;
  private sequence = 0;

  override connectedCallback(): void {
    super.connectedCallback();
    const source = this.store.snapshot?.sources.find((item) => item.id === this.request.sourceId);
    this.rules = structuredClone(source?.safe_copy_rules ?? []);
    // The workspace has no selected project. Make the policy being inspected explicit.
    this.projectId =
      this.store.snapshot?.projects.find(
        (project) =>
          project.space_id === source?.space_id &&
          !project.archived &&
          source?.project_scope?.mode !== "none" &&
          (source?.project_scope?.mode !== "selected" ||
            source.project_scope.project_ids.includes(project.id)),
      )?.id ?? null;
    void this.load();
  }

  private context(): WorkspaceContext {
    const source = this.store.snapshot?.sources.find((item) => item.id === this.request.sourceId);
    if (!source) throw new Error("Source no longer exists");
    return { spaceId: source.space_id, projectId: this.projectId };
  }

  private async load() {
    const sequence = ++this.sequence;
    this.loading = true;
    this.details = null;
    this.page = 0;
    this.error = "";
    try {
      const details = await this.store.backend.getSourceSafeCopyDetails(
        this.context(),
        this.request.sourceId,
      );
      if (sequence === this.sequence) this.details = details;
    } catch (error) {
      if (sequence === this.sequence) this.error = String(error);
    } finally {
      if (sequence === this.sequence) this.loading = false;
    }
  }

  private async save() {
    this.busy = true;
    this.error = "";
    try {
      await this.store.backend.saveSourceSafeCopyRules(this.context(), this.request.sourceId, this.rules);
      this.dirty = false;
      await this.store.reloadSnapshot();
      this.store.refreshStatus();
      await this.load();
      this.store.toast("success", "Safe-copy exclusions saved and synced");
    } catch (error) {
      this.error = String(error);
      this.store.toast("error", this.error);
    } finally {
      this.busy = false;
    }
  }

  private fileRow(file: SafeCopyFile) {
    return html`<tr>
      <td class="font-mono text-xs break-all whitespace-normal">${file.path}</td>
      <td class="align-top">
        <span
          class="badge badge-sm badge-soft ${file.state === "safe" ? "badge-success" : file.state === "unsafe" ? "badge-warning" : "badge-ghost"}"
        >
          ${file.state === "safe" ? "Safe" : file.state === "unsafe" ? "Not safe" : "Excluded"}
        </span>
        <div class="text-xs mt-1">${file.safe_copies} effective copies</div>
      </td>
      <td class="text-xs whitespace-normal">
        ${file.verified_destinations.length ? html`<p>Verified: ${file.verified_destinations.join(", ")}</p>` : nothing}
        ${file.acknowledged_destinations.length ? html`<p class="text-warning">Skip acknowledged (not byte-verified): ${file.acknowledged_destinations.join(", ")}</p>` : nothing}
        ${file.reasons.map((reason) => html`<p class="mt-1 text-base-content/70">${reason}</p>`)}
      </td>
    </tr>`;
  }

  override render() {
    const details = this.details;
    const source = this.store.snapshot?.sources.find((item) => item.id === this.request.sourceId);
    const projects =
      this.store.snapshot?.projects.filter(
        (project) =>
          project.space_id === source?.space_id &&
          !project.archived &&
          source?.project_scope?.mode !== "none" &&
          (source?.project_scope?.mode !== "selected" ||
            source.project_scope.project_ids.includes(project.id)),
      ) ?? [];
    const files = (details?.files ?? []).filter(
      (file) =>
        (this.filter === "all" || file.state === this.filter) &&
        file.path.toLowerCase().includes(this.search.toLowerCase()),
    );
    const count = (state: SafeCopyFile["state"]) =>
      details?.files.filter((file) => file.state === state).length ?? 0;
    const body = html`<div class="flex flex-col gap-4">
      <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <label class="text-sm"
          >Safety policy
          <select
            class="select select-sm ml-2"
            aria-label="Safety policy"
            .value=${this.projectId ?? ""}
            ?disabled=${this.busy}
            @change=${(event: Event) => {
              this.projectId = (event.target as HTMLSelectElement).value || null;
              this.page = 0;
              void this.load();
            }}
          >
            <option value="" ?selected=${this.projectId === null}>Workspace (no single threshold)</option>
            ${projects.map((project) => html`<option value=${project.id} ?selected=${this.projectId === project.id}>${project.name} · ${project.final_copies_required} copies</option>`)}
          </select>
        </label>
        <button
          class="btn btn-sm btn-ghost"
          ?disabled=${this.loading || this.busy}
          @click=${() => this.load()}
        >
          Refresh
        </button>
      </div>
      <p class="text-sm text-base-content/70">
        Coverage is shared by source tasks on this device. Wiping is checked against every applicable active
        project. A device copy counts only when a destination covers every required file. Temporary copies
        count only under the space policy.
      </p>
      ${this.error ? html`<div role="alert" class="alert alert-error alert-soft text-sm">${this.error}<button class="btn btn-sm" @click=${() => this.load()}>Retry</button></div>` : nothing}
      ${
        this.loading
          ? html`<p role="status" class="py-6 text-center">Loading safe-copy details...</p>`
          : this.error && !details
            ? nothing
            : details
              ? html` <div class="rounded-box bg-base-200 p-3 text-sm">
                    <strong
                      >${details.safe_copies}${details.required_copies === null ? "" : ` / ${details.required_copies}`}
                      effective device copies</strong
                    >
                    <p class="mt-1">
                      ${details.blocking_reason ?? (details.wipe_eligible ? "Ready to wipe under every applicable active project" : "Wipe is not enabled for this source")}
                    </p>
                    <p class="mt-1">
                      ${count("safe")} safe · ${count("unsafe")} not safe · ${count("excluded")} excluded
                    </p>
                  </div>
                  ${this.dirty ? html`<p role="status" class="text-sm text-warning">Unsaved rules. File coverage below still reflects saved rules.</p>` : nothing}
                  <div class="flex flex-col gap-2 sm:flex-row">
                    <input
                      class="input input-sm flex-1"
                      aria-label="Search source files"
                      placeholder="Search source files"
                      .value=${this.search}
                      @input=${(event: Event) => {
                        this.search = (event.target as HTMLInputElement).value;
                        this.page = 0;
                      }}
                    />
                    <select
                      class="select select-sm"
                      aria-label="File safety filter"
                      .value=${this.filter}
                      @change=${(event: Event) => {
                        this.filter = (event.target as HTMLSelectElement).value as typeof this.filter;
                        this.page = 0;
                      }}
                    >
                      <option value="all">All files</option>
                      <option value="safe">Safe (${count("safe")})</option>
                      <option value="unsafe">Not safe (${count("unsafe")})</option>
                      <option value="excluded">Excluded (${count("excluded")})</option>
                    </select>
                  </div>
                  ${
                    files.length
                      ? html`<div
                            class="overflow-x-auto max-h-72 overflow-y-auto rounded-box border border-base-300"
                          >
                            <table class="table table-sm table-pin-rows">
                              <thead>
                                <tr>
                                  <th class="w-1/3">Device-relative file</th>
                                  <th>Status</th>
                                  <th>Evidence / next step</th>
                                </tr>
                              </thead>
                              <tbody>
                                ${files.slice(this.page * 50, (this.page + 1) * 50).map((file) => this.fileRow(file))}
                              </tbody>
                            </table>
                          </div>
                          <div class="flex items-center justify-end gap-2 text-xs">
                            <span
                              >${this.page * 50 + 1}-${Math.min(files.length, (this.page + 1) * 50)} of
                              ${files.length}</span
                            >
                            <button
                              class="btn btn-xs"
                              ?disabled=${this.page === 0}
                              @click=${() => this.page--}
                            >
                              Previous
                            </button>
                            <button
                              class="btn btn-xs"
                              ?disabled=${(this.page + 1) * 50 >= files.length}
                              @click=${() => this.page++}
                            >
                              Next
                            </button>
                          </div>`
                      : html`<p role="status" class="py-6 text-center text-base-content/60">
                          ${details.files.length ? "No files match this filter." : "No source files are known. Connect the device and scan or transfer its files."}
                        </p>`
                  }
                  <section class="rounded-box border border-base-300 p-4">
                    <h4 class="font-semibold">Files not required for safe copy</h4>
                    <p class="text-sm text-base-content/70 mt-1 mb-3">
                      Rules apply relative to this source folder (${source?.path_template || "whole device"}),
                      and sync to peers. Excluded files do not block readiness; transfers are unchanged.
                      Delete backed-up files preserves them, but quick format erases them. A file required by
                      another source task still counts.
                    </p>
                    ${
                      details.editable
                        ? html`<fieldset ?disabled=${this.busy}>
                            <omb-rules-editor
                              .rules=${this.rules}
                              .variables=${this.store.snapshot?.spaces.find((space) => space.id === source?.space_id)?.variables.map((variable) => variable.name) ?? []}
                              @rules-change=${(event: CustomEvent<FileRule[]>) => {
                                this.rules = event.detail;
                                this.dirty = true;
                              }}
                            ></omb-rules-editor>
                          </fieldset>`
                        : html`<p class="text-sm">
                            Read-only: exclusions can only be edited for devices mapped on this computer.
                          </p>`
                    }
                  </section>`
              : nothing
      }
    </div>`;
    return html`<omb-modal
      heading="Safe-copy details"
      subheading=${details ? `${details.device_name} · ${source?.task_name || source?.path_template || "Whole device"}` : "Source device coverage"}
      icon="shield-check"
      size="xl"
      @close=${this.onClosed}
      .body=${body}
      .actions=${html`<button class="btn btn-ghost" @click=${() => this.dismiss()}>Close</button> ${
          details?.editable
            ? html`<button
                class="btn btn-primary"
                ?disabled=${!this.dirty || this.busy || this.loading || this.rules.some((rule) => !!ruleError(rule))}
                @click=${() => this.save()}
              >
                ${this.busy ? "Saving..." : "Save exclusions"}
              </button>`
            : nothing
        }`}
    ></omb-modal>`;
  }
}
