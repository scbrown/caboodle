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

# A stale copy earlier on PATH must be named, and the installed one must not.
mkdir -p "$fixture/stale"
printf '#!/bin/sh\necho "caboodle 0.0.1 (stale)"\n' > "$fixture/stale/caboodle"
chmod 0755 "$fixture/stale/caboodle"
shadow_err=$(PATH="$fixture/stale:$fixture/bin:$PATH" \
    CABOODLE_VERSION="v$version" CABOODLE_TARGET="$target" \
    CABOODLE_RELEASE_BASE_URL="file://$fixture" CABOODLE_INSTALL_DIR="$fixture/bin" \
    "$repo_root/scripts/install.sh" 2>&1 >/dev/null)
printf '%s\n' "$shadow_err" | grep -F "WARNING: \`caboodle\` on PATH is $fixture/stale/caboodle (caboodle 0.0.1 (stale))"
clean_err=$(PATH="$fixture/bin:$fixture/stale:$PATH" \
    CABOODLE_VERSION="v$version" CABOODLE_TARGET="$target" \
    CABOODLE_RELEASE_BASE_URL="file://$fixture" CABOODLE_INSTALL_DIR="$fixture/bin" \
    "$repo_root/scripts/install.sh" 2>&1 >/dev/null)
if printf '%s\n' "$clean_err" | grep -q WARNING; then
    printf '%s\n' 'installer warned although PATH runs the installed copy' >&2
    exit 1
fi

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
