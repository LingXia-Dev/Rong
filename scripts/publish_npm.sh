#!/usr/bin/env bash
set -euo pipefail

PACKAGE_DIRS=("packages/rong_types" "packages/skill")
CREATE_PACKAGE_TAGS=false

usage() {
  local exit_code=${1:-0}
  cat << EOF
Usage: $0 [OPTIONS]

Publish repo-maintained npm packages using npm trusted publishing.

OPTIONS:
  --package-tag  Create and push npm-<package>-v<version> git tags for
                 published or already-published packages
  -h, --help     Show this help message
EOF
  exit "$exit_code"
}

while [[ $# -gt 0 ]]; do
  case $1 in
    --package-tag)
      CREATE_PACKAGE_TAGS=true
      shift
      ;;
    -h|--help)
      usage
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage 1
      ;;
  esac
done

if [ "${GITHUB_ACTIONS:-}" != "true" ] \
  || [ -z "${ACTIONS_ID_TOKEN_REQUEST_TOKEN:-}" ] \
  || [ -z "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ]; then
  echo "npm publish is restricted to GitHub Actions trusted publishing." >&2
  echo "Run the Publish Packages workflow with id-token: write." >&2
  exit 1
fi

if [ -n "${NODE_AUTH_TOKEN:-}" ] || [ -n "${NPM_TOKEN:-}" ]; then
  echo "NPM_TOKEN and NODE_AUTH_TOKEN are not used for npm publishing." >&2
  echo "Remove token-based npm credentials and use trusted publishing." >&2
  exit 1
fi

echo "npm publish auth mode: trusted-publishing"

ensure_npm_tag() {
  local package_name=$1
  local package_version=$2
  local slug
  local tag

  slug="$(printf '%s' "$package_name" | sed 's/^@//; s#[/@]#-#g')"
  tag="npm-${slug}-v${package_version}"

  if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    echo "Tag ${tag} already exists locally."
    return 0
  fi

  if git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1; then
    echo "Tag ${tag} already exists on origin."
    return 0
  fi

  git tag -a "$tag" -m "${package_name} v${package_version}"
  git push origin "$tag"
  echo "Created tag ${tag}"
}

for package_dir in "${PACKAGE_DIRS[@]}"; do
  package_json="${package_dir}/package.json"

  package_name="$(node -p "require('./${package_json}').name")"
  package_version="$(node -p "require('./${package_json}').version")"

  if [ -z "$package_name" ] || [ -z "$package_version" ]; then
    echo "Unable to resolve npm package name/version from ${package_json}" >&2
    exit 1
  fi

  echo "Preparing npm publish for ${package_name}@${package_version}"

  package_exists=false
  if npm view "${package_name}" name >/dev/null 2>&1; then
    package_exists=true
  fi

  if [ "$package_exists" = false ]; then
    cat >&2 << EOF
${package_name} does not exist on npm yet.

npm trusted publishing is configured on an existing package. Create
${package_name} outside this repository automation, configure its trusted
publisher for GitHub Actions in npm package settings, then rerun this workflow.
EOF
    exit 1
  fi

  existing_version="$(npm view "${package_name}@${package_version}" version 2>/dev/null || true)"
  if [ "$existing_version" = "$package_version" ]; then
    echo "Skipping ${package_name}@${package_version}; already published."
    if [ "$CREATE_PACKAGE_TAGS" = true ]; then
      ensure_npm_tag "$package_name" "$package_version"
    fi
    continue
  fi

  (
    cd "$package_dir"
    npm install --no-package-lock
    npm publish --access public
  )

  if [ "$CREATE_PACKAGE_TAGS" = true ]; then
    ensure_npm_tag "$package_name" "$package_version"
  fi
done
