//! CLI argument grammar built with clap's derive API.
//!
//! All global flags live on `GlobalArgs` with `#[arg(global = true)]` so they
//! flow through to every subcommand without explicit flatten. Subcommand
//! grammars live next to `Cli` for compactness.

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
#[allow(clippy::struct_excessive_bools)]
pub struct GlobalArgs {
    /// Config file path (env: `SLACK_CLI_CONFIG`).
    #[arg(short = 'c', long, global = true, env = "SLACK_CLI_CONFIG")]
    pub config: Option<PathBuf>,

    /// Workspace name (env: `SLACK_CLI_WORKSPACE`).
    #[arg(short = 'w', long, global = true, env = "SLACK_CLI_WORKSPACE")]
    pub workspace: Option<String>,

    /// Force JSON output.
    #[arg(long, global = true, group = "output_fmt")]
    pub json: bool,

    /// Force tab-separated plain output.
    #[arg(long, global = true, group = "output_fmt")]
    pub plain: bool,

    /// Force table output.
    #[arg(long, global = true, group = "output_fmt")]
    pub table: bool,

    /// Increase logging verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress info logging (errors still print).
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Disable ANSI colour output (also honors `NO_COLOR`).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Confirm destructive operations non-interactively (alias: --no-confirm).
    #[arg(long, global = true, visible_alias = "no-confirm")]
    pub yes: bool,

    /// Maximum items returned per page (default 50, capped at 200).
    #[arg(long, global = true, default_value_t = 50)]
    pub max_results: u32,

    /// Fetch every page (pair with --max-pages to bound).
    #[arg(long, global = true)]
    pub all: bool,

    /// Maximum pages fetched when --all is set.
    #[arg(long, global = true, default_value_t = 10)]
    pub max_pages: u32,

    /// For write/admin commands: print the would-be request without sending.
    #[arg(long, global = true)]
    pub dry_run: bool,
}

impl GlobalArgs {
    /// Resolve the explicit output format from CLI flags, if any.
    #[must_use]
    pub fn format(&self) -> Option<Format> {
        if self.json {
            Some(Format::Json)
        } else if self.plain {
            Some(Format::Plain)
        } else if self.table {
            Some(Format::Table)
        } else {
            None
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum TopCommand {
    /// Authentication and workspace management.
    Auth(AuthArgs),
    /// Slack channels.
    Channel(ChannelArgs),
    /// Slack messages.
    Message(MessageArgs),
    /// Slack search.
    Search(SearchArgs),
    /// Slack users.
    User(UserArgs),
    /// Slack files.
    File(FileArgs),
    /// Generate shell completion scripts.
    Completion(CompletionArgs),
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub verb: AuthVerb,
}

#[derive(Debug, Subcommand)]
pub enum AuthVerb {
    /// Interactively configure a workspace.
    Init {
        #[arg(short = 'w', long)]
        workspace: Option<String>,
        #[arg(long)]
        token_stdin: bool,
        #[arg(long, value_enum, default_value_t = AuthBackend::Keyring)]
        backend: AuthBackend,
    },
    /// Show the authenticated identity.
    Whoami,
    /// List configured workspaces.
    List,
    /// Delete the stored token for a workspace.
    Logout {
        #[arg(short = 'w', long)]
        workspace: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AuthBackend {
    Keyring,
    File,
    Env,
}

#[derive(Debug, Args)]
pub struct ChannelArgs {
    #[command(subcommand)]
    pub verb: ChannelVerb,
}

#[derive(Debug, Subcommand)]
pub enum ChannelVerb {
    List {
        #[arg(short = 'q', long)]
        query: Option<String>,
        #[arg(long, default_value = "public_channel,private_channel")]
        types: String,
    },
    View {
        channel: String,
    },
    Create {
        name: String,
        #[arg(long)]
        private: bool,
        #[arg(long)]
        description: Option<String>,
    },
    Archive {
        channel: String,
    },
    Invite {
        channel: String,
        users: Vec<String>,
    },
    Leave {
        channel: String,
    },
}

#[derive(Debug, Args)]
pub struct MessageArgs {
    #[command(subcommand)]
    pub verb: MessageVerb,
}

#[derive(Debug, Subcommand)]
pub enum MessageVerb {
    List {
        channel: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        oldest: Option<String>,
        #[arg(long)]
        latest: Option<String>,
    },
    View {
        channel: String,
        ts: String,
        #[arg(long)]
        thread: bool,
    },
    Send {
        channel: String,
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        #[arg(long, conflicts_with = "text")]
        file: Option<PathBuf>,
        #[arg(long)]
        thread: Option<String>,
        #[arg(long)]
        broadcast: bool,
    },
    Reply {
        channel: String,
        ts: String,
        #[arg(long)]
        text: String,
        #[arg(long)]
        broadcast: bool,
    },
    Update {
        channel: String,
        ts: String,
        #[arg(long)]
        text: String,
    },
    Delete {
        channel: String,
        ts: String,
    },
    React {
        channel: String,
        ts: String,
        emoji: String,
        #[arg(long)]
        remove: bool,
    },
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[command(subcommand)]
    pub verb: SearchVerb,
}

#[derive(Debug, Subcommand)]
pub enum SearchVerb {
    Messages {
        query: String,
        #[arg(long)]
        r#in: Option<String>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    Files {
        query: String,
        #[arg(long, name = "type")]
        file_type: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

#[derive(Debug, Args)]
pub struct UserArgs {
    #[command(subcommand)]
    pub verb: UserVerb,
}

#[derive(Debug, Subcommand)]
pub enum UserVerb {
    List {
        #[arg(short = 'q', long)]
        query: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    View {
        user: String,
    },
}

#[derive(Debug, Args)]
pub struct FileArgs {
    #[command(subcommand)]
    pub verb: FileVerb,
}

#[derive(Debug, Subcommand)]
pub enum FileVerb {
    List {
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    Upload {
        path: PathBuf,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        thread: Option<String>,
        #[arg(long)]
        stdin: bool,
        #[arg(long)]
        filename: Option<String>,
    },
    Download {
        file_id: String,
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
        #[arg(long)]
        stdout: bool,
    },
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    pub shell: clap_complete::Shell,
}
