> Caboodle is becoming **Tambo**. The transition release includes both `tambo`
> and `caboodle`, backed by the same CLI. Existing plan and state paths remain
> compatible. New release archives use `tambo-vVERSION-TARGET.tar.gz` with a
> sibling SHA-256 checksum; supported archives contain both commands.
>
> Repository and registry publication, package-manager migration, and installed
> host verification are separate release steps. The compatibility command stays
> available for at least one release.

<p align="center">
  <img src="assets/header.svg" width="100%" alt="Animated banner — a teal kit box with a tipping lid and three flowing, knotted cords"/>
</p>

<p align="center">
  <img src="assets/logo.svg" width="200" alt="Tambo logo — a light-teal tackle box open with pink fold-out trays, each compartment holding a tool of the stack"/>
</p>

<h1 align="center">tambo</h1>

<p align="center">
  <em>🧰 Agent Platform Toolkit - installs and configures the quipu stack</em>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT"/></a>
  <a href="https://github.com/scbrown/caboodle/actions/workflows/ci.yml"><img src="https://github.com/scbrown/caboodle/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <a href="https://github.com/scbrown/caboodle"><img src="https://img.shields.io/badge/stack-quipu-8B5E3C.svg" alt="Part of the quipu stack"/></a>
</p>

**Caboodle installs and configures a stack built around Quipu, a governed,
bitemporal knowledge store for people and AI coding agents.** Shared facts,
decisions and work history stay available for your agents to query. Its CLI
installs the tools from a reviewable plan and proves each one works; an agent
skill guides you through the same workflow.

## Why you would want it

- Give your agent memory and code context that survive a session.
- Review one plan, then resume installation if it is interrupted.
- Know each tool passed a functional check before you rely on it.

[What the quipu stack is, and three ways to start](https://scbrown.github.io/caboodle/introduction.html).

## Install

**Debian and Ubuntu (amd64):** install from the signed apt repository, which
republishes each checksum-verified release. The package puts `tambo` and the compatibility `caboodle` command in
`/usr/bin`. Check the signing key's fingerprint before trusting it; the
commands stop if it does not match:

```bash
curl -fsSL https://scbrown.github.io/apt/key.gpg -o scbrown-stack.gpg
gpg --show-keys --with-colons scbrown-stack.gpg \
  | grep -qx 'fpr:::::::::AB5625C5612F643BF76464DF0DC99924994EC959:' \
  && sudo install -D -m 0644 scbrown-stack.gpg /etc/apt/keyrings/scbrown-stack.gpg \
  && echo "deb [signed-by=/etc/apt/keyrings/scbrown-stack.gpg] https://scbrown.github.io/apt stable main" \
     | sudo tee /etc/apt/sources.list.d/scbrown-stack.list \
  && sudo apt update && sudo apt install tambo
```

`apt upgrade` then picks up new releases. unattended-upgrades only installs
from origins it allows, so to have it upgrade tambo too, add
`"origin=scbrown,codename=stable";` to `Unattended-Upgrade::Origins-Pattern`.
A host with the older `caboodle` package moves over with
`sudo apt install tambo` (or `apt full-upgrade`): `caboodle 0.3.4-1` is a
transitional package that depends on `tambo`.
Channels, pinning and rollback:
[scbrown/apt](https://github.com/scbrown/apt#channels-and-pinning).

Checksummed releases: Linux x86_64, macOS arm64 and macOS x86_64.
The installer verifies SHA256 before placing the binary in `~/.cargo/bin`.

```bash
curl -fsSL https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh | bash
```

To pin a release, pass the version to the receiving shell:

```bash
curl -fsSL https://raw.githubusercontent.com/scbrown/caboodle/main/scripts/install.sh | CABOODLE_VERSION=vX.Y.Z bash
```

Add the install directory to this shell's PATH:

```bash
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
```

The `export` lasts for this shell only. To keep it, add the same line once to
your shell's startup file: `~/.bashrc` for bash, `~/.zshrc` for zsh (the macOS
default). For bash:

```bash
echo 'export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"' >> ~/.bashrc
```

Apps started outside a terminal, such as the Claude desktop app or an IDE, may
not read that file, which is why the agent step below registers servers by
absolute path.

Or build the latest release from source with Rust (`cargo install --git
https://github.com/scbrown/caboodle --tag vX.Y.Z --locked`). Either way,
`caboodle --version` prints `caboodle X.Y.Z` for the release you installed.

## First success in three commands

Run these in a new, empty directory that will stay put. It is caboodle's own
working directory, NOT the repository you want indexed (that comes in
[Wire it into your agent](#wire-it-into-your-agent)). `retrieval` installs
Quipu, Camayoc and Bobbin, which the agent step needs. To check prerequisites,
Set `QUIPU_SERVER` to the intended authenticated server and obtain an issued
token from its administrator before verification. Install it with
`caboodle provision-quipu-token --from /secure/issued-token --server <server-url>`.
This proves acceptance and creates the canonical credential with `0400/0700`
permissions; it refuses rotation of a differing existing credential. Then
run `caboodle doctor` after `plan` in the same directory (with no plan it checks
`everything`, which needs Go); it exits 0 when ready, and warnings do not block.

```bash
mkdir -p ~/caboodle && cd ~/caboodle
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
hook bundle bobbin: source tool
hook bundle quipu: source shipped copy
hook bundle NOT registered: `st` (shantytown) is not installed on this host. Install it and rerun `caboodle apply`; verify reports the bundles until then
vocabulary: quechua v0.1.0 cached at ~/.local/share/caboodle/vocabulary/quechua-ns-v0.1.0.ttl (no plan quipu_db to load into)
hook bundle NOT verified: `st` (shantytown) is not installed on this host, so no hook bundle is registered here
quipu credential: canonical file 0400/0700 and authenticated read verified; source: default ~/.config/quipu/token
quipu: verified
camayoc: verified
bobbin: verified
```

`hook bundle` lines are expected without Shantytown. Each `verified` is a passed
functional round trip; [the walkthrough](https://scbrown.github.io/caboodle/getting-started.html) covers recovery.

**Success check:** `caboodle install` exits 0 and prints `<tool>: verified` for
every tool in the plan (`quipu`, `camayoc`, `bobbin` for `retrieval`). Recheck at
any time with `caboodle verify` from `~/caboodle`: same lines, exit 0. A failing
tool prints `<tool>: FAILED: …` and the command exits nonzero.

The install leaves these in `~/caboodle`: `caboodle-intent.toml` (downloaded
above), `caboodle-plan.toml` (the reviewed plan) and `.caboodle/` (install
state). Keep the directory; `verify`, `update` and a resumed `install` read it.

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

Tambo is a CLI, with an [agent skill](skills/tambo/SKILL.md), not an MCP
server. Bobbin, which the `retrieval` profile installed above, is one. It
searches one repository: the code you want your agent to work on, such as your
own project's git checkout. Do not use `~/caboodle`, which holds only the plan.
Index that repository, then register bobbin from inside it:

```bash
cd ~/path/to/your/repo
bobbin init
bobbin index
claude mcp add bobbin -- "$(command -v bobbin)" serve "$PWD"
```

`$(command -v bobbin)` and `$PWD` are expanded once, now, so Claude Code
starts the server by absolute path and serves this repository whatever its own
`PATH` or working directory. Registering a bare `bobbin` fails with `ENOENT` when
Claude Code was not started from a shell that has `~/.cargo/bin` on `PATH`.

`claude mcp add` uses the default `local` scope: the server is registered for
this project only, so start Claude Code in this repository to see it (`claude
mcp list` there shows `bobbin: ... ✓ Connected`). Repeat the four lines in each
repository you want searchable. `bobbin init` creates `.bobbin/`
in the repository and, if the repository has a `.gitignore`, appends `.bobbin/`
to it; review that change before you commit. Without `bobbin init`, `bobbin
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

<!-- stack-table:start -->

The Quipu stack is built around a governed, bitemporal knowledge store: shared
facts, decisions and work history your agents can query. Quipu can check writes
against your rules, records where each fact came from, and answers what was true
when.

| tool | what it gives your agents |
|---|---|
| [caboodle](https://github.com/scbrown/caboodle) | 🧰 Agent Platform Toolkit - installs and configures the quipu stack |
| [quipu](https://github.com/scbrown/quipu) | stores shared facts with provenance and history; can check writes against your rules |
| [camayoc](https://github.com/scbrown/camayoc) | prepares knowledge and vocabulary for Quipu, with checks before it is admitted |
| [bobbin](https://github.com/scbrown/bobbin) | searches your code and can include related knowledge from Quipu |
| [yupana](https://github.com/scbrown/yupana) | shows code impact and can read policies from Quipu to check changes |
| [desire-path](https://github.com/scbrown/desire-path) | turns agent mistakes into feature requests; works on its own without Quipu |
| [seeds](https://github.com/scbrown/seeds) | a work item tracker backed by the Quipu knowledge base |
| [shantytown](https://github.com/scbrown/shantytown) | runs crews of AI agents on work items, triggered by hand, on a schedule or by events; can share work and memory through Quipu |
| [shuttle](https://github.com/scbrown/shuttle) | runs signed agent workflows and keeps their records in Quipu |

<!-- stack-table:end -->

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
