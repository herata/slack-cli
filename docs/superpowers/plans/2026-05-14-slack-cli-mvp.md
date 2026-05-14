# slack-cli MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust-based Slack CLI (`slack-cli`) for coding agents that mirrors the `jira-cli` UX, exposing channels / messages / files / search / users / auth with a stable `slack-cli/v0` JSON envelope, TTY-aware output, and a 3-layer test pyramid.

**Architecture:** Single binary crate, `#[tokio::main(flavor = "current_thread")]`, `reqwest + serde` direct calls to Slack Web API (no SDK), `Result<T, CliError>` only (no `anyhow`), `tabled` + `serde` + `Display` for rendering. Secrets via OS keychain by default with file/env fallbacks. Tests with `wiremock` for HTTP mocking and `assert_cmd` + `insta` for E2E.

**Tech Stack:** Rust 1.89 (MSRV), tokio, reqwest, serde, clap (derive), thiserror, tracing, tabled, etcetera, keyring v3, secrecy, toml_edit, uuid (v4), clap_complete. Dev: wiremock, assert_cmd, insta, predicates, pretty_assertions. Tooling: mise (toolchain + cargo-nextest/insta/deny/git-cliff).

**Source spec:** `docs/superpowers/specs/2026-05-14-slack-cli-design.md`

---

## File Structure

```
slack-cli/
├─ Cargo.toml                 # crate metadata + deps + lints + release profile
├─ deny.toml                  # cargo-deny config
├─ cliff.toml                 # git-cliff config
├─ mise.toml                  # already present
├─ src/
│  ├─ main.rs                 # #[tokio::main(current_thread)], install panic hook, dispatch
│  ├─ lib.rs                  # re-exports for integration tests
│  ├─ logging.rs              # tracing init (stderr, EnvFilter forced floor)
│  ├─ cli.rs                  # clap derive: Cli + GlobalArgs + Subcommand enums
│  ├─ error.rs                # CliError (thiserror) + exit_code() + hint()
│  ├─ id.rs                   # Newtypes + ChannelRef / UserRef + TryFrom
│  ├─ output.rs               # Format enum + render() + JSON envelope
│  ├─ config.rs               # TOML I/O via toml_edit, etcetera-based paths
│  ├─ secret.rs               # TokenStore (Keyring/File/Env) + SecretString
│  ├─ slack.rs                # SlackClient (reqwest) + DTOs + error mapping + retry
│  ├─ resolve.rs              # Resolver: name→ID with 60s in-process cache
│  └─ cmd/
│     ├─ mod.rs               # Ctx, dispatch()
│     ├─ auth.rs              # init / whoami / list / logout
│     ├─ channel.rs           # list / view / create / archive / invite / leave
│     ├─ message.rs           # list / view / send / reply / update / delete / react
│     ├─ search.rs            # messages / files
│     ├─ user.rs              # list / view
│     ├─ file.rs              # list / upload / download
│     └─ completion.rs        # clap_complete generator
├─ tests/
│  ├─ common/mod.rs           # wiremock setup, temp config, env helpers
│  ├─ auth.rs                 # E2E auth
│  ├─ channel.rs              # E2E channel
│  ├─ message.rs              # E2E message + dry-run
│  ├─ search.rs               # E2E search
│  ├─ user.rs                 # E2E user
│  ├─ file.rs                 # E2E file
│  └─ fixtures/*.json         # Slack API sample responses
└─ .github/
   ├─ dependabot.yml
   └─ workflows/
      ├─ ci.yml
      └─ release.yml
```

---

## Phase 0 — Project bootstrap

### Task 0.1: Initialize Cargo crate

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: empty module stubs under `src/`

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "slack-cli"
version = "0.1.0"
edition = "2021"
rust-version = "1.89"
license = "MIT OR Apache-2.0"
description = "Slack CLI for coding agents (jira-cli style)"
repository = "https://github.com/<owner>/slack-cli"
default-run = "slack-cli"

[[bin]]
name = "slack-cli"
path = "src/main.rs"

[lib]
path = "src/lib.rs"

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
opt-level = "z"
panic = "abort"

[dependencies]

[dev-dependencies]
```

- [ ] **Step 2: Write `src/lib.rs`**

```rust
//! Internal library facade for `slack-cli`.
//!
//! Exposed only so integration tests under `tests/` can reach
//! the same modules the binary uses. Not a stable public API.

#![warn(missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc, clippy::missing_panics_doc)]

pub mod cli;
pub mod cmd;
pub mod config;
pub mod error;
pub mod id;
pub mod logging;
pub mod output;
pub mod resolve;
pub mod secret;
pub mod slack;
```

- [ ] **Step 3: Write minimal `src/main.rs`**

```rust
use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("slack-cli scaffolding (no commands wired yet)");
    ExitCode::SUCCESS
}
```

- [ ] **Step 4: Create empty module stubs**

Run:

```bash
mkdir -p src/cmd
for f in cli error id logging output config secret slack resolve; do
  echo "// placeholder, filled in later phases" > "src/$f.rs"
done
echo "// placeholder, filled in later phases" > src/cmd/mod.rs
```

- [ ] **Step 5: Run `cargo check`**

Run: `cargo check`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/
git commit -m "chore: bootstrap empty cargo crate with module skeleton"
```

### Task 0.2: Add `.github/dependabot.yml`, `deny.toml`, `cliff.toml`

**Files:**
- Create: `deny.toml`
- Create: `cliff.toml`
- Create: `.github/dependabot.yml`

- [ ] **Step 1: Write `deny.toml`**

```toml
[advisories]
version = 2
yanked = "warn"

[bans]
multiple-versions = "warn"
wildcards = "deny"

[licenses]
version = 2
allow = [
  "MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause", "BSD-3-Clause", "ISC",
  "Unicode-DFS-2016", "Unicode-3.0", "CC0-1.0", "MPL-2.0", "Zlib",
]
confidence-threshold = 0.93

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

- [ ] **Step 2: Write `cliff.toml`**

```toml
[changelog]
header = "# Changelog\n\nAll notable changes to this project are documented here.\n"
body = """
{% if version %}
## [{{ version | trim_start_matches(pat="v") }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% else %}
## [Unreleased]
{% endif %}
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }}
{% endfor %}
{% endfor %}
"""
trim = true

[git]
conventional_commits = true
filter_unconventional = true
commit_parsers = [
  { message = "^feat", group = "Features" },
  { message = "^fix", group = "Bug Fixes" },
  { message = "^docs", group = "Documentation" },
  { message = "^perf", group = "Performance" },
  { message = "^refactor", group = "Refactor" },
  { message = "^test", group = "Tests" },
  { message = "^chore", group = "Chores" },
  { message = "^ci", group = "CI" },
]
filter_commits = false
tag_pattern = "v[0-9]*"
```

- [ ] **Step 3: Write `.github/dependabot.yml`**

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5

  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5
```

- [ ] **Step 4: Commit**

```bash
git add deny.toml cliff.toml .github/dependabot.yml
git commit -m "chore: add cargo-deny, git-cliff, and dependabot configs"
```

---

## Phase 1 — Foundation types

### Task 1.1: CliError enum

**Files:** Modify `src/error.rs`, `Cargo.toml`.

- [ ] **Step 1: Add `thiserror` dep**

```toml
thiserror = "1"
```

- [ ] **Step 2: Write `src/error.rs`**

```rust
//! CLI-wide error type. All commands return `Result<T, CliError>`.

use std::io;

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CliError {
    #[error("no token configured for workspace '{workspace}'")]
    NoToken { workspace: String },

    #[error("ambiguous workspace: specify --workspace (available: {})", available.join(", "))]
    AmbiguousWorkspace { available: Vec<String> },

    #[error("token kind mismatch: '{api}' requires {needed} token, got {actual}")]
    TokenKindMismatch {
        needed: &'static str,
        actual: &'static str,
        api: &'static str,
    },

    #[error("{kind} '{name}' not found")]
    NotFound { kind: &'static str, name: String },

    #[error("ambiguous {kind} reference '{name}' ({} candidates)", candidates.len())]
    Ambiguous {
        kind: &'static str,
        name: String,
        candidates: Vec<String>,
    },

    #[error("slack api error: {code}")]
    SlackApi {
        code: String,
        request_id: Option<String>,
    },

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("invalid argument: {0}")]
    InvalidArg(String),

    #[error("confirmation required: pass --yes for non-interactive use")]
    ConfirmationRequired,

    #[error("file token backend is not supported on this platform")]
    FileBackendUnsupported,

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("network error: {0}")]
    Network(String),
}

impl CliError {
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NoToken { .. } | Self::AuthFailed { .. } => 3,
            Self::AmbiguousWorkspace { .. } | Self::Config(_) | Self::FileBackendUnsupported => 2,
            Self::RateLimited { .. } => 4,
            Self::NotFound { .. } | Self::Ambiguous { .. } => 5,
            Self::TokenKindMismatch { .. } => 6,
            Self::InvalidArg(_) | Self::ConfirmationRequired => 64,
            _ => 1,
        }
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoToken { .. } => "no_token",
            Self::AmbiguousWorkspace { .. } => "workspace_ambiguous",
            Self::TokenKindMismatch { .. } => "token_kind_mismatch",
            Self::NotFound { .. } => "not_found",
            Self::Ambiguous { .. } => "ambiguous",
            Self::SlackApi { .. } => "slack_api",
            Self::RateLimited { .. } => "rate_limited",
            Self::AuthFailed { .. } => "auth_failed",
            Self::InvalidArg(_) => "invalid_arg",
            Self::ConfirmationRequired => "confirmation_required",
            Self::FileBackendUnsupported => "file_backend_unsupported",
            Self::Config(_) => "config",
            Self::Io(_) => "io",
            Self::Network(_) => "network",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_auth_failed_is_3() {
        let e = CliError::AuthFailed { reason: "x".into() };
        assert_eq!(e.exit_code(), 3);
    }
    #[test]
    fn exit_code_not_found_is_5() {
        let e = CliError::NotFound { kind: "channel", name: "x".into() };
        assert_eq!(e.exit_code(), 5);
    }
    #[test]
    fn exit_code_token_mismatch_is_6() {
        let e = CliError::TokenKindMismatch { needed: "user", actual: "bot", api: "search.messages" };
        assert_eq!(e.exit_code(), 6);
    }
    #[test]
    fn exit_code_invalid_arg_is_64() {
        assert_eq!(CliError::InvalidArg("x".into()).exit_code(), 64);
    }
    #[test]
    fn display_lowercase_no_period() {
        let s = CliError::NoToken { workspace: "acme".into() }.to_string();
        assert!(s.starts_with(char::is_lowercase));
        assert!(!s.ends_with('.'));
    }
}
```

- [ ] **Step 3: Run tests + clippy + commit**

```bash
cargo test --lib error
cargo clippy --all-targets -- -D warnings
git add Cargo.toml src/error.rs
git commit -m "feat: add CliError enum with exit-code and code mappings"
```

### Task 1.2: Identifier Newtypes

**Files:** Modify `src/id.rs`, `Cargo.toml`.

- [ ] **Step 1: Add deps**

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
regex = "1"
```

- [ ] **Step 2: Write `src/id.rs`**

```rust
//! Strongly-typed Slack identifiers.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::CliError;

static ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[CGDUW][A-Z0-9]{8,}$").expect("valid regex"));

macro_rules! string_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub(crate) String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str { &self.0 }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(ChannelId, "Slack channel id (`Cxxxx`, `Gxxxx`, `Dxxxx`).");
string_id!(UserId,    "Slack user id (`Uxxxx`, `Wxxxx`).");
string_id!(Ts,        "Slack message timestamp.");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelRef { Name(String), Id(ChannelId) }

impl TryFrom<&str> for ChannelRef {
    type Error = CliError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CliError::InvalidArg("empty channel reference".into()));
        }
        let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
        if body.is_empty() {
            return Err(CliError::InvalidArg("channel reference is only '#'".into()));
        }
        if body.starts_with(['C', 'G', 'D']) && ID_RE.is_match(body) {
            Ok(Self::Id(ChannelId(body.to_owned())))
        } else {
            Ok(Self::Name(body.to_owned()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef { Name(String), Id(UserId) }

impl TryFrom<&str> for UserRef {
    type Error = CliError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CliError::InvalidArg("empty user reference".into()));
        }
        let body = trimmed.strip_prefix('@').unwrap_or(trimmed);
        if body.is_empty() {
            return Err(CliError::InvalidArg("user reference is only '@'".into()));
        }
        if body.starts_with(['U', 'W']) && ID_RE.is_match(body) {
            Ok(Self::Id(UserId(body.to_owned())))
        } else {
            Ok(Self::Name(body.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn parses_hash_name() {
        assert_eq!(ChannelRef::try_from("#general").unwrap(), ChannelRef::Name("general".into()));
    }
    #[test] fn parses_bare_name() {
        assert_eq!(ChannelRef::try_from("general").unwrap(), ChannelRef::Name("general".into()));
    }
    #[test] fn parses_id() {
        assert_eq!(
            ChannelRef::try_from("C012ABCD9").unwrap(),
            ChannelRef::Id(ChannelId("C012ABCD9".into())),
        );
    }
    #[test] fn rejects_empty() {
        assert!(ChannelRef::try_from("").is_err());
        assert!(ChannelRef::try_from("#").is_err());
    }
    #[test] fn user_ref_at_name() {
        assert_eq!(UserRef::try_from("@alice").unwrap(), UserRef::Name("alice".into()));
    }
    #[test] fn user_ref_enterprise() {
        assert_eq!(UserRef::try_from("W0123ABCD").unwrap(), UserRef::Id(UserId("W0123ABCD".into())));
    }
    #[test] fn short_id_lookalike_is_name() {
        assert_eq!(ChannelRef::try_from("C123").unwrap(), ChannelRef::Name("C123".into()));
    }
}
```

- [ ] **Step 3: Test + clippy + commit**

```bash
cargo test --lib id
cargo clippy --all-targets -- -D warnings
git add Cargo.toml src/id.rs
git commit -m "feat: add ChannelId/UserId/Ts newtypes and Ref parsers"
```

### Task 1.3: Format enum, JSON envelope, render helpers

**Files:** Modify `src/output.rs`, `Cargo.toml`.

- [ ] **Step 1: Add deps**

```toml
tabled = "0.16"
clap = { version = "4", features = ["derive", "wrap_help"] }
```

- [ ] **Step 2: Write `src/output.rs`**

```rust
//! Output formatting and the stable `slack-cli/v0` JSON envelope.

use std::io::{self, IsTerminal, Write};

use serde::Serialize;

use crate::error::CliError;

pub const SCHEMA: &str = "slack-cli/v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[non_exhaustive]
pub enum Format { Table, Plain, Json }

impl Format {
    #[must_use]
    pub fn resolve(flag: Option<Format>, stdout_is_tty: bool) -> Self {
        match (flag, stdout_is_tty) {
            (Some(f), _) => f,
            (None, true) => Format::Table,
            (None, false) => Format::Json,
        }
    }
    #[must_use]
    pub fn detect_default() -> Self {
        Self::resolve(None, io::stdout().is_terminal())
    }
}

#[derive(Debug, Serialize)]
pub struct OkEnvelope<'a, T: Serialize> {
    pub ok: bool,
    pub schema: &'a str,
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListPayload<'a, T: Serialize> {
    pub items: &'a [T],
    pub next_cursor: String,
}

pub fn render_one<T>(
    item: &T,
    fmt: Format,
    pretty_json: bool,
    request_id: Option<String>,
    w: &mut dyn Write,
) -> io::Result<()>
where
    T: tabled::Tabled + Serialize,
{
    match fmt {
        Format::Table => {
            let table = tabled::Table::new([item]).to_string();
            writeln!(w, "{table}")
        }
        Format::Plain => {
            let fields = T::fields(item);
            let line = fields.iter().map(std::convert::AsRef::as_ref).collect::<Vec<_>>().join("\t");
            writeln!(w, "{line}")
        }
        Format::Json => {
            let env = OkEnvelope { ok: true, schema: SCHEMA, data: item, request_id };
            write_json(w, &env, pretty_json)
        }
    }
}

pub fn render_list<T>(
    items: &[T],
    next_cursor: &str,
    fmt: Format,
    pretty_json: bool,
    request_id: Option<String>,
    w: &mut dyn Write,
) -> io::Result<()>
where
    T: tabled::Tabled + Serialize,
{
    match fmt {
        Format::Table => {
            let table = tabled::Table::new(items).to_string();
            writeln!(w, "{table}")
        }
        Format::Plain => {
            for item in items {
                let fields = T::fields(item);
                let line = fields.iter().map(std::convert::AsRef::as_ref).collect::<Vec<_>>().join("\t");
                writeln!(w, "{line}")?;
            }
            Ok(())
        }
        Format::Json => {
            let payload = ListPayload { items, next_cursor: next_cursor.to_owned() };
            let env = OkEnvelope { ok: true, schema: SCHEMA, data: payload, request_id };
            write_json(w, &env, pretty_json)
        }
    }
}

fn write_json<T: Serialize>(w: &mut dyn Write, v: &T, pretty: bool) -> io::Result<()> {
    if pretty {
        serde_json::to_writer_pretty(&mut *w, v).map_err(io::Error::other)?;
    } else {
        serde_json::to_writer(&mut *w, v).map_err(io::Error::other)?;
    }
    writeln!(w)
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum Hint {
    Run { command: String },
    Specify { flag: String },
    List { command: String },
}

#[derive(Debug, Serialize)]
pub struct ErrEnvelope<'a> {
    pub ok: bool,
    pub schema: &'a str,
    pub error: ErrPayload,
}

#[derive(Debug, Serialize)]
pub struct ErrPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<Hint>,
    pub exit_code: u8,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[must_use]
pub fn hint_for_error(err: &CliError) -> Option<Hint> {
    match err {
        CliError::NoToken { .. } => Some(Hint::Run { command: "slack-cli auth init".into() }),
        CliError::AmbiguousWorkspace { .. } => Some(Hint::Specify { flag: "--workspace".into() }),
        CliError::NotFound { kind: "channel", .. } | CliError::Ambiguous { kind: "channel", .. } =>
            Some(Hint::List { command: "slack-cli channel list".into() }),
        CliError::NotFound { kind: "user", .. } | CliError::Ambiguous { kind: "user", .. } =>
            Some(Hint::List { command: "slack-cli user list".into() }),
        _ => None,
    }
}

pub fn render_error(err: &CliError, json: bool, w: &mut dyn Write) -> io::Result<()> {
    if json {
        let payload = ErrPayload {
            code: err.code().to_owned(),
            message: err.to_string(),
            hint: hint_for_error(err),
            exit_code: err.exit_code(),
            details: serde_json::Value::Null,
            request_id: None,
        };
        let env = ErrEnvelope { ok: false, schema: SCHEMA, error: payload };
        write_json(w, &env, false)
    } else {
        writeln!(w, "error: {err}")?;
        if let Some(h) = hint_for_error(err) {
            match h {
                Hint::Run { command } => writeln!(w, "hint:  run `{command}`")?,
                Hint::Specify { flag } => writeln!(w, "hint:  specify `{flag}`")?,
                Hint::List { command } => writeln!(w, "hint:  list with `{command}`")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use tabled::Tabled;

    #[derive(Tabled, Serialize)]
    struct Row { id: &'static str, name: &'static str }

    #[test] fn resolve_explicit() {
        assert_eq!(Format::resolve(Some(Format::Plain), true), Format::Plain);
    }
    #[test] fn resolve_tty() {
        assert_eq!(Format::resolve(None, true), Format::Table);
    }
    #[test] fn resolve_pipe() {
        assert_eq!(Format::resolve(None, false), Format::Json);
    }
    #[test] fn one_json_envelope() {
        let row = Row { id: "C1", name: "general" };
        let mut buf = Vec::new();
        render_one(&row, Format::Json, false, None, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"ok\":true"));
        assert!(s.contains("\"schema\":\"slack-cli/v0\""));
        assert!(s.contains("\"id\":\"C1\""));
    }
    #[test] fn list_json_has_cursor() {
        let rows = [Row { id: "C1", name: "general" }];
        let mut buf = Vec::new();
        render_list(&rows, "next", Format::Json, false, None, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"items\""));
        assert!(s.contains("\"next_cursor\":\"next\""));
    }
    #[test] fn hint_run_for_no_token() {
        let h = hint_for_error(&CliError::NoToken { workspace: "acme".into() }).unwrap();
        assert!(matches!(h, Hint::Run { .. }));
    }
}
```

- [ ] **Step 3: Test + clippy + commit**

```bash
cargo test --lib output
cargo clippy --all-targets -- -D warnings
git add Cargo.toml src/output.rs
git commit -m "feat: add Format enum, JSON envelopes, and render helpers"
```

---

## Phase 2 — Config and secrets

### Task 2.1: Config TOML I/O

**Files:** Modify `src/config.rs`, `Cargo.toml`.

- [ ] **Step 1: Add deps**

```toml
etcetera = "0.8"
toml_edit = { version = "0.22", features = ["serde"] }
tempfile = "3"
```

Also add `tempfile = "3"` under `[dev-dependencies]` (cargo dedupes; both entries are fine).

- [ ] **Step 2: Write `src/config.rs`**

```rust
//! Configuration file I/O.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use etcetera::{choose_base_strategy, BaseStrategy};
use serde::{Deserialize, Serialize};

use crate::error::CliError;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub default_workspace: Option<String>,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")] pub format: String,
    #[serde(default = "default_color")]  pub color: String,
    #[serde(default)]                    pub utc: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { format: default_format(), color: default_color(), utc: false }
    }
}

fn default_format() -> String { "auto".into() }
fn default_color()  -> String { "auto".into() }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceConfig {
    pub team_id: Option<String>,
    pub team_domain: Option<String>,
    pub token_source: TokenSource,
    pub token: Option<String>,
    pub token_env: Option<String>,
    #[serde(default)] pub meta: WorkspaceMeta,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TokenSource { Keyring, File, Env }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorkspaceMeta {
    pub token_kind: Option<String>,
    pub user_id: Option<String>,
    pub updated_at: Option<String>,
}

pub fn resolve_path(cli: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(p) = cli {
        return Ok(p.to_path_buf());
    }
    if let Ok(envp) = std::env::var("SLACK_CLI_CONFIG") {
        return Ok(PathBuf::from(envp));
    }
    let strat = choose_base_strategy().map_err(|e| CliError::Config(e.to_string()))?;
    Ok(strat.config_dir().join("slack-cli").join("config.toml"))
}

pub fn load(path: &Path) -> Result<Config, CliError> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(path)?;
    toml_edit::de::from_str(&raw).map_err(|e| CliError::Config(e.to_string()))
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), CliError> {
    let parent = path.parent().ok_or_else(|| CliError::Config("path has no parent".into()))?;
    std::fs::create_dir_all(parent)?;

    let serialized = toml_edit::ser::to_string_pretty(cfg)
        .map_err(|e| CliError::Config(e.to_string()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(serialized.as_bytes())?;
    tmp.as_file().sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))?;
    }

    tmp.persist(path).map_err(|e| CliError::Io(e.error))?;
    Ok(())
}

/// Pick the workspace to use given CLI/env/default precedence.
pub fn pick_workspace<'a>(
    cfg: &'a Config,
    cli: Option<&str>,
) -> Result<(&'a str, &'a WorkspaceConfig), CliError> {
    let env_ws = std::env::var("SLACK_CLI_WORKSPACE").ok();
    let name = cli.map(str::to_owned)
        .or(env_ws)
        .or_else(|| cfg.default_workspace.clone())
        .or_else(|| if cfg.workspaces.len() == 1 {
            cfg.workspaces.keys().next().cloned()
        } else { None });

    let name = name.ok_or_else(|| CliError::AmbiguousWorkspace {
        available: cfg.workspaces.keys().cloned().collect(),
    })?;
    let ws = cfg.workspaces.get(&name).ok_or_else(|| CliError::AmbiguousWorkspace {
        available: cfg.workspaces.keys().cloned().collect(),
    })?;
    // SAFETY: `name` lives in cfg only through workspaces lookup, but we need
    // a &str tied to cfg. Use the key from the map to return a reference.
    let key = cfg.workspaces.get_key_value(&name).expect("just checked").0.as_str();
    Ok((key, ws))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let cfg = load(&p).unwrap();
        assert!(cfg.default_workspace.is_none());
    }

    #[test] fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a").join("b").join("config.toml");
        let mut cfg = Config::default();
        cfg.default_workspace = Some("acme".into());
        cfg.workspaces.insert("acme".into(), WorkspaceConfig {
            team_id: Some("T1".into()),
            team_domain: Some("acme".into()),
            token_source: TokenSource::Keyring,
            token: None,
            token_env: None,
            meta: WorkspaceMeta::default(),
        });
        save(&p, &cfg).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.default_workspace.as_deref(), Some("acme"));
    }

    #[cfg(unix)]
    #[test] fn save_sets_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        save(&p, &Config::default()).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test] fn pick_workspace_uses_default() {
        let mut cfg = Config::default();
        cfg.default_workspace = Some("acme".into());
        cfg.workspaces.insert("acme".into(), WorkspaceConfig {
            team_id: None, team_domain: None,
            token_source: TokenSource::Keyring,
            token: None, token_env: None,
            meta: WorkspaceMeta::default(),
        });
        let (name, _) = pick_workspace(&cfg, None).unwrap();
        assert_eq!(name, "acme");
    }
}
```

- [ ] **Step 3: Test + clippy + commit**

```bash
cargo test --lib config
cargo clippy --all-targets -- -D warnings
git add Cargo.toml src/config.rs
git commit -m "feat: TOML config with atomic save and workspace picker"
```

### Task 2.2: TokenStore with prefix validation (env + file backends)

**Files:** Modify `src/secret.rs`, `Cargo.toml`.

- [ ] **Step 1: Add `secrecy`**

```toml
secrecy = "0.10"
```

- [ ] **Step 2: Write `src/secret.rs`**

```rust
//! Token storage with three backends: keyring (OS), file (config.toml), env.

use secrecy::{ExposeSecret, SecretString};

use crate::config::{TokenSource, WorkspaceConfig};
use crate::error::CliError;

pub fn validate_prefix(raw: &str) -> Result<(), CliError> {
    if raw.starts_with("xoxp-") || raw.starts_with("xoxb-") {
        Ok(())
    } else {
        Err(CliError::InvalidArg("token must start with 'xoxp-' or 'xoxb-'".into()))
    }
}

pub fn load(workspace_name: &str, ws: &WorkspaceConfig) -> Result<SecretString, CliError> {
    if let Ok(t) = std::env::var("SLACK_CLI_TOKEN") {
        validate_prefix(&t)?;
        return Ok(SecretString::from(t));
    }
    match ws.token_source {
        TokenSource::Env => {
            let name = ws.token_env.as_deref()
                .ok_or_else(|| CliError::Config(format!("workspace '{workspace_name}' has no token_env")))?;
            let t = std::env::var(name)
                .map_err(|_| CliError::NoToken { workspace: workspace_name.into() })?;
            validate_prefix(&t)?;
            Ok(SecretString::from(t))
        }
        TokenSource::File => {
            #[cfg(target_os = "windows")]
            return Err(CliError::FileBackendUnsupported);
            #[cfg(not(target_os = "windows"))]
            {
                let t = ws.token.as_deref()
                    .ok_or_else(|| CliError::NoToken { workspace: workspace_name.into() })?;
                validate_prefix(t)?;
                Ok(SecretString::from(t.to_owned()))
            }
        }
        TokenSource::Keyring => keyring_load(workspace_name),
    }
}

pub fn save(workspace_name: &str, ws: &mut WorkspaceConfig, token: &SecretString) -> Result<(), CliError> {
    validate_prefix(token.expose_secret())?;
    match ws.token_source {
        TokenSource::Env => Ok(()),
        TokenSource::File => {
            #[cfg(target_os = "windows")]
            return Err(CliError::FileBackendUnsupported);
            #[cfg(not(target_os = "windows"))]
            {
                ws.token = Some(token.expose_secret().to_owned());
                Ok(())
            }
        }
        TokenSource::Keyring => keyring_save(workspace_name, token),
    }
}

pub fn delete(workspace_name: &str, ws: &mut WorkspaceConfig) -> Result<(), CliError> {
    match ws.token_source {
        TokenSource::Env => Ok(()),
        TokenSource::File => {
            ws.token = None;
            Ok(())
        }
        TokenSource::Keyring => keyring_delete(workspace_name),
    }
}

fn keyring_entry(workspace_name: &str) -> Result<keyring::Entry, CliError> {
    keyring::Entry::new("slack-cli", workspace_name)
        .map_err(|e| CliError::Config(format!("keyring init failed: {e}")))
}

fn keyring_load(workspace_name: &str) -> Result<SecretString, CliError> {
    let entry = keyring_entry(workspace_name)?;
    match entry.get_password() {
        Ok(t) => {
            validate_prefix(&t)?;
            Ok(SecretString::from(t))
        }
        Err(keyring::Error::NoEntry) => Err(CliError::NoToken { workspace: workspace_name.into() }),
        Err(e) => {
            tracing::warn!(error = %e, "keyring load failed");
            Err(CliError::Config(format!("keyring error: {e}")))
        }
    }
}

fn keyring_save(workspace_name: &str, token: &SecretString) -> Result<(), CliError> {
    keyring_entry(workspace_name)?
        .set_password(token.expose_secret())
        .map_err(|e| CliError::Config(format!("keyring error: {e}")))
}

fn keyring_delete(workspace_name: &str) -> Result<(), CliError> {
    match keyring_entry(workspace_name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CliError::Config(format!("keyring error: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TokenSource, WorkspaceConfig, WorkspaceMeta};

    fn ws(src: TokenSource) -> WorkspaceConfig {
        WorkspaceConfig {
            team_id: None, team_domain: None,
            token_source: src,
            token: None, token_env: None,
            meta: WorkspaceMeta::default(),
        }
    }

    #[test] fn validate_accepts_xoxp_and_xoxb() {
        assert!(validate_prefix("xoxp-1-2-3").is_ok());
        assert!(validate_prefix("xoxb-1-2-3").is_ok());
    }
    #[test] fn validate_rejects_garbage() {
        assert!(validate_prefix("hello").is_err());
    }
    #[cfg(not(target_os = "windows"))]
    #[test] fn file_save_then_load() {
        let mut w = ws(TokenSource::File);
        let secret = SecretString::from("xoxp-1-2-3");
        save("acme", &mut w, &secret).unwrap();
        let loaded = load("acme", &w).unwrap();
        assert_eq!(loaded.expose_secret(), "xoxp-1-2-3");
    }
}
```

- [ ] **Step 3: Add keyring deps in `Cargo.toml`**

```toml
[target.'cfg(target_os = "macos")'.dependencies]
keyring = { version = "3", features = ["apple-native"] }

[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "3", features = ["sync-secret-service"] }

[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3", features = ["windows-native"] }

# Always pull `tracing` for the keyring warn!() call.
tracing = "0.1"
```

(Move `tracing` to the top-level `[dependencies]` section, not OS-specific.)

- [ ] **Step 4: Test + clippy + commit**

```bash
cargo test --lib secret
cargo clippy --all-targets -- -D warnings
git add Cargo.toml src/secret.rs
git commit -m "feat: TokenStore with keyring/file/env backends and prefix validation"
```

---

## Phase 3 — Slack HTTP client

### Task 3.1: SlackClient skeleton + `auth.test`

**Files:** Modify `src/slack.rs`, `Cargo.toml`.

- [ ] **Step 1: Add deps**

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls-webpki-roots", "multipart", "stream"] }
tokio = { version = "1", features = ["macros", "rt", "fs", "io-util", "signal"] }
uuid = { version = "1", features = ["v4"] }
```

Update `dev-dependencies` to add `wiremock`:

```toml
[dev-dependencies]
wiremock = "0.6"
tokio = { version = "1", features = ["macros", "rt", "rt-multi-thread", "test-util"] }
```

- [ ] **Step 2: Write `src/slack.rs` skeleton with `auth.test`**

```rust
//! Thin Slack Web API client. No SDK; direct `reqwest` calls.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tracing::instrument;

use crate::error::CliError;

const DEFAULT_BASE_URL: &str = "https://slack.com";

#[derive(Debug)]
pub struct SlackClient {
    base_url: String,
    http: reqwest::Client,
    token: SecretString,
}

impl SlackClient {
    pub fn new(token: SecretString) -> Result<Self, CliError> {
        Self::build(DEFAULT_BASE_URL.into(), token)
    }

    pub fn with_base_url(base_url: String, token: SecretString) -> Result<Self, CliError> {
        Self::build(base_url, token)
    }

    fn build(base_url: String, token: SecretString) -> Result<Self, CliError> {
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
            .map_err(|e| CliError::Config(format!("bad token: {e}")))?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(concat!("slack-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CliError::Network(e.to_string()))?;
        Ok(Self { base_url, http, token })
    }

    fn _token(&self) -> &SecretString { &self.token }

    #[instrument(skip(self))]
    pub async fn auth_test(&self) -> Result<AuthTest, CliError> {
        let url = format!("{}/api/auth.test", self.base_url);
        let resp = self.http.post(&url).send().await
            .map_err(|e| CliError::Network(e.to_string()))?;
        parse_slack::<AuthTest>(resp).await
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AuthTest {
    pub ok: bool,
    pub url: Option<String>,
    pub team: Option<String>,
    pub user: Option<String>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub bot_id: Option<String>,
    pub error: Option<String>,
}

async fn parse_slack<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> Result<T, CliError> {
    let request_id = resp.headers().get("x-slack-req-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let status = resp.status();
    let body = resp.text().await.map_err(|e| CliError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(CliError::SlackApi {
            code: format!("http_{}", status.as_u16()),
            request_id,
        });
    }
    // Slack returns 200 + ok:false.
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| CliError::SlackApi { code: format!("decode: {e}"), request_id: request_id.clone() })?;
    if v.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let code = v.get("error").and_then(|x| x.as_str()).unwrap_or("unknown").to_owned();
        return Err(map_slack_error(&code, request_id));
    }
    let parsed: T = serde_json::from_value(v)
        .map_err(|e| CliError::SlackApi { code: format!("decode: {e}"), request_id })?;
    Ok(parsed)
}

fn map_slack_error(code: &str, request_id: Option<String>) -> CliError {
    match code {
        "not_authed" | "invalid_auth" | "account_inactive" | "token_revoked" | "token_expired"
            => CliError::AuthFailed { reason: code.to_owned() },
        "channel_not_found" => CliError::NotFound { kind: "channel", name: "<remote>".into() },
        "user_not_found"    => CliError::NotFound { kind: "user", name: "<remote>".into() },
        "file_not_found"    => CliError::NotFound { kind: "file", name: "<remote>".into() },
        "ratelimited"       => CliError::RateLimited { retry_after_secs: 1 },
        "missing_scope" | "not_allowed_token_type"
            => CliError::TokenKindMismatch { needed: "?", actual: "?", api: "?" },
        _ => CliError::SlackApi { code: code.to_owned(), request_id },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test(flavor = "current_thread")]
    async fn auth_test_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .and(header("authorization", "Bearer xoxp-test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"team":"acme","user":"alice","team_id":"T1","user_id":"U1"}"#,
            ))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let r = c.auth_test().await.unwrap();
        assert_eq!(r.team_id.as_deref(), Some("T1"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auth_test_invalid_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":false,"error":"invalid_auth"}"#,
            ))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let err = c.auth_test().await.unwrap_err();
        assert!(matches!(err, CliError::AuthFailed { .. }));
    }
}
```

- [ ] **Step 3: Test + clippy + commit**

```bash
cargo test --lib slack
cargo clippy --all-targets -- -D warnings
git add Cargo.toml src/slack.rs
git commit -m "feat: SlackClient skeleton with auth.test and error mapping"
```

### Task 3.2: Retry-After clamp + `ratelimited` single retry

**Files:** Modify `src/slack.rs`.

- [ ] **Step 1: Add retry helper**

Replace `auth_test`'s body and add a helper:

```rust
    async fn post_with_retry(&self, url: &str) -> Result<reqwest::Response, CliError> {
        let do_send = || self.http.post(url).send();
        let resp = do_send().await.map_err(|e| CliError::Network(e.to_string()))?;
        if resp.status().as_u16() == 429 {
            let retry = resp.headers().get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1);
            let wait = retry.min(60);
            tokio::time::sleep(Duration::from_secs(wait)).await;
            return do_send().await.map_err(|e| CliError::Network(e.to_string()));
        }
        Ok(resp)
    }
```

Update `auth_test`:

```rust
        let url = format!("{}/api/auth.test", self.base_url);
        let resp = self.post_with_retry(&url).await?;
```

- [ ] **Step 2: Add test**

```rust
    #[tokio::test(flavor = "current_thread")]
    async fn retry_once_after_429() {
        use wiremock::matchers::method;
        let server = MockServer::start().await;
        let body_ok = r#"{"ok":true,"team":"acme","team_id":"T1","user_id":"U1"}"#;

        let ok_resp = ResponseTemplate::new(200).set_body_string(body_ok);
        let limited = ResponseTemplate::new(429)
            .insert_header("retry-after", "0");
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .respond_with(limited)
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .respond_with(ok_resp)
            .mount(&server)
            .await;

        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let r = c.auth_test().await.unwrap();
        assert_eq!(r.team_id.as_deref(), Some("T1"));
    }
```

- [ ] **Step 3: Test + commit**

```bash
cargo test --lib slack
git add src/slack.rs
git commit -m "feat: retry once on HTTP 429 with Retry-After clamped to 60s"
```

### Task 3.3: Read-side API methods (`conversations.list`, `conversations.history`, `users.list`, `users.info`, `conversations.info`, `search.messages`, `search.files`, `files.list`, `files.info`)

For each API method, follow the same pattern:

1. Define a DTO struct (`#[derive(Debug, Clone, Deserialize, Serialize, Tabled)]` where helpful).
2. Add a method on `SlackClient` that uses `post_with_retry` or `get_with_retry`.
3. Write a wiremock-based test with a fixture in `tests/fixtures/<method>.ok.json`.
4. Commit with `feat: add slack client method <name>`.

**Files:** Modify `src/slack.rs`, add fixtures under `tests/fixtures/`.

- [ ] **Step 1: Add `get_with_retry` helper** (mirror of `post_with_retry` but with `self.http.get(url)`).

- [ ] **Step 2: Implement `conversations.list`**

DTO:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelDto {
    pub id: String,
    pub name: String,
    pub is_private: bool,
    pub is_archived: bool,
    pub num_members: Option<u32>,
    pub topic: Option<TopicPurpose>,
    pub purpose: Option<TopicPurpose>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TopicPurpose {
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationsList {
    pub ok: bool,
    pub channels: Vec<ChannelDto>,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMetadata {
    pub next_cursor: Option<String>,
}
```

Method:

```rust
    pub async fn conversations_list(
        &self,
        types: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ConversationsList, CliError> {
        let mut url = format!(
            "{}/api/conversations.list?types={}&limit={}",
            self.base_url, types, limit
        );
        if let Some(c) = cursor {
            url.push_str("&cursor=");
            url.push_str(c);
        }
        let resp = self.get_with_retry(&url).await?;
        parse_slack::<ConversationsList>(resp).await
    }
```

Add test with fixture file `tests/fixtures/conversations.list.ok.json`:

```json
{
  "ok": true,
  "channels": [
    {"id":"C001","name":"general","is_private":false,"is_archived":false,"num_members":5,
     "topic":{"value":"team"},"purpose":{"value":"discuss"}}
  ],
  "response_metadata": {"next_cursor":""}
}
```

Test:

```rust
    #[tokio::test(flavor = "current_thread")]
    async fn conversations_list_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.list"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_string(include_str!("../tests/fixtures/conversations.list.ok.json")))
            .mount(&server).await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.conversations_list("public_channel,private_channel", 50, None).await.unwrap();
        assert_eq!(res.channels.len(), 1);
        assert_eq!(res.channels[0].name, "general");
    }
```

Commit: `feat: add conversations.list with cursor`.

- [ ] **Step 3: Implement `conversations.info`** with fixture `conversations.info.ok.json`.

```rust
    pub async fn conversations_info(&self, channel: &str) -> Result<ConversationsInfo, CliError> {
        let url = format!("{}/api/conversations.info?channel={}", self.base_url, channel);
        let resp = self.get_with_retry(&url).await?;
        parse_slack::<ConversationsInfo>(resp).await
    }

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationsInfo {
    pub ok: bool,
    pub channel: ChannelDto,
}
```

Commit: `feat: add conversations.info`.

- [ ] **Step 4: Implement `conversations.history`** with `messages` array + pagination.

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageDto {
    pub ts: String,
    pub user: Option<String>,
    pub text: String,
    pub thread_ts: Option<String>,
    pub reply_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct History {
    pub ok: bool,
    pub messages: Vec<MessageDto>,
    pub response_metadata: Option<ResponseMetadata>,
}

    pub async fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        cursor: Option<&str>,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> Result<History, CliError> {
        let mut url = format!(
            "{}/api/conversations.history?channel={}&limit={}",
            self.base_url, channel, limit,
        );
        if let Some(c) = cursor { url.push_str(&format!("&cursor={c}")); }
        if let Some(t) = oldest { url.push_str(&format!("&oldest={t}")); }
        if let Some(t) = latest { url.push_str(&format!("&latest={t}")); }
        let resp = self.get_with_retry(&url).await?;
        parse_slack::<History>(resp).await
    }
```

Commit: `feat: add conversations.history with oldest/latest filters`.

- [ ] **Step 5: Implement `conversations.replies`** (for thread view): same pattern, query params `channel`, `ts`, `cursor`, `limit`.

Commit: `feat: add conversations.replies for thread view`.

- [ ] **Step 6: Implement `users.list`** + `users.info`.

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserDto {
    pub id: String,
    pub name: String,
    pub real_name: Option<String>,
    pub deleted: bool,
    pub is_bot: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsersList {
    pub ok: bool,
    pub members: Vec<UserDto>,
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsersInfo {
    pub ok: bool,
    pub user: UserDto,
}
```

Methods + tests + fixtures + commits: `feat: add users.list`, `feat: add users.info`.

- [ ] **Step 7: Implement `search.messages`** + `search.files` (user-token only; the API rejects bot tokens — surface as `TokenKindMismatch`).

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SearchMessagesResp {
    pub ok: bool,
    pub messages: SearchMessagesPayload,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMessagesPayload {
    pub matches: Vec<MessageDto>,
    pub paging: SearchPaging,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchPaging {
    pub page: u32,
    pub pages: u32,
}
```

Method `search_messages(&self, query: &str, page: u32, count: u32)`.

Commit: `feat: add search.messages and search.files`.

- [ ] **Step 8: Implement `files.list`** + `files.info`. Fixtures + tests + commit.

### Task 3.4: Write-side API methods (`chat.postMessage`, `chat.update`, `chat.delete`, `reactions.add`, `chat.postEphemeral` not needed for MVP, `conversations.replies` already in 3.3)

**Files:** Modify `src/slack.rs`.

- [ ] **Step 1: Implement `chat.postMessage` with idempotency key**

DTO + payload:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PostMessage {
    pub channel: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_broadcast: Option<bool>,
    pub client_msg_id: String,
}

impl PostMessage {
    pub fn new(channel: String, text: String) -> Self {
        Self {
            channel, text,
            thread_ts: None,
            reply_broadcast: None,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PostMessageResp {
    pub ok: bool,
    pub ts: String,
    pub channel: String,
    pub message: MessageDto,
}
```

Method:

```rust
    pub async fn chat_post_message(&self, req: &PostMessage) -> Result<PostMessageResp, CliError> {
        let url = format!("{}/api/chat.postMessage", self.base_url);
        let resp = self.http.post(&url).json(req).send().await
            .map_err(|e| CliError::Network(e.to_string()))?;
        parse_slack::<PostMessageResp>(resp).await
    }
```

Test with fixture + commit `feat: add chat.postMessage with idempotency key`.

- [ ] **Step 2: `chat.update`** — payload `{channel, ts, text}`, response includes `ts`.

- [ ] **Step 3: `chat.delete`** — payload `{channel, ts}`.

- [ ] **Step 4: `reactions.add`** — payload `{channel, timestamp, name}`. Map `already_reacted` to a successful no-op (return `Ok(())`).

```rust
    pub async fn reactions_add(&self, channel: &str, ts: &str, name: &str) -> Result<(), CliError> {
        let url = format!("{}/api/reactions.add", self.base_url);
        let resp = self.http.post(&url)
            .json(&serde_json::json!({"channel": channel, "timestamp": ts, "name": name}))
            .send().await.map_err(|e| CliError::Network(e.to_string()))?;
        // Custom handling: treat already_reacted as success.
        let status = resp.status();
        let request_id = resp.headers().get("x-slack-req-id").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let body = resp.text().await.map_err(|e| CliError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(CliError::SlackApi { code: format!("http_{}", status.as_u16()), request_id });
        }
        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| CliError::SlackApi { code: format!("decode: {e}"), request_id })?;
        if v.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            return Ok(());
        }
        let code = v.get("error").and_then(|x| x.as_str()).unwrap_or("unknown");
        if code == "already_reacted" {
            return Ok(());
        }
        Err(map_slack_error(code, None))
    }
```

Test: 200/ok and 200/already_reacted both succeed. Commit `feat: add reactions.add (already_reacted is success)`.

- [ ] **Step 5: Admin: `conversations.create`, `conversations.archive`, `conversations.invite`, `conversations.leave`**

Each takes simple payloads. Commit one by one.

- [ ] **Step 6: `files.upload`** (multipart)

```rust
    pub async fn files_upload(
        &self,
        channel: Option<&str>,
        filename: &str,
        bytes: Vec<u8>,
        title: Option<&str>,
        initial_comment: Option<&str>,
        thread_ts: Option<&str>,
    ) -> Result<FileDto, CliError> {
        let url = format!("{}/api/files.upload", self.base_url);
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename.to_owned());
        let mut form = reqwest::multipart::Form::new().part("file", part);
        if let Some(c) = channel { form = form.text("channels", c.to_owned()); }
        if let Some(t) = title { form = form.text("title", t.to_owned()); }
        if let Some(c) = initial_comment { form = form.text("initial_comment", c.to_owned()); }
        if let Some(t) = thread_ts { form = form.text("thread_ts", t.to_owned()); }
        let resp = self.http.post(&url).multipart(form).send().await
            .map_err(|e| CliError::Network(e.to_string()))?;
        let parsed: FilesUploadResp = parse_slack(resp).await?;
        Ok(parsed.file)
    }

#[derive(Debug, Clone, Deserialize)]
pub struct FilesUploadResp { pub ok: bool, pub file: FileDto }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileDto {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mimetype: String,
    pub user: Option<String>,
    pub url_private: Option<String>,
}
```

Commit `feat: add files.upload (multipart)`.

- [ ] **Step 7: `files.delete`** + download helper using `url_private` + `Authorization` header (Slack file downloads require bearer auth on `files.slack.com`). Commit each.

---

## Phase 4 — Resolver

### Task 4.1: Resolver with name→ID + 60s cache

**Files:** Modify `src/resolve.rs`.

- [ ] **Step 1: Write `src/resolve.rs`**

```rust
//! Name → ID resolution with a 60s in-process cache.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::CliError;
use crate::id::{ChannelId, ChannelRef, UserId, UserRef};
use crate::slack::SlackClient;

const TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Resolver {
    channels: HashMap<String, (ChannelId, Instant)>,
    users: HashMap<String, (UserId, Instant)>,
}

impl Resolver {
    #[must_use]
    pub fn new() -> Self { Self::default() }

    pub async fn channel(&mut self, client: &SlackClient, r: &ChannelRef) -> Result<ChannelId, CliError> {
        match r {
            ChannelRef::Id(id) => Ok(id.clone()),
            ChannelRef::Name(name) => {
                if let Some((id, ts)) = self.channels.get(name) {
                    if ts.elapsed() < TTL {
                        return Ok(id.clone());
                    }
                }
                let id = lookup_channel(client, name).await?;
                self.channels.insert(name.clone(), (id.clone(), Instant::now()));
                Ok(id)
            }
        }
    }

    pub async fn user(&mut self, client: &SlackClient, r: &UserRef) -> Result<UserId, CliError> {
        match r {
            UserRef::Id(id) => Ok(id.clone()),
            UserRef::Name(name) => {
                if let Some((id, ts)) = self.users.get(name) {
                    if ts.elapsed() < TTL {
                        return Ok(id.clone());
                    }
                }
                let id = lookup_user(client, name).await?;
                self.users.insert(name.clone(), (id.clone(), Instant::now()));
                Ok(id)
            }
        }
    }
}

async fn lookup_channel(client: &SlackClient, name: &str) -> Result<ChannelId, CliError> {
    let mut cursor: Option<String> = None;
    let mut matches: Vec<(String, bool)> = Vec::new(); // (id, is_private)
    loop {
        let page = client.conversations_list(
            "public_channel,private_channel",
            200,
            cursor.as_deref(),
        ).await?;
        for ch in &page.channels {
            if ch.name == name {
                matches.push((ch.id.clone(), ch.is_private));
            }
        }
        cursor = page.response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|s| !s.is_empty());
        if cursor.is_none() { break; }
    }
    match matches.len() {
        0 => Err(CliError::NotFound { kind: "channel", name: name.into() }),
        1 => Ok(ChannelId(matches.remove(0).0)),
        _ => Err(CliError::Ambiguous {
            kind: "channel",
            name: name.into(),
            candidates: matches.into_iter().map(|(id, p)| {
                format!("{id} ({})", if p { "private" } else { "public" })
            }).collect(),
        }),
    }
}

async fn lookup_user(client: &SlackClient, name: &str) -> Result<UserId, CliError> {
    let mut cursor: Option<String> = None;
    let mut matches: Vec<String> = Vec::new();
    loop {
        let page = client.users_list(200, cursor.as_deref()).await?;
        for u in &page.members {
            if u.name == name && !u.deleted {
                matches.push(u.id.clone());
            }
        }
        cursor = page.response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|s| !s.is_empty());
        if cursor.is_none() { break; }
    }
    match matches.len() {
        0 => Err(CliError::NotFound { kind: "user", name: name.into() }),
        1 => Ok(UserId(matches.remove(0))),
        _ => Err(CliError::Ambiguous {
            kind: "user", name: name.into(), candidates: matches,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ChannelRef;
    use secrecy::SecretString;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test(flavor = "current_thread")]
    async fn channel_id_passthrough_no_http_call() {
        // No mocks mounted; if HTTP fires, the test fails.
        let server = MockServer::start().await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r.channel(&c, &ChannelRef::Id(ChannelId("C0".into()))).await.unwrap();
        assert_eq!(id.as_str(), "C0");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn channel_name_resolves_via_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/conversations.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"channels":[
                    {"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}
                ],"response_metadata":{"next_cursor":""}}"#))
            .mount(&server).await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r.channel(&c, &ChannelRef::Name("general".into())).await.unwrap();
        assert_eq!(id.as_str(), "C1");
    }
}
```

- [ ] **Step 2: Test + clippy + commit**

```bash
cargo test --lib resolve
cargo clippy --all-targets -- -D warnings
git add src/resolve.rs
git commit -m "feat: Resolver with name→ID, ambiguity detection, 60s cache"
```

---

## Phase 5 — CLI + dispatch

### Task 5.1: `cli.rs` with `Cli`, `GlobalArgs`, Subcommand enums

**Files:** Modify `src/cli.rs`.

- [ ] **Step 1: Write `src/cli.rs`**

```rust
//! CLI argument definitions (clap derive).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::output::Format;

#[derive(Debug, Parser)]
#[command(name = "slack-cli", version, about = "Slack CLI for coding agents")]
pub struct Cli {
    #[command(flatten)]
    pub globals: GlobalArgs,

    #[command(subcommand)]
    pub command: TopCommand,
}

#[derive(Debug, Args)]
pub struct GlobalArgs {
    /// Config file path. Overrides `SLACK_CLI_CONFIG`.
    #[arg(short = 'c', long, global = true, env = "SLACK_CLI_CONFIG")]
    pub config: Option<PathBuf>,

    /// Workspace name from config to use.
    #[arg(short = 'w', long, global = true, env = "SLACK_CLI_WORKSPACE")]
    pub workspace: Option<String>,

    /// Output format. Defaults to table on TTY, json on pipe.
    #[arg(long, global = true, value_enum, group = "output_fmt")]
    pub json: bool,

    #[arg(long, global = true, group = "output_fmt")]
    pub plain: bool,

    #[arg(long, global = true, group = "output_fmt")]
    pub table: bool,

    /// Increase log verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress info logging (errors still printed).
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Disable ANSI color output (also via `NO_COLOR`).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Confirm destructive operations non-interactively.
    #[arg(long, global = true, visible_alias = "no-confirm")]
    pub yes: bool,

    /// Max items per page (default 50, max 200).
    #[arg(long, global = true, default_value_t = 50)]
    pub max_results: u32,

    /// Fetch every page (paired with --max-pages).
    #[arg(long, global = true)]
    pub all: bool,

    /// Maximum pages to fetch when --all is set.
    #[arg(long, global = true, default_value_t = 10)]
    pub max_pages: u32,

    /// For write/admin commands: print the would-be request without sending.
    #[arg(long, global = true)]
    pub dry_run: bool,
}

impl GlobalArgs {
    #[must_use]
    pub fn format(&self) -> Option<Format> {
        if self.json { Some(Format::Json) }
        else if self.plain { Some(Format::Plain) }
        else if self.table { Some(Format::Table) }
        else { None }
    }
}

#[derive(Debug, Subcommand)]
pub enum TopCommand {
    Auth(AuthArgs),
    Channel(ChannelArgs),
    Message(MessageArgs),
    Search(SearchArgs),
    User(UserArgs),
    File(FileArgs),
    Completion(CompletionArgs),
}

// ... Args and Subcommand enums for each group (auth, channel, message, search, user, file, completion).
// Each follows the same pattern: a struct with #[command(subcommand)] holding an enum of verbs,
// and each verb is its own Args struct.

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub verb: AuthVerb,
}

#[derive(Debug, Subcommand)]
pub enum AuthVerb {
    /// Interactively configure a workspace.
    Init {
        /// Workspace name to create or update.
        #[arg(short = 'w', long)]
        workspace: Option<String>,
        /// Read the token from stdin instead of prompting.
        #[arg(long)]
        token_stdin: bool,
        /// Token backend.
        #[arg(long, value_enum, default_value_t = AuthBackend::Keyring)]
        backend: AuthBackend,
    },
    /// Print the current identity / workspace.
    Whoami,
    /// List configured workspaces.
    List,
    /// Remove the stored token for a workspace.
    Logout {
        #[arg(short = 'w', long)]
        workspace: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AuthBackend { Keyring, File, Env }

#[derive(Debug, Args)]
pub struct ChannelArgs { #[command(subcommand)] pub verb: ChannelVerb }

#[derive(Debug, Subcommand)]
pub enum ChannelVerb {
    List {
        #[arg(short = 'q', long)] query: Option<String>,
        #[arg(long, default_value = "public_channel,private_channel")] types: String,
    },
    View { channel: String },
    Create { name: String, #[arg(long)] private: bool, #[arg(long)] description: Option<String> },
    Archive { channel: String },
    Invite { channel: String, users: Vec<String> },
    Leave { channel: String },
}

#[derive(Debug, Args)]
pub struct MessageArgs { #[command(subcommand)] pub verb: MessageVerb }

#[derive(Debug, Subcommand)]
pub enum MessageVerb {
    List {
        channel: String,
        #[arg(long, default_value_t = 50)] limit: u32,
        #[arg(long)] oldest: Option<String>,
        #[arg(long)] latest: Option<String>,
    },
    View {
        channel: String,
        ts: String,
        #[arg(long)] thread: bool,
    },
    Send {
        channel: String,
        #[arg(long, conflicts_with_all = ["file"])] text: Option<String>,
        #[arg(long, conflicts_with_all = ["text"])] file: Option<PathBuf>,
        #[arg(long)] thread: Option<String>,
        #[arg(long)] broadcast: bool,
    },
    Reply { channel: String, ts: String, #[arg(long)] text: String, #[arg(long)] broadcast: bool },
    Update { channel: String, ts: String, #[arg(long)] text: String },
    Delete { channel: String, ts: String },
    React { channel: String, ts: String, emoji: String, #[arg(long)] remove: bool },
}

#[derive(Debug, Args)]
pub struct SearchArgs { #[command(subcommand)] pub verb: SearchVerb }

#[derive(Debug, Subcommand)]
pub enum SearchVerb {
    Messages {
        query: String,
        #[arg(long)] r#in: Option<String>,
        #[arg(long)] from: Option<String>,
        #[arg(long, default_value_t = 50)] limit: u32,
    },
    Files {
        query: String,
        #[arg(long, name = "type")] file_type: Option<String>,
        #[arg(long, default_value_t = 50)] limit: u32,
    },
}

#[derive(Debug, Args)]
pub struct UserArgs { #[command(subcommand)] pub verb: UserVerb }

#[derive(Debug, Subcommand)]
pub enum UserVerb {
    List {
        #[arg(short = 'q', long)] query: Option<String>,
        #[arg(long, default_value_t = 50)] limit: u32,
    },
    View { user: String },
}

#[derive(Debug, Args)]
pub struct FileArgs { #[command(subcommand)] pub verb: FileVerb }

#[derive(Debug, Subcommand)]
pub enum FileVerb {
    List {
        #[arg(long)] channel: Option<String>,
        #[arg(long)] user: Option<String>,
        #[arg(long, default_value_t = 50)] limit: u32,
    },
    Upload {
        path: PathBuf,
        #[arg(long)] channel: Option<String>,
        #[arg(long)] title: Option<String>,
        #[arg(long)] comment: Option<String>,
        #[arg(long)] thread: Option<String>,
        #[arg(long)] stdin: bool,
        #[arg(long)] filename: Option<String>,
    },
    Download {
        file_id: String,
        #[arg(short = 'o', long)] output: Option<PathBuf>,
        #[arg(long)] stdout: bool,
    },
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    pub shell: clap_complete::Shell,
}
```

- [ ] **Step 2: Add `clap_complete` dep**

```toml
clap_complete = "4"
```

- [ ] **Step 3: Smoke test**

Run: `cargo run -- --help`
Expected: shows the top-level help with all subcommand groups.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/cli.rs
git commit -m "feat: define CLI argument grammar (clap derive)"
```

### Task 5.2: `cmd::mod` with `Ctx` and `dispatch`

**Files:** Modify `src/cmd/mod.rs`.

- [ ] **Step 1: Create per-subcommand stub files**

```bash
for f in auth channel message search user file completion; do
  cat > "src/cmd/$f.rs" <<'EOF'
//! Subcommand handlers; filled in later tasks.
use crate::cmd::Ctx;
use crate::error::CliError;
EOF
done
```

- [ ] **Step 2: Write `src/cmd/mod.rs`**

```rust
//! Command dispatch: build a Ctx from CLI args + config, then route.

pub mod auth;
pub mod channel;
pub mod completion;
pub mod file;
pub mod message;
pub mod search;
pub mod user;

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::cli::{Cli, GlobalArgs, TopCommand};
use crate::config::{self, Config, WorkspaceConfig};
use crate::error::CliError;
use crate::output::Format;
use crate::resolve::Resolver;
use crate::slack::SlackClient;

#[derive(Debug)]
pub struct Ctx {
    pub config: Config,
    pub config_path: PathBuf,
    pub workspace_name: String,
    pub client: Option<SlackClient>,
    pub resolver: Resolver,
    pub format: Format,
    pub stdout_is_tty: bool,
    pub globals: GlobalArgs,
}

pub async fn dispatch(cli: Cli) -> Result<(), CliError> {
    let path = config::resolve_path(cli.globals.config.as_deref())?;
    let cfg = config::load(&path)?;

    // `auth` does not require an existing workspace.
    let need_client = !matches!(cli.command, TopCommand::Auth(_) | TopCommand::Completion(_));

    let (workspace_name, client) = if need_client {
        let (name, ws) = config::pick_workspace(&cfg, cli.globals.workspace.as_deref())?;
        let secret = crate::secret::load(name, ws)?;
        let client = SlackClient::new(secret)?;
        (name.to_owned(), Some(client))
    } else {
        (cli.globals.workspace.clone().unwrap_or_default(), None)
    };

    let stdout_is_tty = std::io::stdout().is_terminal();
    let fmt = Format::resolve(cli.globals.format(), stdout_is_tty);

    let mut ctx = Ctx {
        config: cfg,
        config_path: path,
        workspace_name,
        client,
        resolver: Resolver::new(),
        format: fmt,
        stdout_is_tty,
        globals: cli.globals,
    };

    match cli.command {
        TopCommand::Auth(args) => auth::run(args, &mut ctx).await,
        TopCommand::Channel(args) => channel::run(args, &mut ctx).await,
        TopCommand::Message(args) => message::run(args, &mut ctx).await,
        TopCommand::Search(args) => search::run(args, &mut ctx).await,
        TopCommand::User(args) => user::run(args, &mut ctx).await,
        TopCommand::File(args) => file::run(args, &mut ctx).await,
        TopCommand::Completion(args) => completion::run(args, &mut ctx).await,
    }
}
```

- [ ] **Step 3: Add `run` stubs**

In each `src/cmd/*.rs` (except `mod.rs`), add a stub:

```rust
pub async fn run(_args: crate::cli::AuthArgs, _ctx: &mut Ctx) -> Result<(), CliError> {
    Err(CliError::InvalidArg("not implemented".into()))
}
```

Adjust the `Args` type per file (`AuthArgs`, `ChannelArgs`, etc.).

- [ ] **Step 4: Update `main.rs` to wire dispatch**

```rust
use std::process::ExitCode;

use clap::Parser;
use slack_cli::cli::Cli;
use slack_cli::cmd::dispatch;
use slack_cli::logging;
use slack_cli::output::{render_error, Format};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.globals.verbose, cli.globals.quiet);

    match dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let json = matches!(Format::detect_default(), Format::Json);
            let _ = render_error(&e, json, &mut std::io::stderr().lock());
            ExitCode::from(e.exit_code())
        }
    }
}
```

- [ ] **Step 5: Build + clippy + commit**

```bash
cargo build
cargo clippy --all-targets -- -D warnings
git add src/cmd/ src/main.rs
git commit -m "feat: wire dispatch with Ctx, format resolution, and error rendering"
```

### Task 5.3: `logging.rs`

**Files:** Modify `src/logging.rs`.

- [ ] **Step 1: Write `src/logging.rs`**

```rust
//! Tracing initialization. Always writes to stderr.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init(verbose: u8, quiet: bool) {
    let directive = if quiet {
        "error".to_owned()
    } else {
        match verbose {
            0 => "slack_cli=info,reqwest=warn,hyper=warn".into(),
            1 => "slack_cli=info,reqwest=warn,hyper=warn".into(),
            2 => "slack_cli=debug,reqwest=warn,hyper=warn".into(),
            _ => "slack_cli=trace,reqwest=warn,hyper=warn".into(),
        }
    };
    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directive));
    let _ = fmt()
        .with_env_filter(env)
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_level(true)
        .try_init();
}
```

- [ ] **Step 2: Add `tracing-subscriber`**

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 3: Build + commit**

```bash
cargo build
git add Cargo.toml src/logging.rs
git commit -m "feat: tracing init with stderr writer and rate-limited http logs"
```

---

## Phase 6 — `cmd::auth`

### Task 6.1: `auth whoami` (read-only, no destructive ops)

**Files:** Modify `src/cmd/auth.rs`.

- [ ] **Step 1: Implement `whoami`**

```rust
use serde::Serialize;
use tabled::Tabled;

use crate::cli::{AuthArgs, AuthVerb};
use crate::cmd::Ctx;
use crate::error::CliError;
use crate::output::{render_one, Format};

#[derive(Debug, Serialize, Tabled)]
pub struct WhoamiRow {
    pub user_id: String,
    pub user_name: String,
    pub team_id: String,
    pub team_domain: String,
    pub token_kind: String,
}

pub async fn run(args: AuthArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    match args.verb {
        AuthVerb::Whoami => whoami(ctx).await,
        AuthVerb::List => list(ctx).await,
        AuthVerb::Init { .. } | AuthVerb::Logout { .. } => {
            Err(CliError::InvalidArg("auth init/logout not yet implemented".into()))
        }
    }
}

async fn whoami(ctx: &mut Ctx) -> Result<(), CliError> {
    let client = ctx.client.as_ref()
        .ok_or_else(|| CliError::NoToken { workspace: ctx.workspace_name.clone() })?;
    let info = client.auth_test().await?;
    let kind = if info.bot_id.is_some() { "bot" } else { "user" };
    let row = WhoamiRow {
        user_id: info.user_id.unwrap_or_default(),
        user_name: info.user.unwrap_or_default(),
        team_id: info.team_id.unwrap_or_default(),
        team_domain: info.team.unwrap_or_default(),
        token_kind: kind.into(),
    };
    let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
    render_one(&row, ctx.format, pretty, None, &mut std::io::stdout().lock())
        .map_err(CliError::Io)
}

async fn list(ctx: &mut Ctx) -> Result<(), CliError> {
    #[derive(Serialize, Tabled)]
    struct Row<'a> {
        name: &'a str,
        team_id: &'a str,
        token_source: &'a str,
    }
    let rows: Vec<Row<'_>> = ctx.config.workspaces.iter().map(|(n, w)| Row {
        name: n,
        team_id: w.team_id.as_deref().unwrap_or(""),
        token_source: match w.token_source {
            crate::config::TokenSource::Keyring => "keyring",
            crate::config::TokenSource::File => "file",
            crate::config::TokenSource::Env => "env",
        },
    }).collect();
    crate::output::render_list(&rows, "", ctx.format, ctx.stdout_is_tty, None, &mut std::io::stdout().lock())
        .map_err(CliError::Io)
}
```

- [ ] **Step 2: Smoke test (build only — E2E test in Phase 14)**

Run: `cargo check`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/cmd/auth.rs
git commit -m "feat: implement auth whoami and auth list"
```

### Task 6.2: `auth init` (interactive)

- [ ] **Step 1: Add `rpassword` + implement**

In `Cargo.toml`: `rpassword = "7"`.

Implementation summary:

1. If `workspace` is `None`, prompt for name.
2. Read token: `--token-stdin` or `rpassword::prompt_password`.
3. Build a temporary `SlackClient` and call `auth.test`.
4. Build `WorkspaceConfig` with `token_source = match backend`.
5. `secret::save(name, &mut ws, &token)` (no-op for env).
6. `config::save(&path, &cfg)`.
7. Print success row.

Code:

```rust
async fn init(
    workspace: Option<String>,
    token_stdin: bool,
    backend: crate::cli::AuthBackend,
    ctx: &mut Ctx,
) -> Result<(), CliError> {
    use std::io::BufRead;

    let name = workspace.or_else(|| prompt("workspace name: ").ok())
        .ok_or_else(|| CliError::InvalidArg("workspace name required".into()))?;
    let raw = if token_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).map_err(CliError::Io)?;
        line.trim().to_owned()
    } else {
        rpassword::prompt_password("token (input hidden): ")
            .map_err(|e| CliError::InvalidArg(e.to_string()))?
    };
    crate::secret::validate_prefix(&raw)?;
    let secret = secrecy::SecretString::from(raw.clone());

    let client = crate::slack::SlackClient::new(secret.clone())?;
    let info = client.auth_test().await?;
    let kind = if info.bot_id.is_some() { "bot" } else { "user" };

    let token_source = match backend {
        crate::cli::AuthBackend::Keyring => crate::config::TokenSource::Keyring,
        crate::cli::AuthBackend::File => crate::config::TokenSource::File,
        crate::cli::AuthBackend::Env => crate::config::TokenSource::Env,
    };

    let mut ws = crate::config::WorkspaceConfig {
        team_id: info.team_id.clone(),
        team_domain: info.team.clone(),
        token_source: token_source.clone(),
        token: None,
        token_env: None,
        meta: crate::config::WorkspaceMeta {
            token_kind: Some(kind.into()),
            user_id: info.user_id.clone(),
            updated_at: Some(chrono_like_now()),
        },
    };
    crate::secret::save(&name, &mut ws, &secret)?;

    ctx.config.workspaces.insert(name.clone(), ws);
    if ctx.config.default_workspace.is_none() {
        ctx.config.default_workspace = Some(name.clone());
    }
    crate::config::save(&ctx.config_path, &ctx.config)?;

    eprintln!("✓ saved workspace '{name}'");
    Ok(())
}

fn prompt(msg: &str) -> std::io::Result<String> {
    use std::io::{BufRead, Write};
    let mut out = std::io::stderr().lock();
    out.write_all(msg.as_bytes())?;
    out.flush()?;
    let mut buf = String::new();
    std::io::stdin().lock().read_line(&mut buf)?;
    Ok(buf.trim().to_owned())
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // Cheap RFC3339-ish; we don't pull chrono just for this.
    format!("@{secs}")
}
```

Wire `AuthVerb::Init { workspace, token_stdin, backend }` to call `init(...)`.

- [ ] **Step 2: Commit**

```bash
cargo check
git add Cargo.toml src/cmd/auth.rs
git commit -m "feat: implement auth init (interactive)"
```

### Task 6.3: `auth logout`

- [ ] **Step 1: Implement**

```rust
async fn logout(workspace: Option<String>, ctx: &mut Ctx) -> Result<(), CliError> {
    let name = workspace
        .or_else(|| Some(ctx.workspace_name.clone()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CliError::AmbiguousWorkspace {
            available: ctx.config.workspaces.keys().cloned().collect(),
        })?;
    if !ctx.globals.yes && std::io::stdin().is_terminal() {
        // TTY: prompt
        let line = prompt(&format!("delete token for '{name}'? type 'yes': "))?;
        if line != "yes" {
            return Err(CliError::ConfirmationRequired);
        }
    } else if !ctx.globals.yes {
        return Err(CliError::ConfirmationRequired);
    }
    if let Some(ws) = ctx.config.workspaces.get_mut(&name) {
        crate::secret::delete(&name, ws)?;
        ws.token = None;
    }
    crate::config::save(&ctx.config_path, &ctx.config)?;
    eprintln!("✓ logged out '{name}'");
    Ok(())
}
```

- [ ] **Step 2: Wire + commit**

```bash
git add src/cmd/auth.rs
git commit -m "feat: implement auth logout with confirmation gate"
```

---

## Phase 7 — `cmd::channel`

### Task 7.1: `channel list` + `channel view`

**Files:** Modify `src/cmd/channel.rs`.

- [ ] **Step 1: Implement `list`**

```rust
use serde::Serialize;
use tabled::Tabled;

use crate::cli::{ChannelArgs, ChannelVerb};
use crate::cmd::Ctx;
use crate::error::CliError;
use crate::output::{render_list, render_one, Format};

#[derive(Debug, Serialize, Tabled)]
pub struct ChannelRow {
    pub id: String,
    pub name: String,
    pub is_private: bool,
    pub members: u32,
}

pub async fn run(args: ChannelArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = ctx.client.as_ref().ok_or_else(|| CliError::NoToken {
        workspace: ctx.workspace_name.clone(),
    })?;
    match args.verb {
        ChannelVerb::List { query, types } => {
            let limit = ctx.globals.max_results.min(200);
            let max_pages = if ctx.globals.all { ctx.globals.max_pages } else { 1 };
            let mut cursor: Option<String> = None;
            let mut rows: Vec<ChannelRow> = Vec::new();
            let mut next_cursor = String::new();
            for _ in 0..max_pages {
                let page = client.conversations_list(&types, limit, cursor.as_deref()).await?;
                for ch in page.channels {
                    if let Some(q) = query.as_deref() {
                        if !ch.name.contains(q) { continue; }
                    }
                    rows.push(ChannelRow {
                        id: ch.id, name: ch.name,
                        is_private: ch.is_private,
                        members: ch.num_members.unwrap_or(0),
                    });
                }
                next_cursor = page.response_metadata.and_then(|m| m.next_cursor).unwrap_or_default();
                if next_cursor.is_empty() { break; }
                cursor = Some(next_cursor.clone());
            }
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_list(&rows, &next_cursor, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        ChannelVerb::View { channel } => {
            let cref = crate::id::ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            let info = client.conversations_info(id.as_str()).await?;
            let row = ChannelRow {
                id: info.channel.id,
                name: info.channel.name,
                is_private: info.channel.is_private,
                members: info.channel.num_members.unwrap_or(0),
            };
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_one(&row, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        _ => Err(CliError::InvalidArg("not yet implemented".into())),
    }
}
```

- [ ] **Step 2: Commit**

```bash
cargo check
git add src/cmd/channel.rs
git commit -m "feat: channel list + channel view"
```

### Task 7.2: `channel create / archive / invite / leave` with `--dry-run` + `--yes`

- [ ] **Step 1: Implement each verb**

Add helpers to `cmd/mod.rs`:

```rust
pub fn require_confirmation(ctx: &Ctx, what: &str) -> Result<(), CliError> {
    if ctx.globals.yes { return Ok(()); }
    if !ctx.stdout_is_tty {
        return Err(CliError::ConfirmationRequired);
    }
    use std::io::{BufRead, Write};
    let mut err = std::io::stderr().lock();
    write!(err, "{what} type 'yes' to continue: ").map_err(CliError::Io)?;
    err.flush().map_err(CliError::Io)?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).map_err(CliError::Io)?;
    if line.trim() == "yes" { Ok(()) } else { Err(CliError::ConfirmationRequired) }
}
```

In `cmd/channel.rs`:

```rust
        ChannelVerb::Create { name, private, description } => {
            if ctx.globals.dry_run {
                emit_dry_run(ctx, "conversations.create", "POST /api/conversations.create",
                    serde_json::json!({"name": name, "is_private": private, "description": description}))?;
                return Ok(());
            }
            let info = client.conversations_create(&name, private, description.as_deref()).await?;
            print_one_json_or_table(ctx, &info)?;
            Ok(())
        }
        ChannelVerb::Archive { channel } => {
            super::require_confirmation(ctx, &format!("archive channel '{channel}'?"))?;
            if ctx.globals.dry_run {
                return emit_dry_run(ctx, "conversations.archive", "POST /api/conversations.archive",
                    serde_json::json!({"channel": channel}));
            }
            let cref = crate::id::ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            client.conversations_archive(id.as_str()).await?;
            eprintln!("✓ archived");
            Ok(())
        }
        ChannelVerb::Invite { channel, users } => {
            if ctx.globals.dry_run {
                return emit_dry_run(ctx, "conversations.invite", "POST /api/conversations.invite",
                    serde_json::json!({"channel": channel, "users": users}));
            }
            let cref = crate::id::ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let mut uids: Vec<String> = Vec::new();
            for u in &users {
                let uref = crate::id::UserRef::try_from(u.as_str())?;
                uids.push(ctx.resolver.user(client, &uref).await?.to_string());
            }
            client.conversations_invite(cid.as_str(), &uids).await?;
            eprintln!("✓ invited");
            Ok(())
        }
        ChannelVerb::Leave { channel } => {
            super::require_confirmation(ctx, &format!("leave channel '{channel}'?"))?;
            if ctx.globals.dry_run {
                return emit_dry_run(ctx, "conversations.leave", "POST /api/conversations.leave",
                    serde_json::json!({"channel": channel}));
            }
            let cref = crate::id::ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            client.conversations_leave(id.as_str()).await?;
            eprintln!("✓ left");
            Ok(())
        }
```

Helpers `emit_dry_run` and `print_one_json_or_table` go in `cmd/mod.rs`:

```rust
pub fn emit_dry_run(ctx: &Ctx, would_call: &str, endpoint: &str, payload: serde_json::Value) -> Result<(), CliError> {
    use serde::Serialize;
    use std::io::Write;

    #[derive(Serialize)]
    struct DryRunData<'a> {
        dry_run: bool,
        would_call: &'a str,
        endpoint: &'a str,
        payload: serde_json::Value,
    }

    let data = DryRunData { dry_run: true, would_call, endpoint, payload };
    let env = crate::output::OkEnvelope { ok: true, schema: crate::output::SCHEMA, data, request_id: None };
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &env).map_err(|e| CliError::Io(std::io::Error::other(e)))?;
    writeln!(out).map_err(CliError::Io)?;
    Ok(())
}
```

Add `pub` visibility to `OkEnvelope` fields (already public via `pub`).

- [ ] **Step 2: Commit**

```bash
cargo check
git add src/cmd/mod.rs src/cmd/channel.rs
git commit -m "feat: channel create/archive/invite/leave with --dry-run and --yes"
```

---

## Phase 8 — `cmd::message`

### Task 8.1: `message list`, `message view`, `message view --thread`

**Files:** Modify `src/cmd/message.rs`.

- [ ] **Step 1: Implement**

```rust
use serde::Serialize;
use tabled::Tabled;

use crate::cli::{MessageArgs, MessageVerb};
use crate::cmd::Ctx;
use crate::error::CliError;
use crate::id::ChannelRef;
use crate::output::{render_list, render_one, Format};

#[derive(Debug, Serialize, Tabled)]
pub struct MessageRow {
    pub ts: String,
    pub user: String,
    pub text: String,
}

pub async fn run(args: MessageArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = ctx.client.as_ref().ok_or_else(|| CliError::NoToken {
        workspace: ctx.workspace_name.clone(),
    })?;
    match args.verb {
        MessageVerb::List { channel, limit, oldest, latest } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            let page = client.conversations_history(
                id.as_str(),
                limit.min(200),
                None,
                oldest.as_deref(),
                latest.as_deref(),
            ).await?;
            let rows: Vec<MessageRow> = page.messages.into_iter()
                .map(|m| MessageRow { ts: m.ts, user: m.user.unwrap_or_default(), text: m.text })
                .collect();
            let cursor = page.response_metadata.and_then(|m| m.next_cursor).unwrap_or_default();
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_list(&rows, &cursor, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        MessageVerb::View { channel, ts, thread } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            if thread {
                let page = client.conversations_replies(id.as_str(), &ts, None, 100).await?;
                let rows: Vec<MessageRow> = page.messages.into_iter()
                    .map(|m| MessageRow { ts: m.ts, user: m.user.unwrap_or_default(), text: m.text })
                    .collect();
                let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
                render_list(&rows, "", ctx.format, pretty, None, &mut std::io::stdout().lock())
                    .map_err(CliError::Io)
            } else {
                let page = client.conversations_history(id.as_str(), 1, None, Some(&ts), Some(&ts)).await?;
                let m = page.messages.into_iter().next().ok_or_else(|| CliError::NotFound {
                    kind: "message", name: ts.clone(),
                })?;
                let row = MessageRow { ts: m.ts, user: m.user.unwrap_or_default(), text: m.text };
                let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
                render_one(&row, ctx.format, pretty, None, &mut std::io::stdout().lock())
                    .map_err(CliError::Io)
            }
        }
        // send/reply/update/delete/react are filled in 8.2 and 8.3
        _ => Err(CliError::InvalidArg("not yet implemented".into())),
    }
}
```

- [ ] **Step 2: Commit**

```bash
cargo check
git add src/cmd/message.rs
git commit -m "feat: message list and view (with --thread)"
```

### Task 8.2: `message send` / `message reply` with stdin + dry-run + idempotency

- [ ] **Step 1: Implement**

```rust
        MessageVerb::Send { channel, text, file, thread, broadcast } => {
            let body = read_body(text.as_deref(), file.as_deref())?;
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let mut req = crate::slack::PostMessage::new(cid.as_str().into(), body);
            req.thread_ts = thread.clone();
            req.reply_broadcast = if broadcast { Some(true) } else { None };
            if ctx.globals.dry_run {
                return super::emit_dry_run(ctx, "chat.postMessage", "POST /api/chat.postMessage",
                    serde_json::to_value(&req).unwrap());
            }
            let resp = client.chat_post_message(&req).await?;
            let row = MessageRow { ts: resp.ts, user: resp.message.user.unwrap_or_default(), text: resp.message.text };
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_one(&row, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        MessageVerb::Reply { channel, ts, text, broadcast } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let mut req = crate::slack::PostMessage::new(cid.as_str().into(), text);
            req.thread_ts = Some(ts);
            req.reply_broadcast = if broadcast { Some(true) } else { None };
            if ctx.globals.dry_run {
                return super::emit_dry_run(ctx, "chat.postMessage", "POST /api/chat.postMessage",
                    serde_json::to_value(&req).unwrap());
            }
            let resp = client.chat_post_message(&req).await?;
            let row = MessageRow { ts: resp.ts, user: resp.message.user.unwrap_or_default(), text: resp.message.text };
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_one(&row, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
```

Helper:

```rust
fn read_body(text: Option<&str>, file: Option<&std::path::Path>) -> Result<String, CliError> {
    use std::io::Read;
    match (text, file) {
        (Some("-"), _) | (None, Some(p)) if file.map_or(true, |f| f.as_os_str() == "-") => {
            let mut s = String::new();
            std::io::stdin().lock().read_to_string(&mut s).map_err(CliError::Io)?;
            Ok(s.trim_end_matches('\n').to_owned())
        }
        (Some(s), None) => Ok(s.to_owned()),
        (None, Some(p)) => std::fs::read_to_string(p).map_err(CliError::Io),
        (None, None) => Err(CliError::InvalidArg("--text or --file required".into())),
        (Some(_), Some(_)) => Err(CliError::InvalidArg("--text and --file are mutually exclusive".into())),
    }
}
```

- [ ] **Step 2: Commit**

```bash
cargo check
git add src/cmd/message.rs
git commit -m "feat: message send/reply with stdin, --dry-run, idempotency key"
```

### Task 8.3: `message update / delete / react`

- [ ] **Step 1: Implement** (each follows the same dry-run / confirmation pattern as 7.2). `react` honors `--remove` by calling `reactions.remove` (add to slack.rs first).

```rust
        MessageVerb::Update { channel, ts, text } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            if ctx.globals.dry_run {
                return super::emit_dry_run(ctx, "chat.update", "POST /api/chat.update",
                    serde_json::json!({"channel": cid.as_str(), "ts": ts, "text": text}));
            }
            client.chat_update(cid.as_str(), &ts, &text).await?;
            eprintln!("✓ updated");
            Ok(())
        }
        MessageVerb::Delete { channel, ts } => {
            super::require_confirmation(ctx, &format!("delete message {ts} in '{channel}'?"))?;
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            if ctx.globals.dry_run {
                return super::emit_dry_run(ctx, "chat.delete", "POST /api/chat.delete",
                    serde_json::json!({"channel": cid.as_str(), "ts": ts}));
            }
            client.chat_delete(cid.as_str(), &ts).await?;
            eprintln!("✓ deleted");
            Ok(())
        }
        MessageVerb::React { channel, ts, emoji, remove } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let name = emoji.trim_matches(':');
            if ctx.globals.dry_run {
                let endpoint = if remove { "POST /api/reactions.remove" } else { "POST /api/reactions.add" };
                return super::emit_dry_run(ctx, if remove { "reactions.remove" } else { "reactions.add" },
                    endpoint, serde_json::json!({"channel": cid.as_str(), "timestamp": ts, "name": name}));
            }
            if remove {
                client.reactions_remove(cid.as_str(), &ts, name).await?;
            } else {
                client.reactions_add(cid.as_str(), &ts, name).await?;
            }
            eprintln!("✓ reaction {}", if remove { "removed" } else { "added" });
            Ok(())
        }
```

- [ ] **Step 2: Commit**

```bash
cargo check
git add src/cmd/message.rs src/slack.rs
git commit -m "feat: message update/delete/react with confirmation and dry-run"
```

---

## Phase 9 — `cmd::search`, `cmd::user`, `cmd::file`, `cmd::completion`

Each is similar in structure. One task per command file.

### Task 9.1: `cmd::search`

**Files:** Modify `src/cmd/search.rs`.

Implementation:

```rust
use serde::Serialize;
use tabled::Tabled;

use crate::cli::{SearchArgs, SearchVerb};
use crate::cmd::Ctx;
use crate::error::CliError;
use crate::output::{render_list, Format};

#[derive(Debug, Serialize, Tabled)]
pub struct SearchHit {
    pub ts: String,
    pub user: String,
    pub text: String,
}

pub async fn run(args: SearchArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = ctx.client.as_ref().ok_or_else(|| CliError::NoToken {
        workspace: ctx.workspace_name.clone(),
    })?;
    match args.verb {
        SearchVerb::Messages { query, r#in, from, limit } => {
            let q = compose_query(&query, r#in.as_deref(), from.as_deref());
            let resp = client.search_messages(&q, 1, limit.min(200)).await?;
            let rows: Vec<SearchHit> = resp.messages.matches.into_iter()
                .map(|m| SearchHit { ts: m.ts, user: m.user.unwrap_or_default(), text: m.text })
                .collect();
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_list(&rows, "", ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        SearchVerb::Files { query, file_type, limit } => {
            let mut q = query;
            if let Some(t) = file_type { q.push_str(&format!(" type:{t}")); }
            let resp = client.search_files(&q, 1, limit.min(200)).await?;
            // Render flatly; FileDto already has serde + tabled.
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            crate::output::render_list(&resp.files.matches, "", ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
    }
}

fn compose_query(base: &str, in_chan: Option<&str>, from_user: Option<&str>) -> String {
    let mut q = base.to_owned();
    if let Some(ch) = in_chan { q.push_str(&format!(" in:{ch}")); }
    if let Some(u) = from_user { q.push_str(&format!(" from:{u}")); }
    q
}
```

(`search_files` returns `{ ok, files: { matches: Vec<FileDto>, paging } }`; add the DTO in `slack.rs` if missing. Make `FileDto` derive `Tabled`.)

Commit: `feat: search messages and search files`.

### Task 9.2: `cmd::user`

**Files:** Modify `src/cmd/user.rs`.

```rust
use serde::Serialize;
use tabled::Tabled;

use crate::cli::{UserArgs, UserVerb};
use crate::cmd::Ctx;
use crate::error::CliError;
use crate::id::UserRef;
use crate::output::{render_list, render_one, Format};

#[derive(Debug, Serialize, Tabled)]
pub struct UserRow {
    pub id: String,
    pub name: String,
    pub real_name: String,
}

pub async fn run(args: UserArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = ctx.client.as_ref().ok_or_else(|| CliError::NoToken {
        workspace: ctx.workspace_name.clone(),
    })?;
    match args.verb {
        UserVerb::List { query, limit } => {
            let page = client.users_list(limit.min(200), None).await?;
            let rows: Vec<UserRow> = page.members.into_iter()
                .filter(|u| !u.deleted)
                .filter(|u| query.as_deref().map_or(true, |q| u.name.contains(q)))
                .map(|u| UserRow {
                    id: u.id, name: u.name,
                    real_name: u.real_name.unwrap_or_default(),
                })
                .collect();
            let cursor = page.response_metadata.and_then(|m| m.next_cursor).unwrap_or_default();
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_list(&rows, &cursor, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        UserVerb::View { user } => {
            let uref = UserRef::try_from(user.as_str())?;
            let id = ctx.resolver.user(client, &uref).await?;
            let info = client.users_info(id.as_str()).await?;
            let row = UserRow {
                id: info.user.id, name: info.user.name,
                real_name: info.user.real_name.unwrap_or_default(),
            };
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_one(&row, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
    }
}
```

Commit: `feat: user list and user view`.

### Task 9.3: `cmd::file`

**Files:** Modify `src/cmd/file.rs`, add `indicatif`.

`Cargo.toml`: `indicatif = "0.17"`.

```rust
use std::io::Read;
use std::path::PathBuf;

use crate::cli::{FileArgs, FileVerb};
use crate::cmd::Ctx;
use crate::error::CliError;
use crate::output::{render_list, render_one, Format};

pub async fn run(args: FileArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = ctx.client.as_ref().ok_or_else(|| CliError::NoToken {
        workspace: ctx.workspace_name.clone(),
    })?;
    match args.verb {
        FileVerb::List { channel, user: _user, limit } => {
            let resp = client.files_list(channel.as_deref(), limit.min(200), None).await?;
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_list(&resp.files, "", ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        FileVerb::Upload { path, channel, title, comment, thread, stdin, filename } => {
            let (bytes, name) = if stdin {
                let mut buf = Vec::new();
                std::io::stdin().lock().read_to_end(&mut buf).map_err(CliError::Io)?;
                let name = filename.ok_or_else(|| CliError::InvalidArg("--filename required with --stdin".into()))?;
                (buf, name)
            } else {
                let bytes = std::fs::read(&path).map_err(CliError::Io)?;
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("upload").to_owned();
                (bytes, name)
            };
            if ctx.globals.dry_run {
                return super::emit_dry_run(ctx, "files.upload", "POST /api/files.upload",
                    serde_json::json!({
                        "channel": channel, "title": title, "initial_comment": comment,
                        "thread_ts": thread, "filename": name, "size": bytes.len(),
                    }));
            }
            let resolved_channel = if let Some(c) = channel.as_deref() {
                let cref = crate::id::ChannelRef::try_from(c)?;
                Some(ctx.resolver.channel(client, &cref).await?.to_string())
            } else { None };
            let file = client.files_upload(
                resolved_channel.as_deref(), &name, bytes,
                title.as_deref(), comment.as_deref(), thread.as_deref(),
            ).await?;
            let pretty = ctx.stdout_is_tty && matches!(ctx.format, Format::Json);
            render_one(&file, ctx.format, pretty, None, &mut std::io::stdout().lock())
                .map_err(CliError::Io)
        }
        FileVerb::Download { file_id, output, stdout } => {
            let info = client.files_info(&file_id).await?;
            let url = info.file.url_private.ok_or_else(|| CliError::Config("file has no url_private".into()))?;
            let resp = client.download_authorized(&url).await?;
            if stdout {
                use std::io::Write;
                std::io::stdout().lock().write_all(&resp).map_err(CliError::Io)?;
            } else {
                let out = output.unwrap_or_else(|| PathBuf::from(&info.file.name));
                std::fs::write(&out, &resp).map_err(CliError::Io)?;
                eprintln!("✓ saved {}", out.display());
            }
            Ok(())
        }
    }
}
```

(`download_authorized` needs to be added to `slack.rs` — a `get` with `Authorization` header that returns `Vec<u8>`.)

Commit: `feat: file list/upload/download`.

### Task 9.4: `cmd::completion`

**Files:** Modify `src/cmd/completion.rs`.

```rust
use clap::CommandFactory;

use crate::cli::{Cli, CompletionArgs};
use crate::cmd::Ctx;
use crate::error::CliError;

pub async fn run(args: CompletionArgs, _ctx: &mut Ctx) -> Result<(), CliError> {
    let mut cmd = Cli::command();
    let mut stdout = std::io::stdout().lock();
    clap_complete::generate(args.shell, &mut cmd, "slack-cli", &mut stdout);
    Ok(())
}
```

Commit: `feat: shell completion generator`.

---

## Phase 10 — Integration tests

### Task 10.1: `tests/common/mod.rs` helpers

**Files:** Create `tests/common/mod.rs`.

- [ ] **Step 1: Write helpers**

```rust
//! Shared helpers for integration tests.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;
use wiremock::MockServer;

pub struct Sandbox {
    pub dir: TempDir,
    pub config_path: PathBuf,
    pub server: MockServer,
}

pub async fn sandbox(workspace: &str) -> Sandbox {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, format!(
        r#"
default_workspace = "{ws}"

[workspaces.{ws}]
team_id = "T1"
team_domain = "acme"
token_source = "env"
token_env = "FAKE_TOKEN_{ws}"
"#,
        ws = workspace
    )).unwrap();

    let server = MockServer::start().await;
    Sandbox { dir, config_path, server }
}

pub fn cmd(sb: &Sandbox, ws: &str, token: &str) -> Command {
    let mut c = Command::new(cargo_bin("slack-cli"));
    c.env("SLACK_CLI_CONFIG", &sb.config_path);
    c.env("SLACK_CLI_WORKSPACE", ws);
    c.env(format!("FAKE_TOKEN_{ws}"), token);
    c.env("SLACK_CLI_TOKEN", "");   // do not bleed real tokens
    c.env("NO_COLOR", "1");
    c.env("RUST_LOG", "error");
    // Force every Slack call through wiremock by injecting a base-URL env we
    // honor in slack.rs. (We add this env hook in Task 10.2.)
    c.env("SLACK_CLI_BASE_URL_OVERRIDE", sb.server.uri());
    c
}
```

- [ ] **Step 2: Add deps**

```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3"
insta = { version = "1", features = ["json", "yaml", "filters"] }
pretty_assertions = "1"
```

- [ ] **Step 3: Commit**

```bash
git add tests/common/mod.rs Cargo.toml
git commit -m "test: add shared integration-test helpers (sandbox + cmd)"
```

### Task 10.2: Honor `SLACK_CLI_BASE_URL_OVERRIDE` in production code

**Files:** Modify `src/slack.rs` and `src/cmd/mod.rs`.

- [ ] **Step 1: Read env in `SlackClient::new`**

In `slack.rs`:

```rust
    pub fn new(token: SecretString) -> Result<Self, CliError> {
        let base = std::env::var("SLACK_CLI_BASE_URL_OVERRIDE").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        Self::build(base, token)
    }
```

- [ ] **Step 2: Add unit test for override**

```rust
    #[test]
    fn override_via_env() {
        std::env::set_var("SLACK_CLI_BASE_URL_OVERRIDE", "http://localhost:9");
        let c = SlackClient::new(SecretString::from("xoxp-test")).unwrap();
        assert!(c.base_url.starts_with("http://localhost:9"));
        std::env::remove_var("SLACK_CLI_BASE_URL_OVERRIDE");
    }
```

Make `base_url` `pub(crate)` so the test can read it.

- [ ] **Step 3: Commit**

```bash
cargo test --lib slack
git add src/slack.rs
git commit -m "test: allow base URL override via env for integration tests"
```

### Task 10.3–10.8: Per-command E2E tests

For each subcommand group, create `tests/<group>.rs` with at least:

1. One success path (mock 200, verify stdout JSON snapshot via `insta::assert_snapshot!` with redactions for `ts`, `req_id`, `client_msg_id`).
2. One failure path (mock `ok:false`, verify exit code and stderr JSON).
3. Where applicable: one `--dry-run` test asserting no HTTP call (mount `Mock::given(...).expect(0)`).

Example `tests/message.rs`:

```rust
mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn send_dry_run_makes_no_call() {
    let sb = common::sandbox("acme").await;
    // Refuse any POST to chat.postMessage.
    Mock::given(method("POST")).and(path("/api/chat.postMessage"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&sb.server)
        .await;
    // Channel resolution needs conversations.list ok response.
    Mock::given(method("GET")).and(path("/api/conversations.list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"channels":[{"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}],"response_metadata":{"next_cursor":""}}"#))
        .mount(&sb.server).await;

    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["message", "send", "#general", "--text", "hi", "--dry-run", "--json"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    insta::with_settings!({filters => vec![
        (r"\d{10}\.\d{6}", "[TS]"),
        (r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b", "[UUID]"),
    ]}, { insta::assert_snapshot!("message_send_dry_run", out); });
}
```

Repeat per command. Each test gets its own commit:

- `test: e2e auth whoami`
- `test: e2e channel list`
- `test: e2e message send dry-run`
- `test: e2e message send happy path`
- `test: e2e search messages`
- `test: e2e user view`
- `test: e2e file upload dry-run`

### Task 10.9: Confirmation-gate test for destructive commands

`tests/channel.rs`:

```rust
#[tokio::test(flavor = "current_thread")]
async fn archive_without_yes_in_non_tty_errors_64() {
    let sb = common::sandbox("acme").await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["channel", "archive", "C1"])
        .assert();
    assert.code(64);  // ConfirmationRequired -> 64
}
```

Commit: `test: e2e confirmation-required exit code`.

---

## Phase 11 — CI / Release workflows

### Task 11.1: `.github/workflows/ci.yml`

**Files:** Create `.github/workflows/ci.yml`.

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: short

jobs:
  test:
    name: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: jdx/mise-action@5083fe46898c414b2475087cc79da59e7da859e8 # v2.1.11
        with: { install: true, cache: true }
      - uses: Swatinem/rust-cache@98c8021b550208e191a6a3145459bfc9fb29c4c0 # v2.7.4
      - run: cargo fmt --all -- --check
        if: matrix.os == 'ubuntu-latest'
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo nextest run
      - name: docs
        if: matrix.os == 'ubuntu-latest'
        env:
          RUSTDOCFLAGS: -D warnings
        run: cargo doc --no-deps --document-private-items

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: jdx/mise-action@5083fe46898c414b2475087cc79da59e7da859e8 # v2.1.11
        with: { install: true, cache: true }
      - run: cargo deny check advisories bans licenses sources

  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: dtolnay/rust-toolchain@stable
        with: { toolchain: "1.89" }
      - run: cargo check --all-features

  linux-headless:
    runs-on: ubuntu-latest
    container:
      image: rust:1.89-slim
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - name: Install build deps
        run: apt-get update && apt-get install -y --no-install-recommends pkg-config build-essential
      - run: cargo test --tests
```

Commit: `ci: matrix builds with fmt/clippy/nextest, deny, msrv, headless`.

### Task 11.2: `.github/workflows/release.yml`

```yaml
name: release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  upload:
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
          - target: aarch64-pc-windows-msvc
            os: windows-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: taiki-e/upload-rust-binary-action@d94c2c5e7a35e7d1edd0e6e3b3a4d2d9b94f4f1f # v1.22.1
        with:
          bin: slack-cli
          target: ${{ matrix.target }}
          archive: slack-cli-${{ github.ref_name }}-${{ matrix.target }}
          checksum: sha256
          token: ${{ secrets.GITHUB_TOKEN }}

  changelog:
    runs-on: ubuntu-latest
    needs: upload
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
        with: { fetch-depth: 0 }
      - uses: jdx/mise-action@5083fe46898c414b2475087cc79da59e7da859e8 # v2.1.11
        with: { install: true, cache: true }
      - run: git cliff --tag ${{ github.ref_name }} --output CHANGELOG.md
      - name: Upload CHANGELOG to release
        uses: softprops/action-gh-release@c062e08bd532815e2082a85e87e3ef29c3e6d191 # v2.0.8
        with:
          files: CHANGELOG.md
```

Commit: `ci: release workflow for 6 targets with SHA256SUMS and CHANGELOG`.

### Task 11.3: Smoke test CI locally

- [ ] **Step 1: Run `mise run test`**

```bash
mise install
mise run test
```

Expected: PASS.

- [ ] **Step 2: Commit any lockfile changes**

```bash
git add Cargo.lock
git commit -m "chore: update Cargo.lock after CI smoke test" || echo "no changes"
```

---

## Self-Review checklist

- [ ] **Spec coverage scan** — every section of the spec mapped to ≥1 task:
  - §3 architecture → Phase 0–5
  - §3.4 error → Task 1.1, 5.2
  - §4 commands → Phases 6–9
  - §5 JSON schema → Task 1.3, 5.2, 7.2, 8.2
  - §6 module structure → Phase 0 + every phase below
  - §7 main types → Tasks 1.1, 1.2, 1.3, 2.x, 3.x, 4.1
  - §8 config → Phase 2
  - §9 tests → Phase 10
  - §10 CI/release → Phase 11
  - §11 deps → Task 0.1 + each phase
  - §12 agent UX → Tasks 5.2, 7.2, 8.2 (dry-run JSON), 1.4 (hints)
  - §13 security → 2.x (TokenStore), 3.x (sensitive header)
  - §14 logging → Task 5.3

- [ ] **Placeholder scan** — no "TBD" / "implement later" / "similar to Task N" patterns in the steps above.

- [ ] **Type consistency** — `ChannelId.as_str()`, `UserId.as_str()`, `SlackClient::with_base_url` referenced consistently. `PostMessage::new` field names match struct definition.

- [ ] **JSON schema constant** — `slack-cli/v0` referenced in `SCHEMA` constant once and reused.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-14-slack-cli-mvp.md`. Two execution options:

**1. Subagent-Driven (recommended)** — Fresh subagent per task, two-stage review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with checkpoints.

Which approach?
