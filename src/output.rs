//! Output formatting and the stable `slack-cli/v0` JSON envelope.

use std::io::{self, IsTerminal, Write};

use serde::Serialize;

use crate::error::{CliError, ResourceKind};

pub const SCHEMA: &str = "slack-cli/v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[non_exhaustive]
pub enum Format {
    Table,
    Plain,
    Json,
}

impl Format {
    #[must_use]
    pub fn resolve(flag: Option<Format>, stdout_is_tty: bool) -> Self {
        match (flag, stdout_is_tty) {
            (Some(f), _) => f,
            (None, true) => Format::Table,
            (None, false) => Format::Json,
        }
    }
    #[must_use]
    pub fn detect_default() -> Self {
        Self::resolve(None, io::stdout().is_terminal())
    }
}

#[derive(Debug, Serialize)]
pub struct OkEnvelope<'a, T: Serialize> {
    pub ok: bool,
    pub schema: &'a str,
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListPayload<'a, T: Serialize> {
    pub items: &'a [T],
    pub next_cursor: String,
}

pub fn render_one<T>(
    item: &T,
    fmt: Format,
    pretty_json: bool,
    request_id: Option<String>,
    w: &mut dyn Write,
) -> io::Result<()>
where
    T: tabled::Tabled + Serialize,
{
    match fmt {
        Format::Table => {
            let table = tabled::Table::new([item]).to_string();
            writeln!(w, "{table}")
        }
        Format::Plain => {
            let fields = T::fields(item);
            let line = fields
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect::<Vec<_>>()
                .join("\t");
            writeln!(w, "{line}")
        }
        Format::Json => {
            let env = OkEnvelope {
                ok: true,
                schema: SCHEMA,
                data: item,
                request_id,
            };
            write_json(w, &env, pretty_json)
        }
    }
}

pub fn render_list<T>(
    items: &[T],
    next_cursor: &str,
    fmt: Format,
    pretty_json: bool,
    request_id: Option<String>,
    w: &mut dyn Write,
) -> io::Result<()>
where
    T: tabled::Tabled + Serialize,
{
    match fmt {
        Format::Table => {
            let table = tabled::Table::new(items).to_string();
            writeln!(w, "{table}")
        }
        Format::Plain => {
            for item in items {
                let fields = T::fields(item);
                let line = fields
                    .iter()
                    .map(std::convert::AsRef::as_ref)
                    .collect::<Vec<_>>()
                    .join("\t");
                writeln!(w, "{line}")?;
            }
            Ok(())
        }
        Format::Json => {
            let payload = ListPayload {
                items,
                next_cursor: next_cursor.to_owned(),
            };
            let env = OkEnvelope {
                ok: true,
                schema: SCHEMA,
                data: payload,
                request_id,
            };
            write_json(w, &env, pretty_json)
        }
    }
}

fn write_json<T: Serialize>(w: &mut dyn Write, v: &T, pretty: bool) -> io::Result<()> {
    if pretty {
        serde_json::to_writer_pretty(&mut *w, v).map_err(io::Error::other)?;
    } else {
        serde_json::to_writer(&mut *w, v).map_err(io::Error::other)?;
    }
    writeln!(w)
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum Hint {
    Run { command: String },
    Specify { flag: String },
    List { command: String },
}

#[derive(Debug, Serialize)]
pub struct ErrEnvelope<'a> {
    pub ok: bool,
    pub schema: &'a str,
    pub error: ErrPayload,
}

#[derive(Debug, Serialize)]
pub struct ErrPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<Hint>,
    pub exit_code: u8,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[must_use]
pub fn hint_for_error(err: &CliError) -> Option<Hint> {
    match err {
        CliError::NoToken { .. } => Some(Hint::Run {
            command: "slack-cli auth init".into(),
        }),
        CliError::AmbiguousWorkspace { .. } => Some(Hint::Specify {
            flag: "--workspace".into(),
        }),
        CliError::NotFound {
            kind: ResourceKind::Channel,
            ..
        }
        | CliError::Ambiguous {
            kind: ResourceKind::Channel,
            ..
        } => Some(Hint::List {
            command: "slack-cli channel list".into(),
        }),
        CliError::NotFound {
            kind: ResourceKind::User,
            ..
        }
        | CliError::Ambiguous {
            kind: ResourceKind::User,
            ..
        } => Some(Hint::List {
            command: "slack-cli user list".into(),
        }),
        _ => None,
    }
}

pub fn render_error(err: &CliError, json: bool, w: &mut dyn Write) -> io::Result<()> {
    if json {
        let payload = ErrPayload {
            code: err.code().to_owned(),
            message: err.to_string(),
            hint: hint_for_error(err),
            exit_code: err.exit_code(),
            details: serde_json::Value::Null,
            request_id: None,
        };
        let env = ErrEnvelope {
            ok: false,
            schema: SCHEMA,
            error: payload,
        };
        write_json(w, &env, false)
    } else {
        writeln!(w, "error: {err}")?;
        if let Some(h) = hint_for_error(err) {
            match h {
                Hint::Run { command } => writeln!(w, "hint:  run `{command}`")?,
                Hint::Specify { flag } => writeln!(w, "hint:  specify `{flag}`")?,
                Hint::List { command } => writeln!(w, "hint:  list with `{command}`")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use tabled::Tabled;

    #[derive(Tabled, Serialize)]
    struct Row {
        id: &'static str,
        name: &'static str,
    }

    #[test]
    fn resolve_explicit() {
        assert_eq!(Format::resolve(Some(Format::Plain), true), Format::Plain);
    }
    #[test]
    fn resolve_tty() {
        assert_eq!(Format::resolve(None, true), Format::Table);
    }
    #[test]
    fn resolve_pipe() {
        assert_eq!(Format::resolve(None, false), Format::Json);
    }
    #[test]
    fn one_json_envelope() {
        let row = Row {
            id: "C1",
            name: "general",
        };
        let mut buf = Vec::new();
        render_one(&row, Format::Json, false, None, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"ok\":true"));
        assert!(s.contains("\"schema\":\"slack-cli/v0\""));
        assert!(s.contains("\"id\":\"C1\""));
    }
    #[test]
    fn list_json_has_cursor() {
        let rows = [Row {
            id: "C1",
            name: "general",
        }];
        let mut buf = Vec::new();
        render_list(&rows, "next", Format::Json, false, None, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"items\""));
        assert!(s.contains("\"next_cursor\":\"next\""));
    }
    #[test]
    fn hint_run_for_no_token() {
        let h = hint_for_error(&CliError::NoToken {
            workspace: "acme".into(),
        })
        .unwrap();
        assert!(matches!(h, Hint::Run { .. }));
    }
}
