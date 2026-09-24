# The caboodle stack README standard

One README shape and one theme for the six public repos of the stack:
**caboodle, quipu, camayoc, bobbin, yupana, desire-path.**

A stranger who lands on any of them should, within one screen, know what
the tool is and whether it is for them. Within five minutes they should have
run it and watched it succeed. Someone who has used one repo should find the
next one laid out the same way. Depth lives in each repo's mdBook, not in its
README.

Status: **DRAFT v0.1** (aegis-0tpicu), co-evolved with the yupana pilot
(aegis-ewa07y). It goes to review together with that pilot before the other
five repos are brought into line.

The fill-in template is [`README-TEMPLATE.md`](README-TEMPLATE.md).

---

## 1. Section order (fixed)

Every README has these sections, in this order, with these **exact headings**,
so a reader who knows one repo can scan another. A section a tool genuinely
lacks is omitted. It is never renamed and never moved.

| # | heading (exact) | what goes in it | budget |
|---|---|---|---|
| 0 | *(header block, no heading)* | banner/logo, name, one-line tagline, ≤ 3 badges | 1 screen with §1 |
| 1 | *(bold first paragraph, no heading)* | **What it is, who it is for, and the forms it takes** (CLI / MCP / hooks / library). One sentence on the name if the name needs it. | ≤ 6 lines |
| 2 | `## Why you would want it` | 3 bullets, each an **outcome** for the reader, then ONE link to the book page with the full case and the comparison table | ≤ 12 lines |
| 3 | `## Install` | Release first (platforms it ships for, checksum verified), then from source. Ends with `<tool> --version` and its expected output. | ≤ 25 lines |
| 4 | `## First success in three commands` | A self-contained fixture, **≤ 3 commands**, the **exact expected output** in a `text` block, and one sentence on why that output matters | ≤ 30 lines |
| 5 | `## On your own code` *(or `## On your own data`)* | A "question → command" table, then a link to the CLI reference in the book | ≤ 20 lines |
| 6 | `## Wire it into your agent` | The MCP one-liner and the minimal hook/config, then links for everything else | ≤ 25 lines |
| 7 | `## Before you start` | Only what a newcomer needs up front: the platforms/languages table and prerequisites | ≤ 20 lines |
| 8 | `## What's next` | Exactly 3 links into the book: the book home, the docs map, and the part of the book a new user should read next | 3 lines |
| 9 | `## 🧺 The stack` | **The identical shared block** (§3), with this repo's row marked | fixed |
| 10 | `## Contributing` | ≤ 3 `just` commands (build, test, the local CI equivalent), plus a link to CONTRIBUTING if one exists | ≤ 10 lines |
| 11 | `## 📜 License` | One line, then the footer banner | fixed |

**Length budget: the first install command appears within the first 60 lines,
and the README stays under 250 lines.** Anything that does not fit is a book
page with a one-line link left behind. Today the first install appears at
line 49 in caboodle, 96 in bobbin, 108 in desire-path, 188 in yupana, 197 in
quipu, and 245 in camayoc (measured on `main`, 2026-09-24).

Caboodle is the one repo where installing *is* the first success. Its §3 and
§4 may be a single `## Install and prove it` section. That is the only
permitted merge.

### What moves OUT of the README into the book

The sales copy and comparison tables, "how it works" essays, architecture
diagrams, full command references, the stack essay and pairing notes,
roadmaps, design rationale, benchmark numbers, and troubleshooting beyond
three lines. Leave one sentence and a link.

## 2. Theme (the same look across the stack)

**Header block** (centred, in this order):

```html
<p align="center"><img src="assets/header.svg" width="100%" alt="…"/></p>   <!-- optional banner -->
<p align="center"><img src="assets/logo.svg" width="200" alt="…"/></p>
<h1 align="center">toolname</h1>
<p align="center"><em>EMOJI One-line outcome-first tagline</em></p>
<p align="center"> …≤ 3 badges… </p>
```

- **Tagline: outcome-first, plain English, no jargon, ≤ 90 characters.**
  Say what the reader gets, not what category the tool belongs to. Example:
  "Know what a change will break before you make it", not "Live per-tenant
  code structure and a change-time policy engine".
- **One emoji per repo, used in the tagline and nowhere else in the body.** It
  is the repo's mark. Current marks, kept: caboodle 🧰, quipu 🪢, yupana 🧵;
  bobbin, camayoc and desire-path choose theirs in their rollout PRs.
- **Emoji in headings: only the two shared closing sections**, `🧺 The stack`
  and `📜 License`, which are identical in every repo and so read as the
  stack's shared footer. No other heading carries an emoji. That means no
  `🚀 Quick Start`.
- **Badges: at most 3, in this order**: License, then CI status (or the latest
  release), then `stack-quipu`. The stack badge is the shared mark:
  `https://img.shields.io/badge/stack-quipu-8B5E3C.svg`, linking to caboodle.
  No language, toolchain or vanity badges. The platform table carries that.
  quipu may add its DOI as a fourth badge, because citation is a real use.
- **Footer**: `assets/footer.svg` under the license line if the repo has one.
- **Headings are sentence case.**

**mdBook theme**: `default-theme = "coal"`, `preferred-dark-theme = "coal"`
(all six already agree), plus the shared `custom/css/custom.css`. quipu and
bobbin already ship one. It becomes the stack's single copy, vendored into
each book, and diverging from it is a review finding.

## 3. The shared stack block (identical everywhere)

Copy verbatim, then mark your own row with **(you are here)**. It lists the
six stack repos only. Other projects belong on caboodle's book page, not in
every README.

```markdown
## 🧺 The stack

Caboodle installs these together and proves each one works; every tool also stands alone.

| tool | what it gives your agents |
|---|---|
| [caboodle](https://github.com/scbrown/caboodle) | one wizard that installs the stack and proves it works |
| [quipu](https://github.com/scbrown/quipu) | a knowledge graph that refuses facts that break its rules |
| [camayoc](https://github.com/scbrown/camayoc) | the starter vocabulary, and how new knowledge earns its way in |
| [bobbin](https://github.com/scbrown/bobbin) | search and context over your repositories, served over MCP |
| [yupana](https://github.com/scbrown/yupana) | which code calls which: the blast radius before an edit |
| [desire-path](https://github.com/scbrown/desire-path) | the tool calls your agents get wrong, so you can fix them |
```

The row wording is outcome-first, like the taglines. Changing it is a change
to this standard, made here, and then synced to all six repos.

## 4. Rules every README is held to

1. **Every command runs as written.** The code blocks in §3 to §6 are
   extracted and run on a clean checkout under `env -i` with a fresh `HOME`.
   The output of §4 is diffed against the README's own `text` block. Where the
   harness exists (yupana's pilot does this), it runs in CI. Where it does not
   yet, the PR states how it was verified.
2. **Every link resolves.** A link checker (lychee or equivalent) runs in
   pre-commit and in CI. Links into the book point at
   `docs/book/src/<page>.md` on GitHub **until** that repo's Pages site
   actually serves, which is checked, not assumed. (yupana's Pages site
   returned 404 on 2026-09-24.)
3. **No internal names.** No hostnames, private IPs, host names or home paths.
   These are public repos. The repos' existing scrub guards apply; the README
   is not exempt.
4. **Claims carry their evidence or their link.** A number in a README
   (speed, accuracy, conformance) links to the book page that shows how it was
   measured and when. A number with no link is removed.
5. **Stranger-first.** Write for someone who has never heard of the stack.
   Define a term the first time it appears, or link it. Internal work-item
   IDs, agent names and history ("renamed from …") go in the book, never in
   the README.

## 5. The mdBook (every repo has one)

Each repo has `docs/book/` with at least these pages. Structure beyond them
is the owner's call.

```
SUMMARY.md
  Introduction                  (the README's §1 and §2, expanded)
  Getting started               (mirrors README §3 to §4, plus troubleshooting)
  Using it with agents          (the full version of README §6)
  Reference                     (every command and config key)
  How it works                  (architecture, design rationale)
  The stack                     (how this tool fits the other five)
  Docs map                      (routes EVERY loose file under docs/)
```

**Docs map**: every file under `docs/` outside the book is either folded into
a book page or listed on the docs map with one line on what it is. A
historical document is marked **Historical** in that line (e.g. yupana's
`rename-from-hank.md`). No loose doc is left unreachable from the book.

All six repos already have a book (`docs/book/book.toml` or `book.toml`) except
**camayoc**, which needs a new one.

## 6. Review checklist (what wu checks on every rollout PR)

- [ ] Section order and exact headings per §1; omissions only, no renames
- [ ] First install command within 60 lines; README under 250 lines
- [ ] Header block per §2: outcome-first tagline, ≤ 3 badges in order, one emoji mark
- [ ] No emoji in headings except `🧺 The stack` and `📜 License`
- [ ] The stack block matches §3 byte for byte, with this repo's row marked
- [ ] §4 has an exact expected output, and the PR shows it was produced by a clean run
- [ ] Link check green, with book links resolving
- [ ] No internal names (scrub guard green)
- [ ] Every README number links to its measurement page
- [ ] The book has the §5 pages and a docs map that routes every loose doc
- [ ] The rendered README link is on the bead for the owner's review before merge
