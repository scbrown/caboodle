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

[Why Caboodle, and how it compares to installing by hand](docs/book/src/introduction.md#why-use-it).

## Install

Checksummed releases: Linux x86_64, macOS arm64 and macOS x86_64.
The installer verifies SHA256 before placing the binary in `~/.cargo/bin`.

```bash
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh
CABOODLE_VERSION=v0.2.2 sh install.sh
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
```

Or build from source with Rust:

```bash
cargo install --git https://github.com/scbrown/caboodle --tag v0.2.2 --locked
caboodle --version
```

Expected version for either route: `caboodle 0.2.2`.

## First success in three commands

In an empty directory, download the example intent and plan the smallest
profile (Quipu + Camayoc). Read `caboodle-plan.toml` before the third command.
Linux x86_64 can run this directly; see platform prerequisites below for macOS.

```bash
curl -fsSLO https://raw.githubusercontent.com/scbrown/caboodle/main/examples/caboodle-intent.toml
caboodle plan --profile kg --intent caboodle-intent.toml
caboodle install
```

Expected stdout (download and installation diagnostics go to stderr):

```text
plan: caboodle-plan.toml
quipu: applied
camayoc: applied
stack configuration: applied
quipu: verified
camayoc: verified
```

Each `verified` means a functional round trip passed after proving its marker
was absent; [the walkthrough](docs/book/src/getting-started.md) covers recovery.

## On your own code

| I want to… | Command |
|---|---|
| Choose tools and describe my own use | `caboodle init --guided` |
| Find install blockers before changing anything | `caboodle doctor` |
| Install and prove the reviewed selection | `caboodle install` |
| Recheck the selected tools | `caboodle verify` |
| Compare against this build's reviewed versions | `caboodle check-updates` |

[Full command and configuration reference](docs/book/src/reference.md).

## Wire it into your agent

Caboodle is a CLI, with an [agent skill](skills/caboodle/SKILL.md), not an MCP
server. After installing the `retrieval` profile, connect Bobbin to Claude Code:

```bash
claude mcp add bobbin -- bobbin serve
```

MCP (Model Context Protocol) lets your agent call the installed server.
[Agent setup](docs/book/src/agents.md) covers indexing, Yupana, other clients
and managed crew registration through `caboodle project-settings`.

## Before you start

| Platform | Caboodle | Quipu and Yupana |
|---|---|---|
| Linux x86_64 | Release | Release |
| macOS arm64 / x86_64 | Release | Build once with Rust |
| Linux arm64 | Build with Rust | Build with Rust |

Have `curl`, `tar`, `git`, `bash`, `python3` and `sha256sum` on PATH
(`brew install coreutils` supplies `sha256sum` on macOS).
The `everything` profile also needs Go. No account or remote server is needed.
[Installation details](docs/book/src/installing.md) cover source builds,
installed files and removal; `caboodle doctor` names missing prerequisites.

## What's next

- [Read the book](docs/book/src/introduction.md)
- [Find a topic in the docs map](docs/book/src/docs-map.md)
- [Choose your profile](docs/book/src/profiles.md)

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
