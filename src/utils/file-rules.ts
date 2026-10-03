import type { FileRule } from "../api/types";

const GLOB_META = /[\\*?[\]{}]/g;

export function escapeGlobLiteral(value: string): string {
  return value.replace(GLOB_META, "\\$&");
}

export function extensionForRule(fileName: string): string | null {
  const dot = fileName.lastIndexOf(".");
  if (dot <= 0 || dot === fileName.length - 1) return null;
  return fileName.slice(dot + 1);
}

export function exactFilenameExcludePattern(fileName: string): string {
  return escapeGlobLiteral(fileName);
}

export function extensionExcludePattern(fileName: string): string | null {
  const extension = extensionForRule(fileName);
  return extension ? `*.${escapeGlobLiteral(extension)}` : null;
}

export function appendExcludeRule(rules: FileRule[], pattern: string): { rules: FileRule[]; added: boolean } {
  const normalized = pattern.trim().toLowerCase();
  const exists = rules.some(
    (rule) =>
      rule.action === "exclude" &&
      rule.syntax === "glob" &&
      rule.pattern.trim().toLowerCase() === normalized,
  );
  if (exists) return { rules, added: false };
  return { rules: [...rules, { action: "exclude", syntax: "glob", pattern }], added: true };
}

function globToRegExp(pattern: string): RegExp {
  let out = "^";
  for (let i = 0; i < pattern.length; i++) {
    const char = pattern[i];
    if (char === "\\") {
      i++;
      out += i < pattern.length ? escapeRegExp(pattern[i]) : "\\\\";
    } else if (char === "*") {
      out += ".*";
    } else if (char === "?") {
      out += ".";
    } else {
      out += escapeRegExp(char);
    }
  }
  return new RegExp(`${out}$`, "i");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function ruleMatches(rule: FileRule, relPath: string, folders: string[], file: string): boolean {
  const raw = rule.pattern.trim();
  if (!raw) return false;
  if (rule.syntax === "regex") {
    const regex = new RegExp(raw, "i");
    return regex.test(file) || folders.some((folder) => regex.test(folder));
  }
  if (raw.endsWith("/")) {
    const glob = globToRegExp(raw.replace(/\/+$/, ""));
    return folders.some((folder) => glob.test(folder));
  }
  if (raw.includes("/")) return globToRegExp(raw.replace(/^\/+/, "")).test(relPath);
  const glob = globToRegExp(raw);
  return glob.test(file) || folders.some((folder) => glob.test(folder));
}

export function rulesAllowPath(rules: FileRule[], relPath: string): boolean {
  const parts = relPath.split("/").filter(Boolean);
  const file = parts.at(-1);
  if (!file) return false;
  const folders = parts.slice(0, -1);
  let included = !rules.some((rule) => rule.action === "include" && rule.pattern.trim());
  for (const rule of rules) {
    if (ruleMatches(rule, relPath, folders, file)) included = rule.action === "include";
  }
  return included;
}
