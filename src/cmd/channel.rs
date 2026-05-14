//! `slack-cli channel` subcommand handlers.

use serde::Serialize;
use tabled::Tabled;

use crate::cli::{ChannelArgs, ChannelVerb};
use crate::cmd::{emit_dry_run, require_client, require_confirmation, Ctx};
use crate::error::CliError;
use crate::id::{ChannelRef, UserRef};

#[derive(Debug, Serialize, Tabled)]
pub struct ChannelRow {
    pub id: String,
    pub name: String,
    pub is_private: bool,
    pub members: u32,
}

impl From<crate::slack::ChannelDto> for ChannelRow {
    fn from(c: crate::slack::ChannelDto) -> Self {
        Self {
            id: c.id,
            name: c.name,
            is_private: c.is_private,
            members: c.num_members.unwrap_or(0),
        }
    }
}

#[allow(clippy::too_many_lines)]
pub async fn run(args: ChannelArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = require_client(&ctx.client, &ctx.workspace_name)?;

    match args.verb {
        ChannelVerb::List { query, types } => {
            let limit = ctx.globals.max_results.min(200);
            let max_pages = if ctx.globals.all {
                ctx.globals.max_pages
            } else {
                1
            };
            let mut cursor: Option<String> = None;
            let mut rows: Vec<ChannelRow> = Vec::new();
            let mut next_cursor = String::new();
            for _ in 0..max_pages {
                let page = client
                    .conversations_list(&types, limit, cursor.as_deref())
                    .await?;
                for ch in page.channels {
                    if let Some(q) = query.as_deref() {
                        if !ch.name.contains(q) {
                            continue;
                        }
                    }
                    rows.push(ChannelRow::from(ch));
                }
                next_cursor = page
                    .response_metadata
                    .and_then(|m| m.next_cursor)
                    .unwrap_or_default();
                if next_cursor.is_empty() {
                    break;
                }
                cursor = Some(next_cursor.clone());
            }
            ctx.emit_list(&rows, &next_cursor)
        }

        ChannelVerb::View { channel } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            let info = client.conversations_info(id.as_str()).await?;
            let row = ChannelRow::from(info.channel);
            ctx.emit_one(&row)
        }

        ChannelVerb::Create {
            name,
            private,
            description,
        } => {
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "conversations.create",
                    "POST /api/conversations.create",
                    serde_json::json!({
                        "name": name,
                        "is_private": private,
                        "description": description,
                    }),
                );
            }
            let resp = client
                .conversations_create(&name, private, description.as_deref())
                .await?;
            let row = ChannelRow::from(resp.channel);
            ctx.emit_one(&row)
        }

        ChannelVerb::Archive { channel } => {
            require_confirmation(ctx, &format!("archive channel '{channel}'?"))?;
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "conversations.archive",
                    "POST /api/conversations.archive",
                    serde_json::json!({"channel": channel}),
                );
            }
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            client.conversations_archive(id.as_str()).await?;
            eprintln!("✓ archived");
            Ok(())
        }

        ChannelVerb::Invite { channel, users } => {
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "conversations.invite",
                    "POST /api/conversations.invite",
                    serde_json::json!({"channel": channel, "users": users}),
                );
            }
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let mut uids: Vec<String> = Vec::with_capacity(users.len());
            for u in &users {
                let uref = UserRef::try_from(u.as_str())?;
                uids.push(ctx.resolver.user(client, &uref).await?.to_string());
            }
            client.conversations_invite(cid.as_str(), &uids).await?;
            eprintln!("✓ invited {} user(s)", uids.len());
            Ok(())
        }

        ChannelVerb::Leave { channel } => {
            require_confirmation(ctx, &format!("leave channel '{channel}'?"))?;
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "conversations.leave",
                    "POST /api/conversations.leave",
                    serde_json::json!({"channel": channel}),
                );
            }
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            client.conversations_leave(id.as_str()).await?;
            eprintln!("✓ left");
            Ok(())
        }
    }
}
