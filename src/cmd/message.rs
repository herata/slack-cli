//! `slack-cli message` subcommand handlers.

use std::io::Read;
use std::path::Path;

use serde::Serialize;
use tabled::Tabled;

use crate::cli::{MessageArgs, MessageVerb};
use crate::cmd::{emit_dry_run, require_client, require_confirmation, Ctx};
use crate::error::{CliError, ResourceKind};
use crate::id::ChannelRef;
use crate::slack::PostMessage;

#[derive(Debug, Serialize, Tabled)]
pub struct MessageRow {
    pub ts: String,
    pub user: String,
    pub text: String,
}

impl From<crate::slack::MessageDto> for MessageRow {
    fn from(m: crate::slack::MessageDto) -> Self {
        Self {
            ts: m.ts,
            user: m.user.unwrap_or_default(),
            text: m.text,
        }
    }
}

#[allow(clippy::too_many_lines)]
pub async fn run(args: MessageArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = require_client(&ctx.client, &ctx.workspace_name)?;

    match args.verb {
        MessageVerb::List {
            channel,
            limit,
            oldest,
            latest,
        } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            let page = client
                .conversations_history(
                    id.as_str(),
                    limit.min(200),
                    None,
                    oldest.as_deref(),
                    latest.as_deref(),
                )
                .await?;
            let rows: Vec<MessageRow> = page.messages.into_iter().map(MessageRow::from).collect();
            let cursor = page
                .response_metadata
                .and_then(|m| m.next_cursor)
                .unwrap_or_default();
            ctx.emit_list(&rows, &cursor)
        }

        MessageVerb::View {
            channel,
            ts,
            thread,
        } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let id = ctx.resolver.channel(client, &cref).await?;
            if thread {
                let page = client
                    .conversations_replies(id.as_str(), &ts, None, 100)
                    .await?;
                let rows: Vec<MessageRow> =
                    page.messages.into_iter().map(MessageRow::from).collect();
                ctx.emit_list(&rows, "")
            } else {
                let page = client
                    .conversations_history(id.as_str(), 1, None, Some(&ts), Some(&ts))
                    .await?;
                let m = page
                    .messages
                    .into_iter()
                    .next()
                    .ok_or_else(|| CliError::NotFound {
                        kind: ResourceKind::Message,
                        name: ts.clone(),
                    })?;
                let row = MessageRow::from(m);
                ctx.emit_one(&row)
            }
        }

        MessageVerb::Send {
            channel,
            text,
            file,
            thread,
            broadcast,
        } => {
            let body = read_body(text.as_deref(), file.as_deref())?;
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let mut req = PostMessage::new(cid.as_str().to_owned(), body);
            req.thread_ts = thread;
            req.reply_broadcast = if broadcast { Some(true) } else { None };
            if ctx.globals.dry_run {
                let payload = serde_json::to_value(&req).map_err(std::io::Error::other)?;
                return emit_dry_run(
                    ctx,
                    "chat.postMessage",
                    "POST /api/chat.postMessage",
                    payload,
                );
            }
            let resp = client.chat_post_message(&req).await?;
            // Slack guarantees `resp.ts == resp.message.ts`; rely on that
            // so `MessageRow::from` carries the envelope timestamp.
            let row = MessageRow::from(resp.message);
            ctx.emit_one(&row)
        }

        MessageVerb::Reply {
            channel,
            ts,
            text,
            broadcast,
        } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            let mut req = PostMessage::new(cid.as_str().to_owned(), text);
            req.thread_ts = Some(ts);
            req.reply_broadcast = if broadcast { Some(true) } else { None };
            if ctx.globals.dry_run {
                let payload = serde_json::to_value(&req).map_err(std::io::Error::other)?;
                return emit_dry_run(
                    ctx,
                    "chat.postMessage",
                    "POST /api/chat.postMessage",
                    payload,
                );
            }
            let resp = client.chat_post_message(&req).await?;
            // Slack guarantees `resp.ts == resp.message.ts`; rely on that
            // so `MessageRow::from` carries the envelope timestamp.
            let row = MessageRow::from(resp.message);
            ctx.emit_one(&row)
        }

        MessageVerb::Update { channel, ts, text } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "chat.update",
                    "POST /api/chat.update",
                    serde_json::json!({"channel": cid.as_str(), "ts": ts, "text": text}),
                );
            }
            client.chat_update(cid.as_str(), &ts, &text).await?;
            eprintln!("✓ updated");
            Ok(())
        }

        MessageVerb::Delete { channel, ts } => {
            require_confirmation(ctx, &format!("delete message {ts} in '{channel}'?"))?;
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "chat.delete",
                    "POST /api/chat.delete",
                    serde_json::json!({"channel": cid.as_str(), "ts": ts}),
                );
            }
            client.chat_delete(cid.as_str(), &ts).await?;
            eprintln!("✓ deleted");
            Ok(())
        }

        MessageVerb::React {
            channel,
            ts,
            emoji,
            remove,
        } => {
            let cref = ChannelRef::try_from(channel.as_str())?;
            let cid = ctx.resolver.channel(client, &cref).await?;
            // Strip surrounding ':' if user typed `:thumbsup:`.
            let name = emoji.trim_matches(':');
            if ctx.globals.dry_run {
                let endpoint = if remove {
                    "POST /api/reactions.remove"
                } else {
                    "POST /api/reactions.add"
                };
                let would_call = if remove {
                    "reactions.remove"
                } else {
                    "reactions.add"
                };
                return emit_dry_run(
                    ctx,
                    would_call,
                    endpoint,
                    serde_json::json!({"channel": cid.as_str(), "timestamp": ts, "name": name}),
                );
            }
            if remove {
                client.reactions_remove(cid.as_str(), &ts, name).await?;
                eprintln!("✓ reaction removed");
            } else {
                client.reactions_add(cid.as_str(), &ts, name).await?;
                eprintln!("✓ reaction added");
            }
            Ok(())
        }
    }
}

/// Resolve the message body from `--text` / `--file` flags or stdin (when
/// either flag is `-`). Returns `InvalidArg` for empty/conflicting input.
fn read_body(text: Option<&str>, file: Option<&Path>) -> Result<String, CliError> {
    match (text, file) {
        (Some("-"), None) => {
            let mut s = String::new();
            std::io::stdin().lock().read_to_string(&mut s)?;
            Ok(s.trim_end_matches('\n').to_owned())
        }
        (None, Some(p)) if p.as_os_str() == "-" => {
            let mut s = String::new();
            std::io::stdin().lock().read_to_string(&mut s)?;
            Ok(s.trim_end_matches('\n').to_owned())
        }
        (Some(s), None) => Ok(s.to_owned()),
        (None, Some(p)) => Ok(std::fs::read_to_string(p)?),
        (None, None) => Err(CliError::InvalidArg("--text or --file required".into())),
        (Some(_), Some(_)) => Err(CliError::InvalidArg(
            "--text and --file are mutually exclusive".into(),
        )),
    }
}
