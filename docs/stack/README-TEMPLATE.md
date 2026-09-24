<!--
  caboodle stack README template: see README-STANDARD.md for the rules.
  Replace every ALL-CAPS placeholder. Delete a section only if the tool truly
  lacks it; never rename or reorder a heading.
  Budget: first install command within 60 lines, whole file under 250.
-->

<p align="center">
  <img src="assets/header.svg" width="100%" alt="ALT TEXT FOR THE BANNER"/>
</p>

<p align="center">
  <img src="assets/logo.svg" width="200" alt="ALT TEXT FOR THE LOGO"/>
</p>

<h1 align="center">TOOLNAME</h1>

<p align="center">
  <em>EMOJI OUTCOME-FIRST TAGLINE, PLAIN ENGLISH, 90 CHARACTERS OR FEWER</em>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"/></a>
  <a href="https://github.com/scbrown/TOOLNAME/actions/workflows/ci.yml"><img src="https://github.com/scbrown/TOOLNAME/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <a href="https://github.com/scbrown/caboodle"><img src="https://img.shields.io/badge/stack-quipu-8B5E3C.svg" alt="Part of the caboodle stack"/></a>
</p>

**TOOLNAME IS A WHAT-IT-IS FOR WHO-IT-IS-FOR. It comes as a CLI, an MCP server
and FORMS.** ONE OR TWO MORE SENTENCES ON WHAT IT DOES FOR THEM. (OPTIONAL: ONE
SENTENCE ON WHERE THE NAME COMES FROM.)

## Why you would want it

- **OUTCOME ONE**, in the reader's terms.
- **OUTCOME TWO**.
- **OUTCOME THREE**.

The full case, and how TOOLNAME compares with the alternatives:
[Why TOOLNAME](docs/book/src/introduction.md).

## Install

Linux x86_64 and macOS (PLATFORMS): download the checksummed release.

```bash
INSTALL COMMAND FROM THE RELEASE
```

Anywhere else, build from source (needs TOOLCHAIN):

```bash
cargo install --git https://github.com/scbrown/TOOLNAME --locked
```

Check it:

```bash
TOOLNAME --version
```

```text
TOOLNAME X.Y.Z
```

If that prints an older version, another copy is earlier on your `PATH`:
`which -a TOOLNAME`.

## First success in three commands

```bash
FIXTURE SETUP (SELF-CONTAINED: A TEMP DIR, A TINY SAMPLE)
COMMAND ONE
COMMAND TWO
```

```text
THE EXACT OUTPUT A CLEAN RUN PRINTS
```

ONE SENTENCE: WHAT THAT OUTPUT SHOWS AND WHY IT MATTERS.

## On your own code

| you want to know | run |
|---|---|
| QUESTION | `COMMAND` |
| QUESTION | `COMMAND` |
| QUESTION | `COMMAND` |

Every command and flag: [CLI reference](docs/book/src/reference.md).

## Wire it into your agent

```bash
MCP ONE-LINER (e.g. claude mcp add TOOLNAME -- TOOLNAME serve)
```

MINIMAL HOOK OR CONFIG, IF ANY.

Hooks, other agents and every option: [Using TOOLNAME with agents](docs/book/src/agents.md).

## Before you start

| | Linux x86_64 | macOS arm64 | macOS x86_64 | Linux arm64 |
|---|---|---|---|---|
| TOOLNAME | release | release | build | build |

PREREQUISITES, ONE LINE EACH.

## What's next

- [The TOOLNAME book](docs/book/src/introduction.md): start here to go deeper
- [Docs map](docs/book/src/docs-map.md): every document in this repo, routed
- [NEXT PAGE A NEW USER SHOULD READ](docs/book/src/PAGE.md)

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

<!-- mark THIS repo's row: put " **(you are here)**" straight after its repo link -->

## Contributing

```bash
just build
just test
just ci      # what CI runs
```

## 📜 License

[MIT](LICENSE)

<p align="center">
  <img src="assets/footer.svg" width="100%" alt="ALT TEXT FOR THE FOOTER"/>
</p>
