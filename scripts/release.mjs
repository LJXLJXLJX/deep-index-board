#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = dirname(dirname(fileURLToPath(import.meta.url)));
const bump = process.argv[2] ?? "patch";

const paths = {
  packageJson: join(rootDir, "package.json"),
  cargoToml: join(rootDir, "src-tauri", "Cargo.toml"),
  tauriConfig: join(rootDir, "src-tauri", "tauri.conf.json"),
  changelog: join(rootDir, "CHANGELOG.md"),
};

function run(command, args, options = {}) {
  execFileSync(command, args, {
    cwd: options.cwd ?? rootDir,
    env: { ...process.env, ...(options.env ?? {}) },
    stdio: "inherit",
  });
}

function output(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: options.cwd ?? rootDir,
    encoding: "utf8",
  }).trim();
}

function assertCleanWorktree() {
  const status = output("git", ["status", "--porcelain"]);
  if (status) {
    throw new Error("Release requires a clean worktree. Commit or stash changes first.");
  }
}

function parseVersion(version) {
  const match = version.match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!match) {
    throw new Error(`Unsupported version format: ${version}`);
  }
  return match.slice(1).map(Number);
}

function nextVersion(current, releaseType) {
  if (/^v?\d+\.\d+\.\d+$/.test(releaseType)) {
    return releaseType.replace(/^v/, "");
  }

  const [major, minor, patch] = parseVersion(current);
  if (releaseType === "major") return `${major + 1}.0.0`;
  if (releaseType === "minor") return `${major}.${minor + 1}.0`;
  if (releaseType === "patch") return `${major}.${minor}.${patch + 1}`;
  throw new Error("Usage: npm run release -- <major|minor|patch|x.y.z>");
}

function replaceOnce(content, pattern, replacement, label) {
  if (!pattern.test(content)) {
    throw new Error(`Could not update ${label}`);
  }
  return content.replace(pattern, replacement);
}

function updatePackageJson(version) {
  const packageJson = JSON.parse(readFileSync(paths.packageJson, "utf8"));
  packageJson.version = version;
  writeFileSync(paths.packageJson, `${JSON.stringify(packageJson, null, 2)}\n`);
}

function updateCargoToml(version) {
  const cargoToml = readFileSync(paths.cargoToml, "utf8");
  const updated = replaceOnce(
    cargoToml,
    /^version = ".+"$/m,
    `version = "${version}"`,
    "Cargo.toml version"
  );
  writeFileSync(paths.cargoToml, updated);
}

function updateTauriConfig(version) {
  const tauriConfig = JSON.parse(readFileSync(paths.tauriConfig, "utf8"));
  tauriConfig.version = version;
  writeFileSync(paths.tauriConfig, `${JSON.stringify(tauriConfig, null, 2)}\n`);
}

function updateChangelog(version) {
  const changelog = readFileSync(paths.changelog, "utf8");
  const today = new Date().toISOString().slice(0, 10);
  const releasedHeading = `## v${version} - ${today}`;
  const updated = replaceOnce(
    changelog,
    /^## Unreleased$/m,
    `## Unreleased\n\n- _Nothing yet._\n\n${releasedHeading}`,
    "CHANGELOG.md Unreleased heading"
  );
  writeFileSync(paths.changelog, updated);
}

function main() {
  assertCleanWorktree();

  const packageJson = JSON.parse(readFileSync(paths.packageJson, "utf8"));
  const version = nextVersion(packageJson.version, bump);
  const tag = `v${version}`;

  if (output("git", ["tag", "--list", tag])) {
    throw new Error(`Tag already exists: ${tag}`);
  }

  updatePackageJson(version);
  updateCargoToml(version);
  updateTauriConfig(version);
  updateChangelog(version);

  run("cargo", ["check"], { cwd: join(rootDir, "src-tauri") });
  run("npm", ["run", "build"]);
  run("cargo", ["check"], {
    cwd: join(rootDir, "src-tauri"),
    env: { RUSTFLAGS: "-Dwarnings" },
  });
  run("git", ["diff", "--check"]);

  run("git", [
    "add",
    "CHANGELOG.md",
    "package.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json",
  ]);
  run("git", ["commit", "-m", `chore: release ${tag}`]);
  run("git", ["tag", tag]);

  console.log(`Released ${tag}`);
}

main();
