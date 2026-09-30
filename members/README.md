# Stack-member manifests

One reviewed TOML file per stack member (aegis-1i5h1j). `build.rs` embeds every
`*.toml` here into the caboodle binary. `src/members.rs` holds the schema, and
its unit tests validate every manifest. Adding a member is a data file and
needs no code change.

## Pins

A member installs only the version pinned here. The install is accepted only
if its asset hashes to the digest recorded here for the host's target. The
release's own sums file is never trusted at install time.

To move a pin to a new release:

    caboodle bump-member members/<name>.toml            # latest stable release
    caboodle bump-member members/<name>.toml --tag <t>  # a specific release

This downloads the release's `sums_asset` and every pinned target's asset. It
refuses unless each asset hashes to its published line, and refuses a
downgrade. It also unpacks the asset for the host it runs on and refuses unless
the programs answer `identity_contains` and report the new version. Run it on a
pinned target.

That proof EXECUTES a release nobody has reviewed yet. To confine it, pass
`--probe-wrapper <exe>`: every execution of the new release then runs as
`<exe> <program> <args...>`. The unattended bump job passes a sandbox with no
network and no home directory (aegis-uy26l7). It then rewrites only `version` and the `[sha256]` values. Review
the diff and open a PR; the new pin reaches users with the next caboodle
release.

## Identity

`identity_contains` must be text that only this member prints, for example a
line of its `--help`. A version line will not do: it has the same shape for a
different program of the same name. The text must ALSO appear in the OLDEST
release a user may still have installed. Install refuses to replace a program
whose identity it cannot confirm, so a string added in a newer release makes
the first upgrade over an older install fail.
