# Using it with agents

Caboodle is a CLI that an agent can run with the
[Caboodle skill](https://github.com/scbrown/caboodle/blob/main/skills/caboodle/SKILL.md).
It does not expose its own MCP server. MCP (Model Context Protocol) lets an
agent call servers supplied by the installed tools.

## Standalone agent

After installing the retrieval profile, register Bobbin with Claude Code:

```bash
claude mcp add bobbin -- bobbin serve
```

For code-intel or everything, also register Yupana:

```bash
claude mcp add yupana -- yupana serve
claude mcp list
```

Index a repository before searching it: run `bobbin init` and `bobbin index`
in that repository. Other MCP clients use the same executable and arguments
in their client-specific server configuration.

## Managed crew

For a configured Shantytown rig, `caboodle project-settings` delegates
registration to Shantytown and its Quipu tooling manifest. It does not launch
or restart agents. See [registering MCP servers](installing.md#registering-mcp-servers)
for prerequisites, selection flags, remote proxies and the policy-only mode.
