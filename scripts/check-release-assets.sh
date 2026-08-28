#!/bin/sh
set -eu

tag=${1:?usage: check-release-assets.sh <vVERSION> <directory>}
directory=${2:?usage: check-release-assets.sh <vVERSION> <directory>}
package_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)
test "$tag" = "v$package_version" || {
    printf '%s\n' "tag $tag does not match Cargo.toml version v$package_version" >&2
    exit 1
}

found=0
for archive in "$directory"/caboodle-"$tag"-*.tar.gz; do
    test -f "$archive" || continue
    checksum="$archive.sha256"
    test -f "$checksum"
    tar -tzf "$archive" | grep -qx 'caboodle'
    expected_name=$(basename "$archive")
    recorded_name=$(awk '{print $2}' "$checksum")
    test "$recorded_name" = "$expected_name" || {
        printf '%s\n' "checksum names $recorded_name, expected $expected_name" >&2
        exit 1
    }
    (cd "$directory" && sha256sum -c "$(basename "$checksum")")
    found=$((found + 1))
done
test "$found" -gt 0 || {
    printf '%s\n' "no release archives found for $tag" >&2
    exit 1
}
for target in ${CABOODLE_EXPECT_TARGETS:-}; do
    test -f "$directory/caboodle-$tag-$target.tar.gz" || {
        printf '%s\n' "missing release archive for supported target $target" >&2
        exit 1
    }
    test -f "$directory/caboodle-$tag-$target.tar.gz.sha256" || {
        printf '%s\n' "missing checksum for supported target $target" >&2
        exit 1
    }
done
printf '%s\n' "release metadata: $found archive(s) verified for $tag"
