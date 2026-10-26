import { html, nothing } from "lit";
import type { Project, ProjectGranularity } from "../../api/types";
import { formatProjectTime, localProjectRange, parseProjectTime } from "../../utils/project-time";

export function projectRangeFields(
  project: Project,
  change: (project: Project) => void,
  reportError: (message: string) => void,
) {
  const granularity = project.granularity ?? "minute";
  const update = (action: () => Project) => {
    try {
      change(action());
      return true;
    } catch (error) {
      reportError(error instanceof Error ? error.message : String(error));
      return false;
    }
  };
  return html`<section class="flex flex-col gap-3">
    <div>
      <h4 class="font-medium">Capture-time range</h4>
      <p class="text-sm text-base-content/60">
        Dates and times use this machine's timezone (${Intl.DateTimeFormat().resolvedOptions().timeZone}).
        Saved in UTC. Both bounds are inclusive; leave both empty for no range.
      </p>
    </div>
    <fieldset class="fieldset">
      <legend class="fieldset-legend">Date precision</legend>
      <select
        class="select w-full"
        aria-label="Project date precision"
        .value=${granularity}
        @change=${(event: Event) =>
          update(() =>
            localProjectRange(project, (event.target as HTMLSelectElement).value as ProjectGranularity),
          )}
      >
        <option value="day">Date only</option>
        <option value="minute">Date and time (optional)</option>
        <option value="year">Year</option>
      </select>
    </fieldset>
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
      ${(["start_time", "end_time"] as const).map(
        (key) =>
          html`<label class="fieldset">
            <span class="fieldset-legend">${key === "start_time" ? "Inclusive start" : "Inclusive end"}</span>
            <input
              class="input w-full"
              type=${granularity === "minute" ? "datetime-local" : granularity === "day" ? "date" : "number"}
              min=${granularity === "year" ? "1" : granularity === "day" ? "0001-01-01" : "0001-01-01T00:00"}
              max=${granularity === "year" ? "9999" : granularity === "day" ? "9999-12-31" : "9999-12-31T23:59"}
              step=${granularity === "year" ? "1" : nothing}
              .value=${formatProjectTime(project[key], granularity)}
              aria-label=${key === "start_time" ? "Inclusive capture range start" : "Inclusive capture range end"}
              @change=${(event: Event) => {
                const input = event.target as HTMLInputElement;
                if (!input.validity.valid) {
                  input.reportValidity();
                  reportError("Invalid project capture time");
                  input.value = formatProjectTime(project[key], granularity);
                  return;
                }
                const updated = update(() =>
                  localProjectRange(project, granularity, {
                    [key]: parseProjectTime(input.value, granularity, key === "end_time"),
                  }),
                );
                if (!updated) input.value = formatProjectTime(project[key], granularity);
              }}
            />
          </label>`,
      )}
    </div>
    <p class="text-xs text-base-content/60">
      Date-only and year ranges include the entire selected local days or years. Date and time includes the
      entire final minute.
    </p>
  </section>`;
}
