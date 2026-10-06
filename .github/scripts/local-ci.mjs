import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const repoRoot = fileURLToPath(new URL("../../", import.meta.url));

export function cargoTestArgs(platform = process.platform, root = repoRoot) {
  const paths = platform === "win32" ? path.win32 : path;
  const args = [
    "test",
    "--locked",
    "--manifest-path",
    paths.join(root, "src-tauri/Cargo.toml"),
    "--workspace",
  ];
  if (platform === "win32") {
    // Cargo runs each test in its package directory, not the workspace directory.
    const runner = [
      "powershell.exe",
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      paths.join(root, "src-tauri/.cargo/run-test.ps1"),
    ];
    args.push("--config", `target.x86_64-pc-windows-msvc.runner=${JSON.stringify(runner)}`);
  }
  return args;
}

export function preflightCommands(platform = process.platform, root = repoRoot) {
  return [
    ["node", "--test", ".github/scripts/release.test.mjs", ".github/scripts/local-ci.test.mjs"],
    ["npm", "run", "typecheck"],
    ["npm", "run", "lint"],
    ["npm", "test"],
    ["npm", "run", "build"],
    ["cargo", "fmt", "--manifest-path", "src-tauri/Cargo.toml", "--all", "--", "--check"],
    [
      "cargo",
      "clippy",
      "--locked",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--workspace",
      "--all-targets",
      "--",
      "-D",
      "warnings",
    ],
    ["cargo", ...cargoTestArgs(platform, root)],
  ];
}

export function runCommands(commands, execute = spawnSync, cwd = repoRoot) {
  for (const [command, ...args] of commands) {
    console.log(`\n> ${command} ${args.join(" ")}`);
    const result = execute(command, args, {
      cwd,
      stdio: "inherit",
      // npm.cmd needs a shell on Windows; all npm arguments here are fixed.
      shell: process.platform === "win32" && command === "npm",
    });
    if (result.error) throw result.error;
    if (result.status !== 0) {
      throw new Error(`${command} failed (${result.signal ?? `exit ${result.status}`})`);
    }
  }
}

export function main(args = process.argv.slice(2)) {
  if (args.length > 1 || (args.length === 1 && args[0] !== "--rust-only")) {
    throw new Error("Usage: node .github/scripts/local-ci.mjs [--rust-only]");
  }
  for (const file of ["run-test.ps1", "comctl-v6.xml"]) {
    const filename = path.join(repoRoot, "src-tauri/.cargo", file);
    if (!existsSync(filename)) throw new Error(`Missing Windows test runner asset: ${filename}`);
  }
  const rustOnly = args[0] === "--rust-only";
  runCommands(rustOnly ? [["cargo", ...cargoTestArgs()]] : preflightCommands());
  console.log(`\n${rustOnly ? "Rust tests" : "Desktop CI preflight"} passed on ${process.platform}.`);
  console.log("Other OS matrix jobs, Linux/Docker and release packaging still require separate validation.");
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    main();
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}
