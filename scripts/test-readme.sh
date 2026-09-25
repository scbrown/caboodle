#!/bin/bash
# arming: ci Documentation workflow; also runnable locally
# Execute the README's release install and first-success blocks with no inherited
# tool installations or configuration. Diff stdout against the README itself.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/home" "$work/run"
awk '
  /^## Install$/ { section = "install"; next }
  /^## First success in three commands$/ { section = "success"; next }
  /^## / { section = "" }
  /^```bash$/ && section != "" { block++; capture = (section == "success" || block == 1); next }
  /^```$/ { capture = 0; next }
  capture { print }
' "$repo/README.md" > "$work/commands.sh"
awk '
  /^## First success in three commands$/ { section = 1; next }
  /^## / { section = 0 }
  /^```text$/ && section { capture = 1; next }
  /^```$/ { capture = 0; next }
  capture { print }
' "$repo/README.md" > "$work/expected.txt"
test -s "$work/expected.txt"
# Separate the release installer (whose output includes HOME) from the three
# first-success commands. Keep the same fresh HOME and shell for both.
sed '/^curl .*examples\/caboodle-intent.toml/i exec > actual.txt' "$work/commands.sh" > "$work/run.sh"
(cd "$work/run" && env -i HOME="$work/home" PATH=/usr/bin:/bin \
  /bin/bash --noprofile --norc -e "$work/run.sh")
diff -u "$work/expected.txt" "$work/run/actual.txt"
printf '%s\n' 'README first success: exact stdout matched in a fresh HOME'
