# @rongjs/rong-skill

Installer and packaging tools for Rong agent skills.

The source skills live under `../../docs/skills/`. Public runtime API references
come from `../../docs/api/`. This package generates self-contained install assets
under `assets/` when packed or when the CLI is run from a source checkout.

## Bundled Skills

- `rong-runtime-developer` - write Rong JavaScript scripts, choose public APIs,
  adapt examples, run `rong_cli`, and compile bytecode.
- `rong-module-author` - write or edit Rust modules that expose Rong APIs,
  classes, functions, type conversions, and JavaScript errors.
- `rong-runtime-embedder` - build Rust hosts around Rong runtimes, module
  policies, worker pools, timeouts, interruption, and lifecycle management.

## Install

Install all bundled skills:

```bash
npx @rongjs/rong-skill install
```

Project-local install:

```bash
npx @rongjs/rong-skill install --project
```

Install one skill:

```bash
npx @rongjs/rong-skill install --skill rong-module-author
```

Install the Rust host embedding skill:

```bash
npx @rongjs/rong-skill install --skill rong-runtime-embedder
```

Custom skills directory:

```bash
npx @rongjs/rong-skill install --dir /path/to/skills
```

Overwrite existing installed copies:

```bash
npx @rongjs/rong-skill install --force
```

List bundled skills:

```bash
npx @rongjs/rong-skill list
```

## Layout

Source of truth:

```text
../../docs/
|-- api/
`-- skills/
    |-- rong-module-author/
    |-- rong-runtime-developer/
    `-- rong-runtime-embedder/
```

Generated package assets:

```text
assets/
|-- rong-module-author/
|   |-- SKILL.md
|   `-- references/
|       |-- classes.md
|       |-- errors.md
|       |-- functions.md
|       |-- module-structure.md
|       `-- type-conversion.md
|-- rong-runtime-developer/
|   |-- SKILL.md
|   `-- references/
|       |-- api-*.md
|       |-- api-index.md
|       |-- examples.md
|       `-- quickstart.md
`-- rong-runtime-embedder/
    |-- SKILL.md
    `-- references/
        |-- interruption.md
        |-- runtime-setup.md
        `-- worker-pools.md
```

Each installed skill follows the standard `SKILL.md` + optional `references/`
structure. `assets/` is generated and is not the documentation source of truth.

## Development

Validate skill packaging without writing `assets/`:

```bash
npm --prefix packages/skill run check
```

Generate `assets/` explicitly:

```bash
npm --prefix packages/skill run pack:skills
```

Remove generated `assets/`:

```bash
node packages/skill/bin/pack.mjs --clean
```

## Requirements

- Node.js >= 18 for the installer.
- An agent runtime that supports file-based skills.

## License

MIT OR Apache-2.0
