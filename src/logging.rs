//! Tracing initialization. Always writes to stderr.

use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the global tracing subscriber.
///
/// `verbose` is the count of `-v` flags (0..=3). `quiet` overrides everything
/// to error-level only. Honors `RUST_LOG` when set explicitly.
pub fn init(verbose: u8, quiet: bool) {
    let directive = if quiet {
        "error".to_owned()
    } else {
        match verbose {
            0 | 1 => "slack_cli=info,reqwest=warn,hyper=warn".into(),
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
