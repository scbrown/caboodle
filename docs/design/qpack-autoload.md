# Design: qpack autoload, monorepo shares and graph pointers

**Status:** proposal, not built. Every behaviour below is future work unless it
says "exists today".

Once repositories routinely ship a quipu pack (`.qpack.tar.gz`), a fresh clone
should have that knowledge in the local graph without anyone running a command.
This document covers three questions: how a clone discovers and loads its
packs, how a monorepo ships one pack per project, and how a pack can point at
graphs that are not loaded yet.

## What exists today

- **Packs.** A `.qpack.tar.gz` is a deterministic text bundle, not a database
  file. Its manifest carries `parent_share`, `tx_anchor`, `graph_hash`,
  `shapes_hash` and a stored CONSTRUCT scope.
- **Composition.** `quipu compose` loads verified packs into one store. Each
  pack keeps its own named graph, and their union is named as a dataset.
  Exact IRIs join across packs; blank nodes stay scoped to their pack.
  Different shape bundles need an explicit `--shapes-from`. A nonconforming
  union stays in quarantine, and nothing is promoted to ROOT. A local
  retraction survives a reload of the same snapshot. See quipu's
  [pack composition](https://scbrown.github.io/quipu/concepts/pack-composition.html).
- **Manual loading.** `install-stack.sh --qpack PATH` loads a pack by hand, and
  `scripts/setup-environment.sh` stages and verifies packs in cloud
  environments.
- **Federation.** quipu reaches remote graphs through SPARQL `SERVICE` and
  federated query fan-out, but only endpoints declared in the federation
  configuration.

Nothing yet notices that a cloned repository ships a pack.

## 1. Discovery and autoload

**The repository declares its packs** in a manifest at a fixed path,
`.quipu/packs.toml`:

```toml
[[pack]]
path = "knowledge/repo.qpack.tar.gz"   # in-repo path
graph = "https://example.org/repo/scbrown/quipu"
sha256 = "…"                           # of the archive
quipu = ">=0.9"                        # the loader version it needs
```

A file at a fixed path is what makes discovery cheap and reliable. Neither
caboodle nor quipu guesses from file extensions.

**Triggers.** A `post-checkout` and `post-merge` hook, installed by
`caboodle init` in repositories the user opts in, runs one command:

```bash
caboodle packs sync --repo .
```

The command is idempotent. It reads the manifest and compares each pack's
`sha256` with the one it last loaded for that repository. It does nothing when
they match. For environments without hooks (cloud sessions, CI), the same
command runs from `setup-environment.sh`.

**Loading goes through composition, never around it.** caboodle verifies the
archive hash, then hands the pack to `quipu compose`. The pack lands in its own
named graph, keyed by repository, pack path and commit.

**A stable name to query.** Composition deliberately never replaces an earlier
snapshot: a new commit's pack is a new composition. So autoload needs two things
quipu does not have yet:

1. A per-repository **current pointer**, for example
   `urn:quipu:repo:<repo>:current`, that names the latest successful
   composition. Agents query that name, not a hash that changes on every
   commit. Moving the pointer is one atomic write.
2. **Retention.** Keep the current snapshot and the previous one, so a bad
   pack can be rolled back by moving the pointer. Drop older snapshot graphs.
   Local retractions live in the source graph and must be carried forward when
   a new snapshot of the same pack is loaded; quipu's composition already
   preserves them within a snapshot.

**Refusals are loud.** A hash mismatch, a manifest naming a missing file, or a
loader older than `quipu =` all refuse, with a message naming the pack. They
never load partially.

## 2. Trust

A pack is data landing in the graph that agents query first, so loading is a
trust decision.

- **Your own repositories load automatically**, meaning repositories whose
  remote owner matches an allow-list in caboodle's configuration.
- **A foreign pack needs an explicit allow**, recorded per repository and pack
  hash. A repository name appearing in a configuration file is not consent.
- **Loading is not promotion.** An autoloaded pack stays in its named dataset,
  queryable through the current pointer. It never enters ROOT. Promotion
  remains a separate, reviewed step, as composition already enforces.
- **Shape authority stays with the store.** Autoload always composes with the
  store's own shapes as the authority (`--shapes-from` the local bundle). A pack
  cannot install constraints.

## 3. Monorepos

A monorepo ships **one parent share and one derived share per project**.

- The parent share covers the whole repository. Each project is a derived share
  (`parent_share` set) whose stored CONSTRUCT scope filters by path prefix,
  such as `packages/api/`.
- `.quipu/packs.toml` lists the derived shares with their `path_prefix`.
  `caboodle packs sync` reloads only the projects whose paths changed in the
  checkout, the same idea as yupana's per-file keys, grouped by project.
- **Cross-project edges survive a reload.** Entity IRIs do not depend on the
  commit, so an edge from one project to another still resolves after either
  side reloads.
- **Sparse checkouts** load only the projects present. The projects that are
  absent can be reached through a pointer (section 4) instead.
- **Ownership:** a CODEOWNERS entry becomes `owned_by` on the project's
  entities.
- **Recommendation:** no combined all-projects pack at first. Composition of
  the derived shares already gives the union. Add a combined pack only if cold
  start is measured to be too slow.

Test case: `scbrown/reckoning`, a real pnpm monorepo.

## 4. Pointers to graphs that are not loaded

A manifest entry can name a graph without shipping it:

```toml
[[pointer]]
graph = "https://example.org/homelab"
kind = "endpoint"                     # or "pack"
endpoint = "https://quipu.example.org/sparql"
```

- **`kind = "endpoint"`**: a live, large or fast-moving graph, such as a
  homelab operations graph. `caboodle packs sync` adds it to quipu's declared
  federation list, so `SERVICE` and federated fan-out reach it with no local
  copy. The trust rule of section 2 applies to the endpoint: declaring it in a
  foreign repository's manifest does not authorise it.
- **`kind = "pack"`** (with `url` and `sha256`): a pinned, offline-capable
  pack. It is fetched, verified and composed on first use rather than at
  checkout. This also works in the browser build.

A sparse monorepo checkout can federate to the projects it did not check out
this way.

## Test cases

| Case | Covers |
| --- | --- |
| quipu's own repository plus a homelab operations pack | sections 1, 2 and 4 |
| `scbrown/reckoning` (pnpm monorepo) | section 3 |
| A pack with a tampered archive | the hash refusal |
| A foreign repository with no allow entry | the trust refusal |
| Two commits of one pack | pointer moves; previous kept; retraction carried |

## Proposed children

1. **quipu:** the per-repository current pointer and snapshot retention, with
   retraction carry-forward across snapshots of one pack.
2. **caboodle:** `.quipu/packs.toml`, `caboodle packs sync`, and the opt-in git
   hooks.
3. **caboodle:** the trust allow-list for foreign packs and pointer endpoints.
4. **quipu and caboodle:** monorepo derived shares by path prefix, proven on
   `scbrown/reckoning`.
5. **quipu:** lazy `kind = "pack"` pointers.
