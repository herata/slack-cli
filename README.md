# slack-cli

> A Rust-based Slack CLI for coding agents and shell automation.

`slack-cli` is a command-line Slack client built primarily for **coding agents**
(Claude Code, Codex, Cursor, etc.) and shell automation. It uses a
`noun → verb` command shape and emits stable JSON when piped, so agents can
parse results without scraping.

[日本語版 README はこちら / Japanese README](./README.ja.md)

## Status

**Pre-release.** APIs and JSON output schema (`slack-cli/v0`) are not yet
stable. Pin a tagged release in CI.

## Highlights

- `noun → verb` command tree: `slack-cli channel list`, `slack-cli message send`, ...
- **Auto-detects TTY**: human-friendly table when interactive, structured JSON when piped.
- **Strict stream separation**: data on stdout, logs / progress / errors on stderr.
- **JSON envelope** with `"schema": "slack-cli/v0"` so agents can version-pin output.
- **`--dry-run`** on every write/admin command — returns the exact API call it
  *would* make as JSON, without invoking Slack.
- **`--yes`** required for destructive operations in non-interactive contexts.
- **Idempotent posts**: `chat.postMessage` gets a UUID v4 `client_msg_id` so
  agent retries are safe.
- **Token storage**: OS keychain (default), config file (`0600` on Unix), or
  environment variable — selected per workspace.
- Single static binary via `rustls`; no OpenSSL required.

## Install

> Pre-release. Pre-built binaries (Linux musl, macOS, Windows; x86_64 + aarch64)
> are published on GitHub Releases once the first tag lands.

### From source (requires [`mise`][mise])

```sh
git clone https://github.com/<owner>/slack-cli && cd slack-cli
mise install
cargo install --path .
```

## Quick start

```sh
# 1. Configure a workspace (token is stored in OS keychain by default)
slack-cli auth init

# 2. Verify the token
slack-cli auth whoami --json

# 3. Send a message (dry-run first)
slack-cli message send '#general' --text 'hello from slack-cli' --dry-run
slack-cli message send '#general' --text 'hello from slack-cli'

# 4. Stream a long body from stdin
echo "$(cat report.md)" | slack-cli message send '#general' --text -
```

## Command map

| Group     | Commands                                                 |
|-----------|----------------------------------------------------------|
| `auth`    | `init`, `whoami`, `list`, `logout`                       |
| `channel` | `list`, `view`, `create`, `archive`, `invite`, `leave`   |
| `message` | `list`, `view`, `send`, `reply`, `update`, `delete`, `react` |
| `search`  | `messages`, `files`                                      |
| `user`    | `list`, `view`                                           |
| `file`    | `list`, `upload`, `download`                             |
| `completion` | `bash`, `zsh`, `fish`, `powershell`, `elvish`         |

Run `slack-cli <group> --help` for full flag details. Every group exposes the
same global flags (`--json`, `--workspace`, `--config`, `-v`, `--quiet`,
`--no-color`, `--yes`, `--max-results`, `--all`, `--max-pages`, `--dry-run`).

## JSON output schema

All JSON output is wrapped in a stable envelope:

```jsonc
{
  "ok": true,
  "schema": "slack-cli/v0",
  "data": { /* command-specific */ },
  "request_id": "req_..."
}
```

Errors land on **stderr** with a parallel shape:

```jsonc
{
  "ok": false,
  "schema": "slack-cli/v0",
  "error": {
    "code": "channel_not_found",
    "message": "channel '#nonexistent' not found",
    "hint": { "action": "run", "command": "slack-cli channel list" },
    "exit_code": 5,
    "details": { "kind": "channel", "name": "#nonexistent" },
    "request_id": "req_..."
  }
}
```

List commands always wrap items + cursor:

```jsonc
{ "data": { "items": [...], "next_cursor": "..." } }
```

## Exit codes

| Code | Meaning                                    |
|-----:|--------------------------------------------|
|    0 | Success                                    |
|    1 | Generic failure (network, unexpected I/O)  |
|    2 | Configuration error                        |
|    3 | Authentication failure                     |
|    4 | Rate limited                               |
|    5 | Resource not found / ambiguous identifier  |
|    6 | Token kind mismatch (user vs bot)          |
|   64 | Invalid argument (sysexits `EX_USAGE`)     |
|  130 | Interrupted (SIGINT)                       |

## Configuration

`slack-cli` resolves its config file via [`etcetera`][etcetera]:

- macOS: `~/Library/Application Support/slack-cli/config.toml`
- Linux: `$XDG_CONFIG_HOME/slack-cli/config.toml` (default `~/.config/slack-cli/config.toml`)
- Windows: `%APPDATA%\slack-cli\config.toml`

Override with `-c, --config <PATH>` or `SLACK_CLI_CONFIG`.

## Security

- Tokens are never accepted via `--token` (would leak through `ps`). Use
  `--token-stdin`, the `SLACK_CLI_TOKEN` environment variable, or the OS
  keychain.
- The `Authorization` header is marked sensitive so logs never expose it.
- Release builds use `panic = "abort"` to suppress memory dumps.
- See [`SECURITY.md`](./SECURITY.md) for reporting vulnerabilities (planned).

## Development

```sh
mise install         # Install pinned toolchain + cargo-nextest, insta, deny, git-cliff
mise run test        # fmt --check + clippy -D warnings + nextest + deny
mise run snap        # Review pending insta snapshots
mise run run -- channel list
```

### pre-commit hooks

One-time setup per clone (installs `pre-commit` to your PATH and wires up
`.git/hooks`):

```sh
pip install --user pre-commit   # or: brew install pre-commit / pipx install pre-commit
pre-commit install
pre-commit install --hook-type commit-msg
```

Run all hooks against every tracked file:

```sh
pre-commit run --all-files
```

Configured in `.pre-commit-config.yaml`: file hygiene, gitleaks (secret
scan), markdownlint, Conventional Commits validator (commit-msg), plus
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` as
local hooks. The CI workflow runs the same set on every PR.

## License

Dual-licensed under MIT OR Apache-2.0.

[mise]: https://mise.jdx.dev
[etcetera]: https://crates.io/crates/etcetera
