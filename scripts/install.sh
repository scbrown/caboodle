#!/bin/sh
set -eu

repo=${CABOODLE_REPOSITORY:-scbrown/caboodle}
version=${CABOODLE_VERSION:-}
install_dir=${CABOODLE_INSTALL_DIR:-${CARGO_HOME:-$HOME/.cargo}/bin}

if [ -z "$version" ]; then
    version=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" |
        sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
fi
case "$version" in
    v[0-9]*) ;;
    *) printf '%s\n' "invalid or unavailable CABOODLE release version: $version" >&2; exit 1 ;;
esac

if [ -n "${CABOODLE_TARGET:-}" ]; then
    target=$CABOODLE_TARGET
else
    machine=$(uname -m)
    system=$(uname -s)
    case "$system:$machine" in
        Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
        Darwin:x86_64) target=x86_64-apple-darwin ;;
        Darwin:arm64) target=aarch64-apple-darwin ;;
        *) printf '%s\n' "unsupported CABOODLE release target: $system $machine" >&2; exit 1 ;;
    esac
fi

archive="caboodle-$version-$target.tar.gz"
base=${CABOODLE_RELEASE_BASE_URL:-https://github.com/$repo/releases/download/$version}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

curl -fsSL "$base/$archive" -o "$work/$archive"
curl -fsSL "$base/$archive.sha256" -o "$work/$archive.sha256"
if command -v sha256sum >/dev/null; then
    (cd "$work" && sha256sum -c "$archive.sha256")
else
    expected=$(awk '{print $1}' "$work/$archive.sha256")
    actual=$(shasum -a 256 "$work/$archive" | awk '{print $1}')
    test "$actual" = "$expected" || {
        printf '%s\n' "$archive: checksum mismatch" >&2
        exit 1
    }
fi
tar -xzf "$work/$archive" -C "$work"
test -x "$work/caboodle"
mkdir -p "$install_dir"
staged="$install_dir/.caboodle.$$.tmp"
cp "$work/caboodle" "$staged"
chmod 0755 "$staged"
mv "$staged" "$install_dir/caboodle"
"$install_dir/caboodle" --version
printf '%s\n' "installed $install_dir/caboodle"
