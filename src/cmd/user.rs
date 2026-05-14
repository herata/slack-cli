//! `slack-cli user` subcommand handlers.

use serde::Serialize;
use tabled::Tabled;

use crate::cli::{UserArgs, UserVerb};
use crate::cmd::{require_client, Ctx};
use crate::error::CliError;
use crate::id::UserRef;

#[derive(Debug, Serialize, Tabled)]
pub struct UserRow {
    pub id: String,
    pub name: String,
    pub real_name: String,
}

impl From<crate::slack::UserDto> for UserRow {
    fn from(u: crate::slack::UserDto) -> Self {
        Self {
            id: u.id,
            name: u.name,
            real_name: u.real_name.unwrap_or_default(),
        }
    }
}

pub async fn run(args: UserArgs, ctx: &mut Ctx) -> Result<(), CliError> {
    let client = require_client(&ctx.client, &ctx.workspace_name)?;

    match args.verb {
        UserVerb::List { query, limit } => {
            let page = client.users_list(limit.min(200), None).await?;
            let rows: Vec<UserRow> = page
                .members
                .into_iter()
                .filter(|u| !u.deleted)
                .filter(|u| query.as_deref().is_none_or(|q| u.name.contains(q)))
                .map(UserRow::from)
                .collect();
            let cursor = page
                .response_metadata
                .and_then(|m| m.next_cursor)
                .unwrap_or_default();
            ctx.emit_list(&rows, &cursor)
        }
        UserVerb::View { user } => {
            let uref = UserRef::try_from(user.as_str())?;
            let id = ctx.resolver.user(client, &uref).await?;
            let info = client.users_info(id.as_str()).await?;
            let row = UserRow::from(info.user);
            ctx.emit_one(&row)
        }
    }
}
