#!/usr/bin/env node
// Build self-contained skill assets from the documentation source tree.

import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(here, "..");
const repoRoot = join(packageRoot, "..", "..");
const docsSkillsDir = join(repoRoot, "docs", "skills");
const apiDocsDir = join(repoRoot, "docs", "api");
const assetsDir = join(packageRoot, "assets");
const PUBLIC_SKILLS = [
  "rong-module-author",
  "rong-runtime-developer",
  "rong-runtime-embedder",
];

function fail(message) {
  console.error(`rong-skill pack: ${message}`);
  process.exit(1);
}

function listMarkdownFiles(dir) {
  return readdirSync(dir)
    .filter((file) => file.endsWith(".md"))
    .sort();
}

function assertSkillFrontmatter(skill, dir) {
  const skillFile = join(dir, "SKILL.md");
  if (!existsSync(skillFile)) fail(`${skill}: missing SKILL.md`);

  const body = readFileSync(skillFile, "utf8");
  if (!body.startsWith("---\n")) fail(`${skill}: SKILL.md missing frontmatter`);

  const end = body.indexOf("\n---", 4);
  if (end < 0) fail(`${skill}: SKILL.md frontmatter is not closed`);

  const frontmatter = body.slice(4, end);
  if (!/^name:\s*\S+/m.test(frontmatter)) fail(`${skill}: SKILL.md missing name`);
  if (!/^description:/m.test(frontmatter)) fail(`${skill}: SKILL.md missing description`);
}

function assertNoRuntimeApiCopies() {
  const refsDir = join(docsSkillsDir, "rong-runtime-developer", "references");
  const apiCopies = listMarkdownFiles(refsDir).filter(
    (file) => file.startsWith("api-") && file !== "api-index.md",
  );

  if (apiCopies.length) {
    fail(
      `runtime skill source must not duplicate docs/api files: ${apiCopies.join(", ")}`,
    );
  }
}

function copySkillSources(outDir) {
  for (const skill of PUBLIC_SKILLS) {
    const source = join(docsSkillsDir, skill);
    const target = join(outDir, skill);
    if (!existsSync(source)) fail(`missing docs source for ${skill}`);
    assertSkillFrontmatter(skill, source);
    cpSync(source, target, { recursive: true });
  }
}

function copyPublicApiReferences(outDir) {
  if (!existsSync(apiDocsDir)) fail("missing docs/api source directory");

  const refsDir = join(outDir, "rong-runtime-developer", "references");
  mkdirSync(refsDir, { recursive: true });

  const apiFiles = listMarkdownFiles(apiDocsDir);
  if (!apiFiles.length) fail("docs/api has no markdown files");

  for (const file of apiFiles) {
    const source = join(apiDocsDir, file);
    const target = join(refsDir, `api-${basename(file)}`);
    cpSync(source, target);
  }
}

function assertPackedAssets(outDir) {
  for (const skill of PUBLIC_SKILLS) {
    assertSkillFrontmatter(skill, join(outDir, skill));
  }

  const runtimeRefs = join(outDir, "rong-runtime-developer", "references");
  for (const file of listMarkdownFiles(apiDocsDir)) {
    const generated = join(runtimeRefs, `api-${basename(file)}`);
    if (!existsSync(generated)) fail(`missing generated runtime reference ${generated}`);
  }

  const moduleRefs = join(outDir, "rong-module-author", "references");
  if (!statSync(moduleRefs).isDirectory()) fail("missing module author references");

  const embedderRefs = join(outDir, "rong-runtime-embedder", "references");
  for (const file of ["interruption.md", "runtime-setup.md", "worker-pools.md"]) {
    if (!existsSync(join(embedderRefs, file))) {
      fail(`missing runtime embedder reference ${file}`);
    }
  }
}

function build(outDir) {
  assertNoRuntimeApiCopies();
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  copySkillSources(outDir);
  copyPublicApiReferences(outDir);
  assertPackedAssets(outDir);
}

const args = new Set(process.argv.slice(2));
if (args.has("--help") || args.has("-h")) {
  console.log(`Usage:
  node packages/skill/bin/pack.mjs          Generate packages/skill/assets from docs.
  node packages/skill/bin/pack.mjs --check  Validate generation without writing packages/skill/assets.
  node packages/skill/bin/pack.mjs --clean  Remove generated packages/skill/assets.`);
  process.exit(0);
}

if (!existsSync(docsSkillsDir)) {
  fail(`missing docs skill source directory: ${docsSkillsDir}`);
}

if (args.has("--clean")) {
  rmSync(assetsDir, { recursive: true, force: true });
  console.log(`removed ${assetsDir}`);
  process.exit(0);
}

if (args.has("--check")) {
  const temp = mkdtempSync(join(tmpdir(), "rong-skill-assets-"));
  build(temp);
  rmSync(temp, { recursive: true, force: true });
  console.log("skill assets check ok");
  process.exit(0);
}

build(assetsDir);
console.log(`generated ${assetsDir}`);
