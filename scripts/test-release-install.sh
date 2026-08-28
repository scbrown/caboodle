#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$repo_root/Cargo.toml" | head -n 1)
target=x86_64-unknown-linux-gnu
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT HUP INT TERM
archive="caboodle-v$version-$target.tar.gz"

cargo build --release --locked --manifest-path "$repo_root/Cargo.toml"
tar -czf "$fixture/$archive" -C "$repo_root/target/release" caboodle
(cd "$fixture" && sha256sum "$archive" > "$archive.sha256")
(cd "$repo_root" && scripts/check-release-assets.sh "v$version" "$fixture")

CABOODLE_VERSION="v$version" \
CABOODLE_TARGET="$target" \
CABOODLE_RELEASE_BASE_URL="file://$fixture" \
CABOODLE_INSTALL_DIR="$fixture/bin" \
    "$repo_root/scripts/install.sh"
"$fixture/bin/caboodle" --version | grep -F "caboodle $version"
test -x "$fixture/bin/caboodle"

printf 'corrupt' >> "$fixture/$archive"
if CABOODLE_VERSION="v$version" \
   CABOODLE_TARGET="$target" \
   CABOODLE_RELEASE_BASE_URL="file://$fixture" \
   CABOODLE_INSTALL_DIR="$fixture/bin" \
       "$repo_root/scripts/install.sh" >/dev/null 2>&1; then
    printf '%s\n' 'corrupt release archive unexpectedly installed' >&2
    exit 1
fi
test -x "$fixture/bin/caboodle"
rm "$fixture/bin/caboodle"
test ! -e "$fixture/bin/caboodle"
printf '%s\n' 'release installer fixture: verified'
