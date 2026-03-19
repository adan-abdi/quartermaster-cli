#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cli_dir="$(cd "${script_dir}/.." && pwd)"
repo_root="$(cd "${cli_dir}/.." && pwd)"

mkdir -p "${cli_dir}/static/dashboard" "${cli_dir}/static/assets"
rm -f "${cli_dir}/static/assets/"*

cp "${repo_root}/dist/dashboard/index.html" "${cli_dir}/static/dashboard/index.html"
cp "${repo_root}/dist/glyph_logo.svg" "${cli_dir}/static/glyph_logo.svg"

shopt -s nullglob
for asset in \
  "${repo_root}"/dist/assets/dashboard-*.css \
  "${repo_root}"/dist/assets/dashboard-*.js \
  "${repo_root}"/dist/assets/index-*.css \
  "${repo_root}"/dist/assets/index-*.js
do
  cp "${asset}" "${cli_dir}/static/assets/"
done
shopt -u nullglob

if ! compgen -G "${cli_dir}/static/assets/dashboard-*.js" > /dev/null; then
  echo "Missing dashboard JS artifact after sync" >&2
  exit 1
fi

if ! compgen -G "${cli_dir}/static/assets/index-*.js" > /dev/null; then
  echo "Missing shared index JS artifact after sync" >&2
  exit 1
fi

echo "Synced dashboard runtime artifacts into ${cli_dir}/static"
