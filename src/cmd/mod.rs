//! Command dispatch: build a `Ctx` from CLI args + config, then route.

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
use crate::config::{self, Config};
use crate::error::CliError;
use crate::output::Format;
use crate::resolve::Resolver;
use crate::slack::SlackClient;

/// Runtime context passed by reference to each subcommand handler.
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

impl Ctx {
    /// Borrow the Slack client or return `NoToken` for the current workspace.
    ///
    /// Implemented via the free `require_client` helper called with the
    /// explicit field path so the borrow checker can split this from later
    /// mutable borrows of sibling fields like `self.resolver`. Callers that
    /// also need `self.resolver` concurrently should invoke
    /// `cmd::require_client(&ctx.client, &ctx.workspace_name)` directly.
    pub fn require_client(&self) -> Result<&SlackClient, CliError> {
        require_client(&self.client, &self.workspace_name)
    }

    /// Whether JSON output should be pretty-printed (TTY + Json format).
    #[must_use]
    pub fn pretty_json(&self) -> bool {
        self.stdout_is_tty && matches!(self.format, Format::Json)
    }

    /// Render a single row through the resolved format (table/plain/json).
    pub fn emit_one<T>(&self, row: &T) -> Result<(), CliError>
    where
        T: tabled::Tabled + serde::Serialize,
    {
        crate::output::render_one(
            row,
            self.format,
            self.pretty_json(),
            None,
            &mut std::io::stdout().lock(),
        )
        .map_err(CliError::Io)
    }

    /// Render a paginated list through the resolved format.
    pub fn emit_list<T>(&self, rows: &[T], cursor: &str) -> Result<(), CliError>
    where
        T: tabled::Tabled + serde::Serialize,
    {
        crate::output::render_list(
            rows,
            cursor,
            self.format,
            self.pretty_json(),
            None,
            &mut std::io::stdout().lock(),
        )
        .map_err(CliError::Io)
    }
}

/// Free-function form of [`Ctx::require_client`] so callers that also need
/// to mutably borrow `Ctx::resolver` can split the borrow at the call site:
/// `let client = require_client(&ctx.client, &ctx.workspace_name)?;`.
pub fn require_client<'a>(
    client: &'a Option<SlackClient>,
    workspace_name: &str,
) -> Result<&'a SlackClient, CliError> {
    client.as_ref().ok_or_else(|| CliError::NoToken {
        workspace: workspace_name.to_owned(),
    })
}

/// Top-level command router. Loads config, picks workspace (when required),
/// constructs the Slack client, then dispatches to the chosen subcommand.
pub async fn dispatch(cli: Cli) -> Result<(), CliError> {
    let path = config::resolve_path(cli.globals.config.as_deref())?;
    let cfg = config::load(&path)?;

    // `auth` and `completion` work without an existing workspace / token.
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

/// Shared helper: refuse a destructive op when not confirmed.
///
/// In a TTY, prompts for `yes` and consumes one line from stdin.
/// Outside a TTY, requires `--yes` to be set; otherwise returns
/// `CliError::ConfirmationRequired`.
pub fn require_confirmation(ctx: &Ctx, prompt: &str) -> Result<(), CliError> {
    use std::io::{BufRead, Write};

    if ctx.globals.yes {
        return Ok(());
    }
    if !ctx.stdout_is_tty {
        return Err(CliError::ConfirmationRequired);
    }
    let mut err = std::io::stderr().lock();
    write!(err, "{prompt} type 'yes' to continue: ")?;
    err.flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    if line.trim() == "yes" {
        Ok(())
    } else {
        Err(CliError::ConfirmationRequired)
    }
}

#[derive(serde::Serialize)]
struct DryRunData<'a> {
    dry_run: bool,
    would_call: &'a str,
    endpoint: &'a str,
    payload: serde_json::Value,
}

/// Shared helper: emit a structured `dry_run` envelope on stdout and return.
pub fn emit_dry_run(
    _ctx: &Ctx,
    would_call: &str,
    endpoint: &str,
    payload: serde_json::Value,
) -> Result<(), CliError> {
    use std::io::Write;

    let data = DryRunData {
        dry_run: true,
        would_call,
        endpoint,
        payload,
    };
    let env = crate::output::OkEnvelope {
        ok: true,
        schema: crate::output::SCHEMA,
        data,
        request_id: None,
    };
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &env).map_err(std::io::Error::other)?;
    writeln!(out)?;
    Ok(())
}
