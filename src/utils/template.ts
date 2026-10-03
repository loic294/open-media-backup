const PLACEHOLDER = /\{([a-zA-Z0-9_]+)\}/g;

/** Built-in variables provided by the engine (see src-tauri/src/plan/vars.rs). */
export const BUILTIN_VARS = ["project", "project_name", "source_name", "backup_folder", "date", "year", "month", "day"];

export function templateVars(template: string): string[] {
  return [...template.matchAll(PLACEHOLDER)].map((m) => m[1]);
}

export interface TemplatePart {
  text: string;
  variable: boolean;
}

/** Splits a template into literal and `{variable}` parts, for highlighting. */
export function templateParts(template: string): TemplatePart[] {
  const parts: TemplatePart[] = [];
  let last = 0;
  for (const match of template.matchAll(PLACEHOLDER)) {
    if (match.index > last) parts.push({ text: template.slice(last, match.index), variable: false });
    parts.push({ text: match[0], variable: true });
    last = match.index + match[0].length;
  }
  if (last < template.length) parts.push({ text: template.slice(last), variable: false });
  return parts;
}

export function expandTemplate(template: string, values: Record<string, string>): string {
  return template.replace(PLACEHOLDER, (whole, name: string) => values[name] ?? whole);
}

export function todayVars(now = new Date()): Record<string, string> {
  const pad = (n: number) => String(n).padStart(2, "0");
  const [year, month, day] = [String(now.getFullYear()), pad(now.getMonth() + 1), pad(now.getDate())];
  return { date: `${year}-${month}-${day}`, year, month, day };
}

/** Mirrors plan/vars.rs: built-ins, space defaults, then non-empty project values. */
export function previewVars(
  space: { variables: { name: string; default_value: string }[] },
  project: { name: string; values: Record<string, string> } | null,
  sourceName = "{source_name}",
  now = new Date(),
): Record<string, string> {
  const vars: Record<string, string> = { ...todayVars(now), source_name: sourceName };
  if (project) Object.assign(vars, { project: project.name, project_name: project.name });
  for (const v of space.variables) if (v.default_value) vars[v.name] = v.default_value;
  for (const [k, v] of Object.entries(project?.values ?? {})) if (v) vars[k] = v;
  return vars;
}
