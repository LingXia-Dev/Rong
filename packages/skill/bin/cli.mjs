#!/usr/bin/env node
// Installer for the Rong agent skills.
//
// Copies bundled skill folders into a Claude skills directory so Claude Code
// can discover it. No registration step is needed - a skill is just a folder
// with a SKILL.md.
//
//   npx @rongjs/rong-skill install            # ~/.claude/skills/<skill>... (personal)
//   npx @rongjs/rong-skill install --project  # ./.claude/skills/<skill>... (project)
//   npx @rongjs/rong-skill install --dir DIR  # DIR/<skill>...              (custom)
//   npx @rongjs/rong-skill install --skill rong-module-author
//   npx @rongjs/rong-skill install --skill rong-runtime-embedder
//   npx @rongjs/rong-skill install --force    # overwrite an existing copy
//   npx @rongjs/rong-skill list
//   npx @rongjs/rong-skill --help

import { cpSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const assetsDir = join(here, "..", "assets");
const PUBLIC_SKILLS = [
  "rong-module-author",
  "rong-runtime-developer",
  "rong-runtime-embedder",
];

function hasBundledAssets() {
  return PUBLIC_SKILLS.every((name) => existsSync(join(assetsDir, name, "SKILL.md")));
}

function ensureBundledAssets() {
  if (hasBundledAssets()) return;

  const packScript = join(here, "pack.mjs");
  const docsSkillsDir = join(here, "..", "..", "..", "docs", "skills");
  if (existsSync(packScript) && existsSync(docsSkillsDir)) {
    const result = spawnSync(process.execPath, [packScript], { stdio: "inherit" });
    if (result.status === 0 && hasBundledAssets()) return;
  }

  console.error(
    "rong-skill: bundled assets are missing. Run `node packages/skill/bin/pack.mjs` from a Rong source checkout, or reinstall the published package.",
  );
  process.exit(1);
}

function bundledSkills() {
  ensureBundledAssets();
  return PUBLIC_SKILLS.filter((name) => existsSync(join(assetsDir, name, "SKILL.md"))).sort();
}

function parseArgs(argv) {
  const opts = {
    command: "install",
    project: false,
    force: false,
    dir: null,
    skills: [],
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "install" || a === "list" || a === "help") opts.command = a;
    else if (a === "--project" || a === "-p") opts.project = true;
    else if (a === "--force" || a === "-f") opts.force = true;
    else if (a === "--dir") opts.dir = argv[++i];
    else if (a === "--skill" || a === "-s") opts.skills.push(argv[++i]);
    else if (a === "--help" || a === "-h") opts.command = "help";
    else {
      console.error(`rong-skill: unknown argument "${a}"\n`);
      opts.command = "help";
      opts.exitCode = 1;
    }
  }
  return opts;
}

function printHelp() {
  let version = "?";
  try {
    version = JSON.parse(readFileSync(join(here, "..", "package.json"), "utf8")).version;
  } catch {
    /* ignore */
  }
  console.log(`rong-skill v${version} - install Rong agent skills

Usage:
  npx @rongjs/rong-skill install [options]
  npx @rongjs/rong-skill list

Options:
  -p, --project     Install into ./.claude/skills (this project only)
      --dir <DIR>   Install into <DIR> (custom skills directory)
  -s, --skill NAME  Install one bundled skill instead of all skills
  -f, --force       Overwrite an existing installation
  -h, --help        Show this help

Default target: ~/.claude/skills (personal, available in all projects)

After installing, open your agent in a Rong project and ask it to help write
Rong scripts, modules, or Rust embedding hosts. Bundled skills:
  ${bundledSkills().join("\n  ")}`);
}

function resolveSkillsDir(opts) {
  if (opts.dir) return resolve(opts.dir);
  if (opts.project) return resolve(".claude", "skills");
  return join(homedir(), ".claude", "skills");
}

function printSkills() {
  for (const skill of bundledSkills()) {
    console.log(skill);
  }
}

function install(opts) {
  const available = bundledSkills();
  const selected = opts.skills.length ? opts.skills : available;
  const missing = selected.filter((skill) => !available.includes(skill));
  if (missing.length) {
    console.error(`rong-skill: unknown bundled skill(s): ${missing.join(", ")}`);
    console.error(`Available skills: ${available.join(", ")}`);
    process.exit(1);
  }

  const skillsDir = resolveSkillsDir(opts);
  mkdirSync(skillsDir, { recursive: true });

  for (const skill of selected) {
    const skillSource = join(assetsDir, skill);
    const target = join(skillsDir, skill);

    if (existsSync(target)) {
      if (!opts.force) {
        console.error(
          `rong-skill: ${target} already exists. Re-run with --force to overwrite.`,
        );
        process.exit(1);
      }
      rmSync(target, { recursive: true, force: true });
    }

    cpSync(skillSource, target, { recursive: true });
    console.log(`[ok] Installed "${skill}" skill to ${target}`);
  }

  console.log(`  Open your agent in a Rong project and ask it to use a Rong skill.`);
}

const opts = parseArgs(process.argv.slice(2));
if (opts.command === "help") {
  printHelp();
  process.exit(opts.exitCode ?? 0);
}
if (opts.command === "list") {
  printSkills();
  process.exit(0);
}
install(opts);
