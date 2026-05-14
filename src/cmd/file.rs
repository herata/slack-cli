//! `slack-cli file` subcommand handlers.

use std::io::Read;
use std::path::PathBuf;

use serde::Serialize;
use tabled::Tabled;

use crate::cli::{FileArgs, FileVerb};
use crate::cmd::{emit_dry_run, require_client, Ctx};
use crate::error::CliError;
use crate::id::ChannelRef;

#[derive(Debug, Serialize, Tabled)]
pub struct FileRow {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mimetype: String,
}

impl From<crate::slack::FileDto> for FileRow {
    fn from(f: crate::slack::FileDto) -> Self {
        Self {
            id: f.id,
            name: f.name,
            size: f.size,
            mimetype: f.mimetype,
        }
    }
}

#[allow(clippy::too_many_lines)]
pub async fn run(args: FileArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = require_client(&ctx.client, &ctx.workspace_name)?;

    match args.verb {
        FileVerb::List {
            channel,
            user: _user,
            limit,
        } => {
            let resp = client
                .files_list(channel.as_deref(), limit.min(200), None)
                .await?;
            let rows: Vec<FileRow> = resp.files.into_iter().map(FileRow::from).collect();
            ctx.emit_list(&rows, "")
        }

        FileVerb::Upload {
            path,
            channel,
            title,
            comment,
            thread,
            stdin,
            filename,
        } => {
            let (bytes, name) = if stdin {
                let mut buf = Vec::new();
                std::io::stdin().lock().read_to_end(&mut buf)?;
                let name = filename.ok_or_else(|| {
                    CliError::InvalidArg("--filename required with --stdin".into())
                })?;
                (buf, name)
            } else {
                let bytes = std::fs::read(&path)?;
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("upload")
                    .to_owned();
                (bytes, name)
            };
            if ctx.globals.dry_run {
                return emit_dry_run(
                    ctx,
                    "files.upload",
                    "POST /api/files.upload",
                    serde_json::json!({
                        "channel": channel,
                        "title": title,
                        "initial_comment": comment,
                        "thread_ts": thread,
                        "filename": name,
                        "size": bytes.len(),
                    }),
                );
            }
            let resolved_channel = if let Some(c) = channel.as_deref() {
                let cref = ChannelRef::try_from(c)?;
                Some(ctx.resolver.channel(client, &cref).await?.to_string())
            } else {
                None
            };
            let f = client
                .files_upload(
                    resolved_channel.as_deref(),
                    &name,
                    bytes,
                    title.as_deref(),
                    comment.as_deref(),
                    thread.as_deref(),
                )
                .await?;
            let row = FileRow {
                id: f.id,
                name: f.name,
                size: f.size,
                mimetype: f.mimetype,
            };
            ctx.emit_one(&row)
        }

        FileVerb::Download {
            file_id,
            output,
            stdout,
        } => {
            let info = client.files_info(&file_id).await?;
            let url = info
                .file
                .url_private
                .ok_or_else(|| CliError::Config("file has no url_private".into()))?;
            let bytes = client.download_authorized(&url).await?;
            if stdout {
                use std::io::Write;
                std::io::stdout().lock().write_all(&bytes)?;
                Ok(())
            } else {
                let out_path = output.unwrap_or_else(|| PathBuf::from(&info.file.name));
                std::fs::write(&out_path, &bytes)?;
                eprintln!("\u{2713} saved {}", out_path.display());
                Ok(())
            }
        }
    }
}
