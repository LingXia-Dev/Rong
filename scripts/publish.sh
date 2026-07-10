#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

NO_VERIFY=false
ALLOW_DIRTY=false
AUTO_CONFIRM=false
WAIT_TIMEOUT=180
POLL_INTERVAL=5
START_FROM=""
SKIP_PUBLISHED_CHECK=false
VERIFY_FEATURES="quickjs"
CREATE_TAGS=false
CHANGED_SINCE=""
CHANGED_MODE=false
CHECK_DRIFT=false
DRY_RUN=false

WORKSPACE_TOML="Cargo.toml"

# Publishable crates in dependency order.
CRATES=(
  "rong_typedef"
  "rong_typegen"
  "rong_macro"
  "rong_rt"
  "rong_core"

  "rong_quickjs_sys"
  "rong_jscore_sys"
  "rong_arkjs_sys"

  "rong_quickjs"
  "rong_jscore"
  "rong_arkjs"

  "rong"

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

  "rong_modules"
  "rong_cli"
)

CORE_CRATES=(
  "rong_typedef"
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

SELECTED_CRATES=()
HAS_SELECTOR=false

usage() {
  cat << EOF
Usage: $0 [OPTIONS]

Publish selected Rust crates to crates.io in dependency order.

SELECTION:
  --crate NAME               Publish one crate; repeatable
  --group NAME               Publish a crate group; repeatable
                             groups: core, engines, modules, bundles,
                                     non-modules, rust, all
  --changed                  Add crates changed since their own latest package
                             tag (<crate>-vX.Y.Z), falling back to the latest
                             repo tag (vX.Y.Z). Internal version churn (the
                             crate's own version and rong* dependency version
                             syncs) is ignored; external dependency version
                             changes still count.
  --changed-since REF        Add crates with files changed since REF, with the
                             same internal version churn filtering.
  -s, --start-from NAME      Slice the selected plan from NAME, inclusive

PUBLISH OPTIONS:
  -n, --no-verify            Skip cargo publish verification
  -a, --allow-dirty          Allow publishing with uncommitted changes
      --skip-published-check Skip crates.io pre-check for existing versions
      --verify-features LIST Cargo features used for crates that need an engine
                             feature during publish verification (default: quickjs)
      --tag                  Create and push <crate>-v<version> tags for
                             published or already-published selected crates
      --dry-run              Print the publish plan without requiring tokens or
                             publishing anything
      --check-drift          With --dry-run: exit 2 when a selected crate's
                             current version is already on crates.io (it needs
                             a version bump before it can be published)
  -y, --yes                  Skip confirmation prompt
  -t, --timeout SECONDS      Maximum wait time for crates.io sync (default: 180)
  -p, --poll SECONDS         Poll interval for crates.io sync (default: 5)
  -h, --help                 Show this help message

EXAMPLES:
  $0 --crate rong_timer
  $0 --crate rong_jscore_sys --crate rong_jscore
  $0 --group engines
  $0 --group modules --tag
  $0 --changed --dry-run
  $0 --changed-since v0.4.0
EOF
  exit 0
}

contains() {
  local needle=$1
  shift
  local value
  for value in "$@"; do
    if [ "$value" = "$needle" ]; then
      return 0
    fi
  done
  return 1
}

is_publishable_crate() {
  contains "$1" "${CRATES[@]}"
}

add_crate() {
  local crate=$1
  HAS_SELECTOR=true
  if ! is_publishable_crate "$crate"; then
    echo -e "${RED}ERROR: unknown or non-publishable crate: $crate${NC}" >&2
    echo "Available crates:" >&2
    printf '  - %s\n' "${CRATES[@]}" >&2
    exit 1
  fi
  if [ ${#SELECTED_CRATES[@]} -eq 0 ] || ! contains "$crate" "${SELECTED_CRATES[@]}"; then
    SELECTED_CRATES+=("$crate")
  fi
}

add_group() {
  local group=$1
  local crate
  HAS_SELECTOR=true
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
    rust|all)
      for crate in "${CRATES[@]}"; do add_crate "$crate"; done
      ;;
    *)
      echo -e "${RED}ERROR: unknown group: $group${NC}" >&2
      usage
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case $1 in
    -n|--no-verify)
      NO_VERIFY=true
      shift
      ;;
    -a|--allow-dirty)
      ALLOW_DIRTY=true
      shift
      ;;
    -s|--start-from)
      if [[ $# -lt 2 ]]; then
        echo "Error: --start-from requires a crate name" >&2
        exit 1
      fi
      START_FROM="$2"
      shift 2
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
    --changed)
      HAS_SELECTOR=true
      CHANGED_MODE=true
      shift
      ;;
    --changed-since)
      if [[ $# -lt 2 ]]; then
        echo "Error: --changed-since requires a git ref" >&2
        exit 1
      fi
      HAS_SELECTOR=true
      CHANGED_SINCE="$2"
      shift 2
      ;;
    --check-drift)
      CHECK_DRIFT=true
      shift
      ;;
    --skip-published-check)
      SKIP_PUBLISHED_CHECK=true
      shift
      ;;
    --verify-features)
      if [[ $# -lt 2 ]]; then
        echo "Error: --verify-features requires a feature list" >&2
        exit 1
      fi
      VERIFY_FEATURES="$2"
      shift 2
      ;;
    --tag)
      CREATE_TAGS=true
      shift
      ;;
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    -y|--yes)
      AUTO_CONFIRM=true
      shift
      ;;
    -t|--timeout)
      WAIT_TIMEOUT="$2"
      shift 2
      ;;
    -p|--poll)
      POLL_INTERVAL="$2"
      shift 2
      ;;
    -h|--help)
      usage
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      ;;
  esac
done

if [ "${CI:-}" = "true" ]; then
  AUTO_CONFIRM=true
fi

METADATA_FILE="$(mktemp)"
trap 'rm -f "$METADATA_FILE"' EXIT
cargo metadata --no-deps --format-version 1 > "$METADATA_FILE"

workspace_root() {
  node - "$METADATA_FILE" <<'NODE'
const fs = require("fs");
const [metadataPath] = process.argv.slice(2);
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
process.stdout.write(metadata.workspace_root);
NODE
}

WORKSPACE_ROOT="$(workspace_root)"

# The CRATES list is maintained by hand; fail fast when it drifts from the
# workspace so a new publishable crate cannot be silently left unpublished.
validate_crate_list() {
  local meta_crates name
  local missing=()
  local extra=()

  meta_crates="$(node - "$METADATA_FILE" <<'NODE'
const fs = require("fs");
const [metadataPath] = process.argv.slice(2);
const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
const members = new Set(metadata.workspace_members);
for (const pkg of metadata.packages) {
  if (!members.has(pkg.id)) continue;
  if (Array.isArray(pkg.publish) && pkg.publish.length === 0) continue;
  console.log(pkg.name);
}
NODE
)"

  while IFS= read -r name; do
    [ -n "$name" ] || continue
    if ! contains "$name" "${CRATES[@]}"; then
      missing+=("$name")
    fi
  done <<< "$meta_crates"

  for name in "${CRATES[@]}"; do
    if ! grep -qx "$name" <<< "$meta_crates"; then
      extra+=("$name")
    fi
  done

  if [ ${#missing[@]} -gt 0 ] || [ ${#extra[@]} -gt 0 ]; then
    if [ ${#missing[@]} -gt 0 ]; then
      echo -e "${RED}ERROR: publishable workspace crates missing from the CRATES list in scripts/publish.sh:${NC}" >&2
      printf '  - %s\n' "${missing[@]}" >&2
    fi
    if [ ${#extra[@]} -gt 0 ]; then
      echo -e "${RED}ERROR: CRATES entries in scripts/publish.sh that are not publishable workspace members:${NC}" >&2
      printf '  - %s\n' "${extra[@]}" >&2
    fi
    exit 1
  fi
}

validate_crate_list

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

# Crate directories (relative to the workspace root), parallel to CRATES.
# Computed once because manifest_for_crate shells out to node per call.
CRATE_DIRS_REL=()
precompute_crate_dirs() {
  local crate manifest

  if [ ${#CRATE_DIRS_REL[@]} -gt 0 ]; then
    return 0
  fi
  for crate in "${CRATES[@]}"; do
    manifest="$(manifest_for_crate "$crate")"
    CRATE_DIRS_REL+=("$(dirname "${manifest#$WORKSPACE_ROOT/}")")
  done
}

crate_dir_for() {
  local i

  for ((i = 0; i < ${#CRATES[@]}; i++)); do
    if [ "${CRATES[$i]}" = "$1" ]; then
      printf '%s' "${CRATE_DIRS_REL[$i]}"
      return 0
    fi
  done
  return 1
}

# Crate directories nest (e.g. quickjs/ contains quickjs/sys/). Files that
# belong to a nested crate must not mark the enclosing crate as changed.
file_belongs_to_nested_crate() {
  local dir=$1
  local file=$2
  local i other_dir

  for ((i = 0; i < ${#CRATES[@]}; i++)); do
    other_dir="${CRATE_DIRS_REL[$i]}"
    [ "$other_dir" = "$dir" ] && continue
    case "$other_dir" in
      "$dir"/*)
        case "$file" in
          "$other_dir"/*) return 0 ;;
        esac
        ;;
    esac
  done
  return 1
}

# Baseline ref for --changed: the crate's own latest package tag, falling back
# to the latest repo-level release tag. Empty when neither exists.
latest_tag_for_crate() {
  local crate=$1
  local tag

  tag="$(git tag --list "${crate}-v*" --sort=-v:refname | head -n 1)"
  if [ -z "$tag" ]; then
    tag="$(git tag --list "v[0-9]*" --sort=-v:refname | head -n 1)"
  fi
  printf '%s' "$tag"
}

# True when REF..HEAD changes to a manifest only touch `version = "..."`
# values. bump_version.sh syncs versions across [workspace.dependencies], so
# version-only churn must not count as a publishable change.
# Strip only internal version-bump churn so it does not count as a change:
# the crate's own version in [package] (including the legacy
# `version.workspace = true` form), and version fields on internal `rong*`
# dependency entries, which bump_version.sh syncs across
# [workspace.dependencies]. External dependency version changes (e.g. tokio)
# are real changes and must survive normalization.
normalize_manifest_versions() {
  awk '
    /^\[/ { section = $0 }
    {
      line = $0

      # Own package version (or its workspace form), including the shared
      # version under [workspace.package]: drop the whole line.
      if (section ~ /^\[(workspace\.)?package\]/ \
          && line ~ /^[[:space:]]*version([[:space:]]*=|\.workspace)/) {
        next
      }

      # Internal rong* dependency entries: strip just the version field.
      if (line ~ /^[[:space:]]*"?rong[a-z0-9_]*"?[[:space:]]*=/ \
          || section ~ /^\[.*dependencies\.rong[a-z0-9_]*\]/) {
        gsub(/version[[:space:]]*=[[:space:]]*"[^"]*"/, "", line)
        gsub(/version\.workspace[[:space:]]*=[[:space:]]*true/, "", line)
      }

      # Tidy leftover commas/whitespace so both sides compare cleanly.
      gsub(/[[:space:]]+/, " ", line)
      gsub(/, *,/, ",", line)
      gsub(/\{ *,/, "{ ", line)
      gsub(/, *\}/, " }", line)
      sub(/^ +/, "", line)
      sub(/ +$/, "", line)
      sub(/,+ *$/, "", line)
      if (line == "") next
      print line
    }
  '
}

is_version_only_manifest_change() {
  local ref=$1
  local file=$2
  local old new

  old="$(git show "${ref}:${file}" 2>/dev/null | normalize_manifest_versions)" || return 1
  new="$(git show "HEAD:${file}" 2>/dev/null | normalize_manifest_versions)" || return 1
  [ "$old" = "$new" ]
}

# True when CRATE has publish-relevant changes in REF..HEAD. For the root
# crate, only its manifest, README, and src/ count; CHANGELOG.md is
# workspace-wide and intentionally ignored.
crate_changed_since() {
  local crate=$1
  local ref=$2
  local dir rel file

  dir="$(crate_dir_for "$crate")"
  rel="${dir}/Cargo.toml"

  if [ "$dir" = "." ]; then
    while IFS= read -r file; do
      [ -n "$file" ] || continue
      case "$file" in
        Cargo.toml)
          is_version_only_manifest_change "$ref" "$file" || return 0
          ;;
        README.md|src/*)
          return 0
          ;;
      esac
    done < <(git diff --name-only "$ref"..HEAD --)
    return 1
  fi

  while IFS= read -r file; do
    [ -n "$file" ] || continue
    if file_belongs_to_nested_crate "$dir" "$file"; then
      continue
    fi
    if [ "$file" = "$rel" ]; then
      is_version_only_manifest_change "$ref" "$file" || return 0
    else
      return 0
    fi
  done < <(git diff --name-only "$ref"..HEAD -- "$dir/")
  return 1
}

add_changed_since_selection() {
  local ref=$1
  local crate

  precompute_crate_dirs
  for crate in "${CRATES[@]}"; do
    if crate_changed_since "$crate" "$ref"; then
      add_crate "$crate"
    fi
  done
}

add_changed_selection() {
  local crate baseline

  precompute_crate_dirs
  for crate in "${CRATES[@]}"; do
    baseline="$(latest_tag_for_crate "$crate")"
    if [ -z "$baseline" ]; then
      echo -e "${YELLOW}No release tag found for ${crate}; treating it as changed.${NC}" >&2
      add_crate "$crate"
      continue
    fi
    if crate_changed_since "$crate" "$baseline"; then
      add_crate "$crate"
    fi
  done
}

# Exact published-version lookup via the crates.io sparse index. `cargo search`
# only reports the latest version, so it cannot answer "is X.Y.Z published".
sparse_index_path() {
  local crate
  crate="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
  case "${#crate}" in
    1) printf '1/%s' "$crate" ;;
    2) printf '2/%s' "$crate" ;;
    3) printf '3/%s/%s' "${crate:0:1}" "$crate" ;;
    *) printf '%s/%s/%s' "${crate:0:2}" "${crate:2:2}" "$crate" ;;
  esac
}

is_crate_published() {
  local crate=$1
  local version=$2
  local url body status

  url="https://index.crates.io/$(sparse_index_path "$crate")"
  status=0
  body="$(curl -fsSL --retry 3 --retry-delay 2 "$url" 2>/dev/null)" || status=$?

  if [ "$status" -eq 22 ]; then
    # HTTP error response; 404 means the crate has never been published.
    return 1
  fi
  if [ "$status" -ne 0 ]; then
    # Network trouble: fall back to cargo search (latest version only).
    cargo search "$crate" --limit 1 2>/dev/null | grep -q "^$crate = \"${version}\""
    return
  fi
  grep -q "\"vers\":\"${version}\"" <<< "$body"
}

if [ -n "$CHANGED_SINCE" ]; then
  add_changed_since_selection "$CHANGED_SINCE"
fi

if [ "$CHANGED_MODE" = true ]; then
  add_changed_selection
fi

if [ "$HAS_SELECTOR" = false ]; then
  echo -e "${YELLOW}No crate selection provided; defaulting to all publishable Rust crates.${NC}"
  for crate in "${CRATES[@]}"; do add_crate "$crate"; done
fi

# Reorder the selection according to the topological publish order.
ORDERED_SELECTION=()
for crate in "${CRATES[@]}"; do
  if [ ${#SELECTED_CRATES[@]} -gt 0 ] && contains "$crate" "${SELECTED_CRATES[@]}"; then
    ORDERED_SELECTION+=("$crate")
  fi
done
PUBLISH_CRATES=()
if [ ${#ORDERED_SELECTION[@]} -gt 0 ]; then
  PUBLISH_CRATES=("${ORDERED_SELECTION[@]}")
fi

if [ -n "$START_FROM" ]; then
  FOUND_START=false
  SLICED=()
  if [ ${#PUBLISH_CRATES[@]} -gt 0 ]; then
    for crate in "${PUBLISH_CRATES[@]}"; do
      if [ "$crate" = "$START_FROM" ]; then
        FOUND_START=true
      fi
      if [ "$FOUND_START" = true ]; then
        SLICED+=("$crate")
      fi
    done
  fi
  if [ "$FOUND_START" = false ]; then
    echo -e "${RED}ERROR: start crate not found in selected publish plan: ${START_FROM}${NC}" >&2
    if [ ${#PUBLISH_CRATES[@]} -gt 0 ]; then
      printf '  - %s\n' "${PUBLISH_CRATES[@]}" >&2
    fi
    exit 1
  fi
  PUBLISH_CRATES=()
  if [ ${#SLICED[@]} -gt 0 ]; then
    PUBLISH_CRATES=("${SLICED[@]}")
  fi
fi

TOTAL_CRATES=${#PUBLISH_CRATES[@]}
if [ "$TOTAL_CRATES" -eq 0 ]; then
  echo -e "${RED}ERROR: publish plan is empty.${NC}" >&2
  exit 1
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  RongJS Rust Publishing Script${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "Total crates to publish: ${GREEN}${TOTAL_CRATES}${NC}"
if [ "$CHANGED_MODE" = true ]; then
  echo -e "Changed since: ${YELLOW}per-crate package tags${NC}"
fi
if [ -n "$CHANGED_SINCE" ]; then
  echo -e "Changed since: ${YELLOW}${CHANGED_SINCE}${NC}"
fi
if [ -n "$START_FROM" ]; then
  echo -e "Start from: ${YELLOW}${START_FROM}${NC}"
fi
echo -e "Skip published pre-check: ${YELLOW}${SKIP_PUBLISHED_CHECK}${NC}"
echo -e "Verify features: ${YELLOW}${VERIFY_FEATURES}${NC}"
echo -e "No verify: ${YELLOW}${NO_VERIFY}${NC}"
echo -e "Allow dirty: ${YELLOW}${ALLOW_DIRTY}${NC}"
echo -e "Create crate tags: ${YELLOW}${CREATE_TAGS}${NC}"
echo -e "Auto confirm: ${YELLOW}${AUTO_CONFIRM}${NC}"
echo -e "Sync timeout: ${YELLOW}${WAIT_TIMEOUT}s${NC}"
echo -e "Poll interval: ${YELLOW}${POLL_INTERVAL}s${NC}"
echo ""
echo -e "${BLUE}Publish plan:${NC}"
for crate in "${PUBLISH_CRATES[@]}"; do
  manifest="$(manifest_for_crate "$crate")"
  if [ "$CHANGED_MODE" = true ]; then
    echo "  - $crate $(crate_version "$manifest") (baseline: $(latest_tag_for_crate "$crate"))"
  else
    echo "  - $crate $(crate_version "$manifest")"
  fi
done
echo ""

if [ "$DRY_RUN" = true ]; then
  DRIFT=0
  for crate in "${PUBLISH_CRATES[@]}"; do
    manifest="$(manifest_for_crate "$crate")"
    version="$(crate_version "$manifest")"
    if is_crate_published "$crate" "$version"; then
      echo -e "${YELLOW}drift: ${crate} ${version} is already on crates.io; bump its version before publishing.${NC}"
      DRIFT=$((DRIFT + 1))
    fi
  done
  if [ "$DRIFT" -gt 0 ]; then
    echo -e "${YELLOW}${DRIFT} selected crate(s) need a version bump before they can publish.${NC}"
    if [ "$CHECK_DRIFT" = true ]; then
      exit 2
    fi
  fi
  echo -e "${GREEN}Dry run complete; no crates were published.${NC}"
  exit 0
fi

if [ "$CREATE_TAGS" = true ] && [ "$ALLOW_DIRTY" != true ] && ! git diff-index --quiet HEAD -- 2>/dev/null; then
  echo -e "${RED}ERROR: --tag requires a clean tracked working tree unless --allow-dirty is set.${NC}" >&2
  exit 1
fi

if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  echo -e "${RED}ERROR: CARGO_REGISTRY_TOKEN is not set${NC}" >&2
  echo "Please set it with: export CARGO_REGISTRY_TOKEN=your_token" >&2
  exit 1
fi

if [ "$AUTO_CONFIRM" = true ]; then
  echo -e "${YELLOW}Auto-confirm enabled; proceeding with publish.${NC}"
else
  read -p "Are you sure you want to continue? (yes/no): " -r
  echo
  if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
    echo "Aborted."
    exit 1
  fi
fi

needs_engine_verify_features() {
  local crate=$1
  case "$crate" in
    rong|rong_console|rong_assert|rong_encoding|rong_url|rong_timer|rong_cron|rong_event|rong_buffer|rong_exception|rong_abort|rong_stream|rong_fs|rong_storage|rong_http|rong_compression|rong_command|rong_redis|rong_sqlite|rong_worker|rong_s3|rong_modules|rong_cli)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

wait_for_crate() {
  local crate=$1
  local version=$2
  local timeout=$3
  local poll_interval=$4
  local elapsed=0

  echo -e "${YELLOW}Waiting for ${crate} ${version} to appear in crates.io index...${NC}"
  while [ "$elapsed" -lt "$timeout" ]; do
    if is_crate_published "$crate" "$version"; then
      echo -e "${GREEN}Found ${crate} ${version} in index after ${elapsed}s${NC}"
      return 0
    fi

    sleep "$poll_interval"
    elapsed=$((elapsed + poll_interval))
    echo -e "${YELLOW}  Still waiting... (${elapsed}s/${timeout}s)${NC}"
  done

  echo -e "${YELLOW}Timeout waiting for ${crate} ${version}; proceeding anyway.${NC}"
  return 1
}

ensure_crate_tag() {
  local crate=$1
  local version=$2
  local tag="${crate}-v${version}"

  if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    echo -e "${YELLOW}Tag ${tag} already exists locally.${NC}"
    return 0
  fi

  if git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1; then
    echo -e "${YELLOW}Tag ${tag} already exists on origin.${NC}"
    return 0
  fi

  git tag -a "$tag" -m "${crate} v${version}"
  git push origin "$tag"
  echo -e "${GREEN}Created tag ${tag}${NC}"
}

PUBLISHED=0
SKIPPED=0
FAILED=0

for crate in "${PUBLISH_CRATES[@]}"; do
  manifest="$(manifest_for_crate "$crate")"
  version="$(crate_version "$manifest")"

  echo ""
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BLUE}Publishing [$((PUBLISHED + SKIPPED + 1))/${TOTAL_CRATES}]: ${GREEN}${crate} ${version}${NC}"
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

  if [ "$SKIP_PUBLISHED_CHECK" = false ] && is_crate_published "$crate" "$version"; then
    echo -e "${YELLOW}Skipping ${crate} ${version}; already published.${NC}"
    SKIPPED=$((SKIPPED + 1))
    if [ "$CREATE_TAGS" = true ]; then
      ensure_crate_tag "$crate" "$version"
    fi
    continue
  fi

  cmd=(cargo publish -p "$crate")

  if [ "$NO_VERIFY" = true ]; then
    cmd+=(--no-verify)
  elif [ -n "$VERIFY_FEATURES" ] && needs_engine_verify_features "$crate"; then
    cmd+=(--features "$VERIFY_FEATURES")
  fi

  if [ "$ALLOW_DIRTY" = true ]; then
    cmd+=(--allow-dirty)
  fi

  echo -e "${YELLOW}Running: ${cmd[*]}${NC}"

  if publish_output="$("${cmd[@]}" 2>&1)"; then
    printf '%s\n' "$publish_output"
    PUBLISHED=$((PUBLISHED + 1))
    echo -e "${GREEN}Successfully published ${crate} ${version}${NC}"

    if [ "$CREATE_TAGS" = true ]; then
      ensure_crate_tag "$crate" "$version"
    fi

    if [ $((PUBLISHED + SKIPPED)) -lt "$TOTAL_CRATES" ]; then
      if ! wait_for_crate "$crate" "$version" "$WAIT_TIMEOUT" "$POLL_INTERVAL"; then
        echo -e "${YELLOW}Proceeding despite crates.io index lag for ${crate}.${NC}"
      fi
    fi
  else
    printf '%s\n' "$publish_output"
    if grep -Eq 'already exists on crates\.io index|already uploaded' <<<"$publish_output"; then
      SKIPPED=$((SKIPPED + 1))
      echo -e "${YELLOW}Skipping ${crate} ${version}; already published.${NC}"
      if [ "$CREATE_TAGS" = true ]; then
        ensure_crate_tag "$crate" "$version"
      fi
      continue
    fi

    FAILED=$((FAILED + 1))
    echo -e "${RED}Failed to publish ${crate} ${version}${NC}"
    exit 1
  fi
done

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Publishing Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Published: ${PUBLISHED}${NC}"
echo -e "${YELLOW}Skipped: ${SKIPPED}${NC}"
if [ "$FAILED" -gt 0 ]; then
  echo -e "${RED}Failed: ${FAILED}${NC}"
  exit 1
fi

echo -e "${GREEN}All selected crates processed.${NC}"
