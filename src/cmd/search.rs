//! `slack-cli search` subcommand handlers.

use std::fmt::Write as _;

use serde::Serialize;
use tabled::Tabled;

use crate::cli::{SearchArgs, SearchVerb};
use crate::cmd::{require_client, Ctx};
use crate::error::CliError;

#[derive(Debug, Serialize, Tabled)]
pub struct MessageHit {
    pub ts: String,
    pub user: String,
    pub text: String,
}

#[derive(Debug, Serialize, Tabled)]
pub struct FileHit {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mimetype: String,
}

pub async fn run(args: SearchArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = require_client(&ctx.client, &ctx.workspace_name)?;

    match args.verb {
        SearchVerb::Messages {
            query,
            r#in,
            from,
            limit,
        } => {
            let q = compose_query(&query, r#in.as_deref(), from.as_deref());
            let resp = client.search_messages(&q, 1, limit.min(200)).await?;
            let rows: Vec<MessageHit> = resp
                .messages
                .matches
                .into_iter()
                .map(|m| MessageHit {
                    ts: m.ts,
                    user: m.user.unwrap_or_default(),
                    text: m.text,
                })
                .collect();
            ctx.emit_list(&rows, "")
        }
        SearchVerb::Files {
            query,
            file_type,
            limit,
        } => {
            let mut q = query;
            if let Some(t) = file_type {
                let _ = write!(q, " type:{t}");
            }
            let resp = client.search_files(&q, 1, limit.min(200)).await?;
            let rows: Vec<FileHit> = resp
                .files
                .matches
                .into_iter()
                .map(|f| FileHit {
                    id: f.id,
                    name: f.name,
                    size: f.size,
                    mimetype: f.mimetype,
                })
                .collect();
            ctx.emit_list(&rows, "")
        }
    }
}

fn compose_query(base: &str, in_chan: Option<&str>, from_user: Option<&str>) -> String {
    let mut q = base.to_owned();
    if let Some(ch) = in_chan {
        let _ = write!(q, " in:{ch}");
    }
    if let Some(u) = from_user {
        let _ = write!(q, " from:{u}");
    }
    q
}
