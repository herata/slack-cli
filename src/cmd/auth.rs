//! `slack-cli auth` subcommand handlers.

use std::io::{BufRead, IsTerminal, Write};

use secrecy::SecretString;
use serde::Serialize;
use tabled::Tabled;

use crate::cli::{AuthArgs, AuthBackend, AuthVerb};
use crate::cmd::Ctx;
use crate::config::{TokenSource, WorkspaceConfig, WorkspaceMeta};
use crate::error::CliError;
use crate::slack::SlackClient;

#[derive(Debug, Serialize, Tabled)]
pub struct WhoamiRow {
    pub user_id: String,
    pub user_name: String,
    pub team_id: String,
    pub team_domain: String,
    pub token_kind: String,
}

#[derive(Debug, Serialize, Tabled)]
pub struct WorkspaceRow {
    pub name: String,
    pub team_id: String,
    pub token_source: String,
}

pub async fn run(args: AuthArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    match args.verb {
        AuthVerb::Init {
            workspace,
            token_stdin,
            backend,
        } => init(workspace, token_stdin, backend, ctx).await,
        AuthVerb::Whoami => whoami(ctx).await,
        AuthVerb::List => list(ctx),
        AuthVerb::Logout { workspace } => logout(workspace, ctx),
    }
}

async fn init(
    workspace: Option<String>,
    token_stdin: bool,
    backend: AuthBackend,
    ctx: &mut Ctx,
) -> Result<(), CliError> {
    let name = match workspace {
        Some(n) => n,
        None => prompt("workspace name: ")?,
    };
    if name.trim().is_empty() {
        return Err(CliError::InvalidArg("workspace name required".into()));
    }

    let raw = if token_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        line.trim().to_owned()
    } else {
        rpassword::prompt_password("token (input hidden): ")
            .map_err(|e| CliError::InvalidArg(format!("token prompt: {e}")))?
    };
    crate::secret::validate_prefix(&raw)?;
    let secret = SecretString::from(raw);

    let client = SlackClient::new(secret.clone())?;
    let info = client.auth_test().await?;
    let token_kind = if info.bot_id.is_some() { "bot" } else { "user" };

    let token_source = match backend {
        AuthBackend::Keyring => TokenSource::Keyring,
        AuthBackend::File => TokenSource::File,
        AuthBackend::Env => TokenSource::Env,
    };

    let mut ws = WorkspaceConfig {
        team_id: info.team_id.clone(),
        team_domain: info.team.clone(),
        token_source,
        token: None,
        token_env: None,
        meta: WorkspaceMeta {
            token_kind: Some(token_kind.into()),
            user_id: info.user_id.clone(),
            updated_at: Some(rfc3339_now()),
        },
    };
    crate::secret::save(&name, &mut ws, &secret)?;

    let key = name.clone();
    ctx.config.workspaces.insert(key.clone(), ws);
    if ctx.config.default_workspace.is_none() {
        ctx.config.default_workspace = Some(key);
    }
    crate::config::save(&ctx.config_path, &ctx.config)?;

    eprintln!("\u{2713} saved workspace '{name}'");
    Ok(())
}

async fn whoami(ctx: &mut Ctx) -> Result<(), CliError> {
    let (name, ws) = crate::config::pick_workspace(&ctx.config, ctx.globals.workspace.as_deref())?;
    let secret = crate::secret::load(name, ws)?;
    let client = SlackClient::new(secret)?;
    let info = client.auth_test().await?;
    let kind = if info.bot_id.is_some() { "bot" } else { "user" };
    let row = WhoamiRow {
        user_id: info.user_id.unwrap_or_default(),
        user_name: info.user.unwrap_or_default(),
        team_id: info.team_id.unwrap_or_default(),
        team_domain: info.team.unwrap_or_default(),
        token_kind: kind.into(),
    };
    ctx.emit_one(&row)
}

fn list(ctx: &mut Ctx) -> Result<(), CliError> {
    let rows: Vec<WorkspaceRow> = ctx
        .config
        .workspaces
        .iter()
        .map(|(name, w)| WorkspaceRow {
            name: name.clone(),
            team_id: w.team_id.clone().unwrap_or_default(),
            token_source: match w.token_source {
                TokenSource::Keyring => "keyring".into(),
                TokenSource::File => "file".into(),
                TokenSource::Env => "env".into(),
            },
        })
        .collect();
    ctx.emit_list(&rows, "")
}

fn logout(workspace: Option<String>, ctx: &mut Ctx) -> Result<(), CliError> {
    let name = workspace
        .or_else(|| {
            if ctx.workspace_name.is_empty() {
                ctx.config.default_workspace.clone()
            } else {
                Some(ctx.workspace_name.clone())
            }
        })
        .ok_or_else(|| CliError::AmbiguousWorkspace {
            available: ctx.config.workspaces.keys().cloned().collect(),
        })?;

    if !ctx.globals.yes {
        if std::io::stdin().is_terminal() {
            let line = prompt(&format!(
                "delete token for '{name}'? type 'yes' to confirm: "
            ))?;
            if line.trim() != "yes" {
                return Err(CliError::ConfirmationRequired);
            }
        } else {
            return Err(CliError::ConfirmationRequired);
        }
    }

    if let Some(ws) = ctx.config.workspaces.get_mut(&name) {
        crate::secret::delete(&name, ws)?;
        ws.token = None;
    }
    crate::config::save(&ctx.config_path, &ctx.config)?;
    eprintln!("\u{2713} logged out '{name}'");
    Ok(())
}

fn prompt(msg: &str) -> Result<String, CliError> {
    let mut err = std::io::stderr().lock();
    err.write_all(msg.as_bytes())?;
    err.flush()?;
    let mut buf = String::new();
    std::io::stdin().lock().read_line(&mut buf)?;
    Ok(buf.trim().to_owned())
}

fn rfc3339_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // Minimal RFC3339-ish; we don't pull chrono just for this stamp.
    format!("@{secs}")
}
