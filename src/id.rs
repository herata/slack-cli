//! Strongly-typed Slack identifiers.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::CliError;

static ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[CGDUW][A-Z0-9]{8,}$").expect("valid regex"));

macro_rules! string_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub(crate) String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(ChannelId, "Slack channel id (`Cxxxx`, `Gxxxx`, `Dxxxx`).");
string_id!(UserId, "Slack user id (`Uxxxx`, `Wxxxx`).");
string_id!(Ts, "Slack message timestamp.");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelRef {
    Name(String),
    Id(ChannelId),
}

impl TryFrom<&str> for ChannelRef {
    type Error = CliError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CliError::InvalidArg("empty channel reference".into()));
        }
        let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
        if body.is_empty() {
            return Err(CliError::InvalidArg("channel reference is only '#'".into()));
        }
        if body.starts_with(['C', 'G', 'D']) && ID_RE.is_match(body) {
            Ok(Self::Id(ChannelId(body.to_owned())))
        } else {
            Ok(Self::Name(body.to_owned()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRef {
    Name(String),
    Id(UserId),
}

impl TryFrom<&str> for UserRef {
    type Error = CliError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(CliError::InvalidArg("empty user reference".into()));
        }
        let body = trimmed.strip_prefix('@').unwrap_or(trimmed);
        if body.is_empty() {
            return Err(CliError::InvalidArg("user reference is only '@'".into()));
        }
        if body.starts_with(['U', 'W']) && ID_RE.is_match(body) {
            Ok(Self::Id(UserId(body.to_owned())))
        } else {
            Ok(Self::Name(body.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hash_name() {
        assert_eq!(
            ChannelRef::try_from("#general").unwrap(),
            ChannelRef::Name("general".into())
        );
    }
    #[test]
    fn parses_bare_name() {
        assert_eq!(
            ChannelRef::try_from("general").unwrap(),
            ChannelRef::Name("general".into())
        );
    }
    #[test]
    fn parses_id() {
        assert_eq!(
            ChannelRef::try_from("C012ABCD9").unwrap(),
            ChannelRef::Id(ChannelId("C012ABCD9".into())),
        );
    }
    #[test]
    fn rejects_empty() {
        assert!(ChannelRef::try_from("").is_err());
        assert!(ChannelRef::try_from("#").is_err());
    }
    #[test]
    fn user_ref_at_name() {
        assert_eq!(
            UserRef::try_from("@alice").unwrap(),
            UserRef::Name("alice".into())
        );
    }
    #[test]
    fn user_ref_enterprise() {
        assert_eq!(
            UserRef::try_from("W0123ABCD").unwrap(),
            UserRef::Id(UserId("W0123ABCD".into()))
        );
    }
    #[test]
    fn short_id_lookalike_is_name() {
        assert_eq!(
            ChannelRef::try_from("C123").unwrap(),
            ChannelRef::Name("C123".into())
        );
    }
}
