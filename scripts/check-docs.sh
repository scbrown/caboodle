#!/bin/sh
# arming: ci Documentation workflow and pre-commit docs-links hook
set -eu
cd "$(dirname "$0")/.."
site=$(mktemp -d)
trap 'rm -rf "$site"' EXIT HUP INT TERM
# Match Pages' /caboodle/ base, including the generated 404 page's root link.
mdbook build docs/book --dest-dir "$site/caboodle"
lychee --config .lychee.toml --root-dir "$site" README.md "$site/caboodle/**/*.html"
