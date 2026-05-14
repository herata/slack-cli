//! `slack-cli completion` subcommand: emit shell completion to stdout.

use clap::CommandFactory;

use crate::cli::{Cli, CompletionArgs};
use crate::cmd::Ctx;
use crate::error::CliError;

#[allow(clippy::unused_async)] // signature matches other subcommand runners
pub async fn run(args: CompletionArgs, _ctx: &mut Ctx) -> Result<(), CliError> {
    let mut cmd = Cli::command();
    let mut stdout = std::io::stdout().lock();
    clap_complete::generate(args.shell, &mut cmd, "slack-cli", &mut stdout);
    Ok(())
}
