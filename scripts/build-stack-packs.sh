#!/bin/sh
#
# Rebuild the stack knowledge packs in packs/ from their Turtle sources in
# packs/src/.
#
# A .qpack.db is an ordinary quipu SQLite store with a one-row pack_manifest
# table — the checked-in packs are the built artifact, this script is what
# makes them regenerable rather than hand-blessed. The Turtle sources are the
# review surface; the packs are what `quipu unpack` consumes.
#
# Not byte-reproducible across runs: quipu stamps the pack with its creation
# time, so a rebuild with identical sources produces a new content_hash. That
# is why this script is run by hand when packs/src/ changes, not by CI on
# every push — a hash that churns without a source change is noise dressed as
# a diff.
#
# Requires a quipu binary (QUIPU=... overrides discovery).
set -eu

quipu=${QUIPU:-quipu}
command -v "$quipu" >/dev/null 2>&1 || {
    printf '%s\n' "quipu binary not found (set QUIPU=/path/to/quipu)" >&2
    exit 1
}

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out_dir="$root/packs"
src_dir="$root/packs/src"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

version=${PACK_VERSION:-0.1.0}

build_pack() {
    name=$1
    graph="https://caboodle.dev/graph/$name"
    src="$src_dir/$name.ttl"
    out="$out_dir/$name.qpack.db"
    test -f "$src" || { printf '%s\n' "missing source: $src" >&2; exit 1; }

    # knot writes to the scratch store's ROOT; graph import lifts that ROOT
    # into a named graph in the assembly store, which is what pack exports.
    "$quipu" knot "$src" --db "$work/$name-scratch.db"
    "$quipu" graph import "$work/$name-scratch.db" --as "$graph" --db "$work/asm.db"
    rm -f "$out"
    "$quipu" pack "$graph" --out "$out" \
        --name "caboodle-$name" --version "$version" --db "$work/asm.db"
    # A pack that cannot verify must never land in the repo.
    "$quipu" pack --verify "$out"
}

build_pack stack-map
build_pack stack-operations

printf '%s\n' "packs rebuilt in $out_dir (version $version)"
