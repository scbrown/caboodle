<p align="center">
  <img src="assets/header.svg" width="100%" alt="Animated banner — a teal kit box with a tipping lid and three flowing, knotted cords"/>
</p>

<p align="center">
  <img src="assets/logo.svg" width="200" alt="Caboodle logo — a light-teal tackle box open with pink fold-out trays, each compartment holding a tool of the stack"/>
</p>

<h1 align="center">caboodle</h1>

<p align="center">
  <em>🧰 The whole kit — one wizard that installs the stack, proves it works, and watches it run</em>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"/></a>
  <a href="https://github.com/scbrown/caboodle/actions/workflows/ci.yml"><img src="https://github.com/scbrown/caboodle/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <a href="https://github.com/scbrown/caboodle"><img src="https://img.shields.io/badge/stack-quipu-8B5E3C.svg" alt="Part of the quipu stack"/></a>
</p>

**Caboodle is an install wizard for people and AI coding agents who want
persistent memory and searchable code context. Its CLI installs the Quipu
stack from a reviewable plan and proves each tool works. An agent skill
walks through the same workflow.**

## Why you would want it

- Give your agent memory and code context that survive a session.
- Review one plan, then resume installation if it is interrupted.
- Know each tool passed a functional check before you rely on it.

[Why Caboodle, and how it compares to installing by hand](https://scbrown.github.io/caboodle/introduction.html#why-use-it).

## Install

Checksummed releases: Linux x86_64, macOS arm64 and macOS x86_64.
The installer verifies SHA256 before placing the binary in `~/.cargo/bin`.

```bash
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh
sh install.sh
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
```

`install.sh` fetches the latest release; set `CABOODLE_VERSION=vX.Y.Z` to pin one.
The `export` lasts for this shell only: add it to `~/.bashrc` (or `~/.zshrc`).
Or build the latest release from source with Rust (`cargo install --git
https://github.com/scbrown/caboodle --tag vX.Y.Z --locked`). Either way,
`caboodle --version` prints `caboodle X.Y.Z` for the release you installed.

## First success in three commands

Run these in a directory that will stay put, such as `~/caboodle`; caboodle keeps
its plan and state there. `retrieval` installs Quipu, Camayoc and Bobbin, which
the agent step needs. To check prerequisites, run `caboodle doctor` after `plan`
in the same directory (with no plan it checks `everything`, which needs Go).

```bash
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/examples/caboodle-intent.toml
caboodle plan --profile retrieval --intent caboodle-intent.toml
caboodle install
```

Expected stdout on a fresh machine (downloads and progress go to stderr):

```text
plan: caboodle-plan.toml
quipu: applied
camayoc: applied
bobbin: applied
stack configuration: applied
hook bundle NOT registered: `st` (shantytown) is not installed on this host. Install it and rerun `caboodle apply`; verify reports the bundles until then
vocabulary: quechua v0.1.0 cached at ~/.local/share/caboodle/vocabulary/quechua-ns-v0.1.0.ttl (no plan quipu_db to load into)
hook bundle NOT verified: `st` (shantytown) is not installed on this host, so no hook bundle is registered here
quipu: verified
camayoc: verified
bobbin: verified
```

`hook bundle` lines are expected without Shantytown. Each `verified` is a passed
functional round trip; [the walkthrough](https://scbrown.github.io/caboodle/getting-started.html) covers recovery.

## On your own code

| I want to… | Command |
|---|---|
| Choose tools and describe my own use | `caboodle init --guided` |
| Find install blockers before changing anything | `caboodle doctor` |
| Install and prove the reviewed selection | `caboodle install` |
| Recheck the selected tools | `caboodle verify` |
| Compare against this build's reviewed versions | `caboodle check-updates` |

[Full command and configuration reference](https://scbrown.github.io/caboodle/reference.html).

## Wire it into your agent

Caboodle is a CLI, with an [agent skill](skills/caboodle/SKILL.md), not an MCP
server. Bobbin, which the `retrieval` profile installed above, is one. It
searches the repository it was started in, so index that repository first, then
register it from the same directory:

```bash
cd ~/path/to/your/repo
bobbin init
bobbin index
claude mcp add bobbin -- bobbin serve
```

`claude mcp add` uses the default `local` scope: the server is registered for
this project only, which matches an index of this repository. Repeat the four
lines in each repository you want searchable. Without `bobbin init`, `bobbin
serve` exits with `Bobbin not initialized` and Claude Code reports the server as
failed to connect.

MCP (Model Context Protocol) lets your agent call the installed server.
[Agent setup](https://scbrown.github.io/caboodle/agents.html) covers indexing, Yupana, other clients
and managed crew registration through `caboodle project-settings`.

## Before you start

| Platform | Caboodle | Quipu | Yupana |
|---|---|---|---|
| Linux x86_64 | Release | Release | Release |
| macOS arm64 / x86_64 | Release | Release | Release |
| Linux arm64 | Build with Rust | Release | Build with Rust |

Have `curl`, `tar`, `git`, `bash`, `python3` and `sha256sum` on PATH
(`brew install coreutils` supplies `sha256sum` on macOS).
The `everything` profile also needs Go. No account or remote server is needed.
[Installation details](https://scbrown.github.io/caboodle/installing.html) cover source builds,
installed files and removal; `caboodle doctor` names missing prerequisites.

## What's next

- [Read the book](https://scbrown.github.io/caboodle/introduction.html)
- [Find a topic in the docs map](https://scbrown.github.io/caboodle/docs-map.html)
- [Choose your profile](https://scbrown.github.io/caboodle/profiles.html)

## 🧺 The stack

Caboodle installs these together and proves each one works; every tool also stands alone.

| tool | what it gives your agents |
|---|---|
| [caboodle](https://github.com/scbrown/caboodle) **(you are here)** | one wizard that installs the stack and proves it works |
| [quipu](https://github.com/scbrown/quipu) | a knowledge graph that refuses facts that break its rules |
| [camayoc](https://github.com/scbrown/camayoc) | the starter vocabulary, and how new knowledge earns its way in |
| [bobbin](https://github.com/scbrown/bobbin) | search and context over your repositories, served over MCP |
| [yupana](https://github.com/scbrown/yupana) | which code calls which: the blast radius before an edit |
| [desire-path](https://github.com/scbrown/desire-path) | the tool calls your agents get wrong, so you can fix them |
| [seeds](https://github.com/scbrown/seeds) | the work your agents track, as facts in the graph with full history |
| [shuttle](https://github.com/scbrown/shuttle) | the workflows your agents run, every step signed and kept in the graph |

## Contributing

```bash
just book
just check
just docs-check
```

## 📜 License

[MIT](LICENSE)

<p align="center">
  <img src="assets/footer.svg" width="100%" alt="Animated footer — a woven band of teal, pink, and ochre cords with sliding beads"/>
</p>
