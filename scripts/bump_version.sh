#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

WORKSPACE_TOML="Cargo.toml"
NPM_PACKAGE_JSONS=("packages/rong_types/package.json" "packages/skill/package.json")

CORE_CRATES=(
  "rong_typegen"
  "rong_macro"
  "rong_rt"
  "rong_core"
  "rong"
)

ENGINE_CRATES=(
  "rong_quickjs_sys"
  "rong_jscore_sys"
  "rong_arkjs_sys"
  "rong_quickjs"
  "rong_jscore"
  "rong_arkjs"
)

MODULE_CRATES=(
  "rong_console"
  "rong_assert"
  "rong_encoding"
  "rong_url"
  "rong_timer"
  "rong_cron"
  "rong_event"
  "rong_buffer"
  "rong_exception"
  "rong_abort"
  "rong_stream"
  "rong_fs"
  "rong_storage"
  "rong_http"
  "rong_compression"
  "rong_command"
  "rong_redis"
  "rong_sqlite"
  "rong_worker"
  "rong_s3"
)

BUNDLE_CRATES=(
  "rong_modules"
  "rong_cli"
)

usage() {
  cat << EOF
Usage: $0 <new-version> [OPTIONS]

Bump selected package versions. Rust crates are versioned independently; this
script does not update a global workspace version.

ARGUMENTS:
  <new-version>              New version number, e.g. 0.4.1

SELECTION:
  --crate NAME               Bump one Rust crate; repeatable
  --group NAME               Bump a Rust/npm group; repeatable
                             groups: core, engines, modules, bundles,
                                     non-modules, rust, npm, all
  --npm                      Bump all repo-maintained npm packages
  --npm-package PATH_OR_NAME Bump one npm package by package.json path or name

OPTIONS:
  --commit                   Create a git commit
  -y, --yes                  Skip confirmation prompt
  -h, --help                 Show this help message

EXAMPLES:
  $0 0.4.1 --crate rong_timer
  $0 0.4.1 --crate rong_jscore_sys --crate rong_jscore
  $0 0.4.1 --group modules
  $0 0.4.1 --group npm
  $0 0.4.1 --group all --commit
EOF
  exit 0
}

NEW_VERSION=""
DO_COMMIT=false
AUTO_CONFIRM=false
SELECTED_CRATES=()
SELECTED_NPM_JSONS=()

add_unique() {
  local value=$1
  shift
  local existing
  for existing in "$@"; do
    if [ "$existing" = "$value" ]; then
      return 1
    fi
  done
  return 0
}

add_crate() {
  local crate=$1
  if [ ${#SELECTED_CRATES[@]} -eq 0 ] || add_unique "$crate" "${SELECTED_CRATES[@]}"; then
    SELECTED_CRATES+=("$crate")
  fi
}

add_npm_json() {
  local package_json=$1
  if [ ${#SELECTED_NPM_JSONS[@]} -eq 0 ] || add_unique "$package_json" "${SELECTED_NPM_JSONS[@]}"; then
    SELECTED_NPM_JSONS+=("$package_json")
  fi
}

add_group() {
  local group=$1
  local crate
  case "$group" in
    core)
      for crate in "${CORE_CRATES[@]}"; do add_crate "$crate"; done
      ;;
    engines)
      for crate in "${ENGINE_CRATES[@]}"; do add_crate "$crate"; done
      ;;
    modules)
      for crate in "${MODULE_CRATES[@]}"; do add_crate "$crate"; done
      ;;
    bundles)
      for crate in "${BUNDLE_CRATES[@]}"; do add_crate "$crate"; done
      ;;
    non-modules)
      add_group core
      add_group engines
      add_group bundles
      ;;
    rust)
      add_group core
      add_group engines
      add_group modules
      add_group bundles
      ;;
    npm)
      for package_json in "${NPM_PACKAGE_JSONS[@]}"; do add_npm_json "$package_json"; done
      ;;
    all)
      add_group rust
      add_group npm
      ;;
    *)
      echo -e "${RED}Error: Unknown group: $group${NC}" >&2
      usage
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case $1 in
    -h|--help)
      usage
      ;;
    --commit)
      DO_COMMIT=true
      shift
      ;;
    -y|--yes)
      AUTO_CONFIRM=true
      shift
      ;;
    --crate)
      if [[ $# -lt 2 ]]; then
        echo "Error: --crate requires a crate name" >&2
        exit 1
      fi
      add_crate "$2"
      shift 2
      ;;
    --group)
      if [[ $# -lt 2 ]]; then
        echo "Error: --group requires a group name" >&2
        exit 1
      fi
      add_group "$2"
      shift 2
      ;;
    --npm)
      add_group npm
      shift
      ;;
    --npm-package)
      if [[ $# -lt 2 ]]; then
        echo "Error: --npm-package requires a package.json path or package name" >&2
        exit 1
      fi
      add_npm_json "$2"
      shift 2
      ;;
    -*)
      echo "Unknown option: $1" >&2
      usage
      ;;
    *)
      if [ -z "$NEW_VERSION" ]; then
        NEW_VERSION="$1"
      else
        echo "Error: Unexpected argument: $1" >&2
        usage
      fi
      shift
      ;;
  esac
done

if [ "${CI:-}" = "true" ]; then
  AUTO_CONFIRM=true
fi

if [ -z "$NEW_VERSION" ]; then
  echo -e "${RED}Error: New version is required${NC}" >&2
  usage
fi

if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?$ ]]; then
  echo -e "${RED}Error: Invalid version format: $NEW_VERSION${NC}" >&2
  exit 1
fi

if [ ${#SELECTED_CRATES[@]} -eq 0 ] && [ ${#SELECTED_NPM_JSONS[@]} -eq 0 ]; then
  echo -e "${RED}Error: No packages selected.${NC}" >&2
  echo "Select packages with --crate, --group, --npm, or --npm-package." >&2
  exit 1
fi

if [ "$DO_COMMIT" = true ] && ! git diff-index --quiet HEAD -- 2>/dev/null; then
  echo -e "${RED}Error: You have uncommitted changes${NC}" >&2
  exit 1
fi

METADATA_FILE="$(mktemp)"
trap 'rm -f "$METADATA_FILE"' EXIT
cargo metadata --no-deps --format-version 1 > "$METADATA_FILE"

manifest_for_crate() {
  node - "$METADATA_FILE" "$1" <<'NODE'
const fs = require("fs");
const [metadataPath, crate] = process.argv.slice(2);
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
const pkg = metadata.packages.find((item) => item.name === crate);
if (!pkg) process.exit(1);
process.stdout.write(pkg.manifest_path);
NODE
}

crate_version() {
  local manifest=$1
  awk '
    /^\[package\]/ { in_package = 1; next }
    /^\[/ && in_package { exit }
    in_package && /^version[[:space:]]*=/ {
      gsub(/"/, "", $3);
      print $3;
      exit
    }
  ' "$manifest"
}

npm_json_for_selector() {
  local selector=$1
  local package_json

  if [ -f "$selector" ]; then
    printf '%s' "$selector"
    return 0
  fi

  for package_json in "${NPM_PACKAGE_JSONS[@]}"; do
    if [ -f "$package_json" ] && [ "$(node -p "require('./${package_json}').name")" = "$selector" ]; then
      printf '%s' "$package_json"
      return 0
    fi
  done

  return 1
}

RESOLVED_NPM_JSONS=()
if [ ${#SELECTED_NPM_JSONS[@]} -gt 0 ]; then
  for package_json in "${SELECTED_NPM_JSONS[@]}"; do
    if resolved="$(npm_json_for_selector "$package_json")"; then
      if [ ${#RESOLVED_NPM_JSONS[@]} -eq 0 ] || add_unique "$resolved" "${RESOLVED_NPM_JSONS[@]}"; then
        RESOLVED_NPM_JSONS+=("$resolved")
      fi
    else
      echo -e "${RED}Error: Unknown npm package: $package_json${NC}" >&2
      exit 1
    fi
  done
fi

SELECTED_NPM_JSONS=()
if [ ${#RESOLVED_NPM_JSONS[@]} -gt 0 ]; then
  SELECTED_NPM_JSONS=("${RESOLVED_NPM_JSONS[@]}")
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  RongJS Package Version Bump${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "New version: ${GREEN}${NEW_VERSION}${NC}"
echo ""

if [ ${#SELECTED_CRATES[@]} -gt 0 ]; then
  echo -e "${BLUE}Rust crates:${NC}"
  for crate in "${SELECTED_CRATES[@]}"; do
    manifest="$(manifest_for_crate "$crate" || true)"
    if [ -z "$manifest" ]; then
      echo -e "${RED}Error: Unknown Rust crate: $crate${NC}" >&2
      exit 1
    fi
    echo "  - $crate ($(crate_version "$manifest") -> $NEW_VERSION)"
  done
fi

if [ ${#SELECTED_NPM_JSONS[@]} -gt 0 ]; then
  echo -e "${BLUE}npm packages:${NC}"
  for package_json in "${SELECTED_NPM_JSONS[@]}"; do
    package_name="$(node -p "require('./${package_json}').name")"
    package_version="$(node -p "require('./${package_json}').version")"
    echo "  - $package_name ($package_version -> $NEW_VERSION)"
  done
fi

echo ""
if [ "$AUTO_CONFIRM" != true ]; then
  read -p "Proceed with version bump? (yes/no): " -r
  echo
  if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
    echo "Aborted."
    exit 0
  fi
fi

FILES_TO_ADD=()

set_manifest_version() {
  local manifest=$1
  NEW_VERSION="$NEW_VERSION" perl -0pi -e '
    my $version = $ENV{NEW_VERSION};
    s/(\[package\][^\[]*?\nversion\s*=\s*")[^"]+(")/$1$version$2/s
      or die "could not update [package].version in $ARGV\n";
  ' "$manifest"
}

set_workspace_dependency_version() {
  local crate=$1
  CRATE="$crate" NEW_VERSION="$NEW_VERSION" perl -0pi -e '
    my $crate = $ENV{CRATE};
    my $version = $ENV{NEW_VERSION};
    s{(\[workspace\.dependencies\]\n)(.*?)(?=\n\[|\z)}{
      my ($heading, $body) = ($1, $2);
      $body =~ s{(^\Q$crate\E\s*=\s*\{[^\n]*)(\})}{
        my ($line, $close) = ($1, $2);
        if ($line =~ /version\s*=\s*"[^"]*"/) {
          $line =~ s/version\s*=\s*"[^"]*"/version = "$version"/;
        } else {
          $line =~ s/\s+$//;
          $line .= qq{, version = "$version"};
        }
        $line . $close;
      }mge;
      $heading . $body;
    }se;
  ' "$WORKSPACE_TOML"
}
if [ ${#SELECTED_CRATES[@]} -gt 0 ]; then
  for crate in "${SELECTED_CRATES[@]}"; do
    manifest="$(manifest_for_crate "$crate")"
    set_manifest_version "$manifest"
    set_workspace_dependency_version "$crate"
    if [ ${#FILES_TO_ADD[@]} -eq 0 ] || add_unique "$manifest" "${FILES_TO_ADD[@]}"; then
      FILES_TO_ADD+=("$manifest")
    fi
    if [ "$manifest" != "$WORKSPACE_TOML" ]; then
      if [ ${#FILES_TO_ADD[@]} -eq 0 ] || add_unique "$WORKSPACE_TOML" "${FILES_TO_ADD[@]}"; then
        FILES_TO_ADD+=("$WORKSPACE_TOML")
      fi
    fi
    echo -e "${GREEN}✓ Updated $crate to $NEW_VERSION${NC}"
  done
fi

if [ ${#SELECTED_NPM_JSONS[@]} -gt 0 ]; then
  for package_json in "${SELECTED_NPM_JSONS[@]}"; do
    node - "$package_json" "$NEW_VERSION" <<'NODE'
const fs = require("fs");
const [file, version] = process.argv.slice(2);
const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
pkg.version = version;
fs.writeFileSync(file, `${JSON.stringify(pkg, null, 2)}\n`);
NODE
    if [ ${#FILES_TO_ADD[@]} -eq 0 ] || add_unique "$package_json" "${FILES_TO_ADD[@]}"; then
      FILES_TO_ADD+=("$package_json")
    fi
    echo -e "${GREEN}✓ Updated $package_json to $NEW_VERSION${NC}"

    package_lock="$(dirname "$package_json")/package-lock.json"
    if [ -f "$package_lock" ]; then
      node - "$package_lock" "$NEW_VERSION" <<'NODE'
const fs = require("fs");
const [file, version] = process.argv.slice(2);
const lock = JSON.parse(fs.readFileSync(file, "utf8"));
lock.version = version;
if (lock.packages?.[""]) lock.packages[""].version = version;
fs.writeFileSync(file, `${JSON.stringify(lock, null, 2)}\n`);
NODE
      if [ ${#FILES_TO_ADD[@]} -eq 0 ] || add_unique "$package_lock" "${FILES_TO_ADD[@]}"; then
        FILES_TO_ADD+=("$package_lock")
      fi
      echo -e "${GREEN}✓ Updated $package_lock to $NEW_VERSION${NC}"
    fi
  done
fi

if [ "$DO_COMMIT" = true ]; then
  git add "${FILES_TO_ADD[@]}"
  git commit -m "chore: bump selected packages to ${NEW_VERSION}"
  echo -e "${GREEN}✓ Created commit${NC}"
fi

echo ""
echo -e "${GREEN}Version bump complete.${NC}"
echo -e "${YELLOW}Next steps:${NC}"
echo -e "  1. Review: ${BLUE}git diff -- ${FILES_TO_ADD[*]}${NC}"
echo -e "  2. Update CHANGELOG.md for the packages being released"
echo -e "  3. Publish with scripts/publish.sh using matching --crate/--group selection"
