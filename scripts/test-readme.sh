#!/bin/bash
# arming: ci Documentation workflow; also runnable locally
# Execute the README's release install and first-success blocks with no inherited
# tool installations or configuration. Diff stdout against the README itself.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
cleanup() {
  if [ -f "$work/credential/server.pid" ]; then
    kill "$(cat "$work/credential/server.pid")" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir "$work/home" "$work/run"
# Use the checkout's checksummed release-shaped fixture so new documented
# behavior is tested before release, not against an older published binary.
cargo build --release --locked --manifest-path "$repo/Cargo.toml"
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repo/Cargo.toml" | head -n 1)
target_dir=$(cargo metadata --no-deps --format-version 1 --manifest-path "$repo/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
archive="caboodle-v$version-x86_64-unknown-linux-gnu.tar.gz"
mkdir "$work/release"
tar -czf "$work/release/$archive" -C "$target_dir/release" caboodle
(cd "$work/release" && sha256sum "$archive" > "$archive.sha256")
bind=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print("127.0.0.1:"+str(s.getsockname()[1])); s.close()')
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
# Absolute: the block changes into its own work directory first (aegis-2gtpsr).
sed "/^curl .*examples\/caboodle-intent.toml/i exec > $work/run/actual.txt" "$work/commands.sh" > "$work/run.sh"
# The documented live-server/issued-token prerequisite is built by the test.
# apply installs the real Quipu server; the actual README install still runs its
# own apply and full verification. No fixture bypasses the credential gate.
sed -i '/^caboodle install$/i\
caboodle apply >/dev/null\
python3 "$CABOODLE_TEST_REPO/scripts/start-credential-fixture.py" --home "$HOME" --root "$CABOODLE_TEST_ROOT/credential" --bind "$CABOODLE_TEST_BIND" >/dev/null\
export QUIPU_SERVER="http://$CABOODLE_TEST_BIND"\
caboodle provision-quipu-token --from "$CABOODLE_TEST_ROOT/credential/issued-token" --server "$QUIPU_SERVER" >/dev/null' "$work/run.sh"
(cd "$work/run" && env -i HOME="$work/home" PATH=/usr/bin:/bin \
  CABOODLE_VERSION="v$version" CABOODLE_RELEASE_BASE_URL="file://$work/release" \
  CABOODLE_TEST_REPO="$repo" CABOODLE_TEST_ROOT="$work" CABOODLE_TEST_BIND="$bind" \
  /bin/bash --noprofile --norc -e "$work/run.sh")
# The README shows paths under the reader's home as ~ (aegis-y4dd06); this run's
# HOME is a fresh temp dir, so write it the same way before comparing.
sed "s|$work/home|~|g" "$work/run/actual.txt" > "$work/actual.txt"
diff -u "$work/expected.txt" "$work/actual.txt"
printf '%s\n' 'README first success: exact stdout matched in a fresh HOME'
