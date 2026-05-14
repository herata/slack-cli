//! Binary entry point for `slack-cli`.

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
            let json_mode = matches!(Format::detect_default(), Format::Json);
            let _ = render_error(&e, json_mode, &mut std::io::stderr().lock());
            ExitCode::from(e.exit_code())
        }
    }
}
