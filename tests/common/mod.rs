//! Shared helpers for integration tests.
//!
//! Each test sets up a temp config + a wiremock server, then runs the CLI
//! binary via `assert_cmd` with env vars wired to both.

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

/// Build a sandbox with a single workspace `<workspace>` whose token source
/// is `env` pointing to `FAKE_TOKEN_<workspace>`.
pub async fn sandbox(workspace: &str) -> Sandbox {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    let body = format!(
        r#"default_workspace = "{ws}"

[workspaces.{ws}]
team_id = "T1"
team_domain = "acme"
token_source = "env"
token_env = "FAKE_TOKEN_{ws}"
"#,
        ws = workspace,
    );
    std::fs::write(&config_path, body).expect("write config");

    let server = MockServer::start().await;
    Sandbox {
        dir,
        config_path,
        server,
    }
}

/// Build a `Command` for the `slack-cli` binary with all sandbox env wired up.
pub fn cmd(sb: &Sandbox, ws: &str, token: &str) -> Command {
    let mut c = Command::new(cargo_bin("slack-cli"));
    c.env("SLACK_CLI_CONFIG", &sb.config_path);
    c.env("SLACK_CLI_WORKSPACE", ws);
    c.env(format!("FAKE_TOKEN_{ws}"), token);
    c.env_remove("SLACK_CLI_TOKEN");
    c.env("NO_COLOR", "1");
    c.env("RUST_LOG", "error");
    c.env("SLACK_CLI_BASE_URL_OVERRIDE", sb.server.uri());
    c
}
