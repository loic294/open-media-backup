import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { cargoTestArgs, main, preflightCommands, repoRoot, runCommands } from "./local-ci.mjs";

test("Windows runner is an absolute, escaped argv array even in a workspace member", () => {
  const root = "D:\\a\\media backup";
  const args = cargoTestArgs("win32", root);
  assert.deepEqual(args.slice(0, 6), [
    "test",
    "--locked",
    "--manifest-path",
    "D:\\a\\media backup\\src-tauri\\Cargo.toml",
    "--workspace",
    "--config",
  ]);
  const config = args[6];
  assert.ok(config.startsWith("target.x86_64-pc-windows-msvc.runner="));
  const runner = JSON.parse(config.slice(config.indexOf("=") + 1));
  assert.deepEqual(runner.slice(0, 5), [
    "powershell.exe",
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
  ]);
  assert.equal(runner[5], "D:\\a\\media backup\\src-tauri\\.cargo\\run-test.ps1");
  for (const cwd of [root, `${root}\\src-tauri`, `${root}\\src-tauri\\crates\\omb-hash`]) {
    assert.equal(path.win32.resolve(cwd, runner[5]), runner[5]);
  }
});

test("non-Windows tests do not install a PowerShell runner", () => {
  for (const platform of ["darwin", "linux"]) {
    assert.deepEqual(cargoTestArgs(platform, "/repo"), [
      "test",
      "--locked",
      "--manifest-path",
      "/repo/src-tauri/Cargo.toml",
      "--workspace",
    ]);
  }
});

test("local and CI entry points share all desktop gates and test runner assets", () => {
  const workflow = readFileSync(new URL("../workflows/ci.yml", import.meta.url), "utf8");
  const pkg = JSON.parse(readFileSync(path.join(repoRoot, "package.json"), "utf8"));
  assert.match(workflow, /run: npm run ci:local/);
  assert.equal(pkg.scripts["ci:local"], "node .github/scripts/local-ci.mjs");
  assert.equal(pkg.scripts["test:rust"], "node .github/scripts/local-ci.mjs --rust-only");
  const commands = preflightCommands("win32", "C:\\repo");
  assert.ok(commands[0].includes(".github/scripts/local-ci.test.mjs"));
  for (const name of ["typecheck", "lint", "build"]) {
    assert.ok(commands.some((command) => command.join(" ") === `npm run ${name}`));
  }
  assert.ok(commands.some((command) => command.join(" ") === "npm test"));
  assert.ok(commands.some((command) => command.includes("fmt") && command.includes("--check")));
  assert.ok(
    commands.some(
      (command) =>
        command.includes("clippy") &&
        command.includes("--workspace") &&
        command.includes("--all-targets") &&
        command.includes("warnings"),
    ),
  );
  assert.deepEqual(commands.at(-1), ["cargo", ...cargoTestArgs("win32", "C:\\repo")]);
  assert.match(
    readFileSync(path.join(repoRoot, "src-tauri/.cargo/run-test.ps1"), "utf8"),
    /\$PSScriptRoot 'comctl-v6.xml'/,
  );
});

test("preflight stops on failed commands, signals and missing executables", () => {
  for (const failure of [
    { status: 9 },
    { status: null, signal: "SIGTERM" },
    { error: new Error("ENOENT") },
  ]) {
    const calls = [];
    assert.throws(() =>
      runCommands([["first"], ["failed"], ["unreachable"]], (command, args, options) => {
        calls.push(command);
        assert.equal(options.cwd, repoRoot);
        return command === "first" ? { status: 0 } : failure;
      }),
    );
    assert.deepEqual(calls, ["first", "failed"]);
  }
  assert.throws(() => main(["--unknown"]), /Usage/);
});

test("Cargo accepts absolute runner configuration from a member cwd with spaces", () => {
  const root = mkdtempSync(path.join(tmpdir(), "omb runner regression "));
  try {
    mkdirSync(path.join(root, "member/src"), { recursive: true });
    writeFileSync(path.join(root, "Cargo.toml"), '[workspace]\nmembers = ["member"]\nresolver = "2"\n');
    writeFileSync(
      path.join(root, "member/Cargo.toml"),
      '[package]\nname = "runner-probe"\nversion = "0.1.0"\nedition = "2021"\n',
    );
    writeFileSync(path.join(root, "member/src/lib.rs"), "#[test]\nfn probe() {}\n");
    const runner = path.join(root, "runner.cjs");
    const report = path.join(root, "report.json");
    writeFileSync(
      runner,
      `const {spawnSync} = require('node:child_process');
require('node:fs').writeFileSync(${JSON.stringify(report)}, JSON.stringify({cwd: process.cwd(), args: process.argv.slice(2)}));
const result = spawnSync(process.argv[2], process.argv.slice(3), {stdio: 'inherit'});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
`,
    );
    // Exercise Cargo's real TOML/argv handling with a portable stand-in for PowerShell.
    const failed = spawnSync(
      "cargo",
      [
        "test",
        "--workspace",
        "--lib",
        "--config",
        `target.'cfg(all())'.runner=${JSON.stringify([process.execPath, "runner.cjs"])}`,
      ],
      { cwd: root, encoding: "utf8" },
    );
    assert.ifError(failed.error);
    assert.notEqual(failed.status, 0, "A workspace-relative runner must fail from the member cwd");
    assert.match(failed.stderr, /Cannot find module/);
    const result = spawnSync(
      "cargo",
      [
        "test",
        "--workspace",
        "--lib",
        "--config",
        `target.'cfg(all())'.runner=${JSON.stringify([process.execPath, runner])}`,
        "--",
        "--exact",
        "probe",
      ],
      { cwd: root, encoding: "utf8" },
    );
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const actual = JSON.parse(readFileSync(report, "utf8"));
    assert.equal(realpathSync(actual.cwd), realpathSync(path.join(root, "member")));
    assert.ok(path.isAbsolute(actual.args[0]));
    assert.deepEqual(actual.args.slice(1), ["--exact", "probe"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
