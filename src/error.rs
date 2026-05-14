//! CLI-wide error type. All commands return `Result<T, CliError>`.

use std::io;

/// The kind of Slack resource a name resolution targets.
///
/// Used by `CliError::NotFound` and `CliError::Ambiguous` to give callers
/// (especially the JSON output hint mapper) a typed view of which lookup
/// failed without relying on string comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceKind {
    Channel,
    User,
    File,
    Message,
}

impl ResourceKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::User => "user",
            Self::File => "file",
            Self::Message => "message",
        }
    }
}

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CliError {
    #[error("no token configured for workspace '{workspace}'")]
    NoToken { workspace: String },

    #[error("ambiguous workspace: specify --workspace (available: {})", available.join(", "))]
    AmbiguousWorkspace { available: Vec<String> },

    #[error("token kind mismatch: '{api}' requires {needed} token, got {actual}")]
    TokenKindMismatch {
        needed: &'static str,
        actual: &'static str,
        api: &'static str,
    },

    #[error("{kind} '{name}' not found")]
    NotFound { kind: ResourceKind, name: String },

    #[error("ambiguous {kind} reference '{name}' ({} candidates)", candidates.len())]
    Ambiguous {
        kind: ResourceKind,
        name: String,
        candidates: Vec<String>,
    },

    #[error("slack api error: {code}")]
    SlackApi {
        code: String,
        request_id: Option<String>,
    },

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("invalid argument: {0}")]
    InvalidArg(String),

    #[error("confirmation required: pass --yes for non-interactive use")]
    ConfirmationRequired,

    #[error("file token backend is not supported on this platform")]
    FileBackendUnsupported,

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("network error: {0}")]
    Network(String),
}

impl CliError {
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NoToken { .. } | Self::AuthFailed { .. } => 3,
            Self::AmbiguousWorkspace { .. } | Self::Config(_) | Self::FileBackendUnsupported => 2,
            Self::RateLimited { .. } => 4,
            Self::NotFound { .. } | Self::Ambiguous { .. } => 5,
            Self::TokenKindMismatch { .. } => 6,
            Self::InvalidArg(_) | Self::ConfirmationRequired => 64,
            _ => 1,
        }
    }

    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoToken { .. } => "no_token",
            Self::AmbiguousWorkspace { .. } => "workspace_ambiguous",
            Self::TokenKindMismatch { .. } => "token_kind_mismatch",
            Self::NotFound { .. } => "not_found",
            Self::Ambiguous { .. } => "ambiguous",
            Self::SlackApi { .. } => "slack_api",
            Self::RateLimited { .. } => "rate_limited",
            Self::AuthFailed { .. } => "auth_failed",
            Self::InvalidArg(_) => "invalid_arg",
            Self::ConfirmationRequired => "confirmation_required",
            Self::FileBackendUnsupported => "file_backend_unsupported",
            Self::Config(_) => "config",
            Self::Io(_) => "io",
            Self::Network(_) => "network",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_auth_failed_is_3() {
        assert_eq!(CliError::AuthFailed { reason: "x".into() }.exit_code(), 3);
    }
    #[test]
    fn exit_code_not_found_is_5() {
        assert_eq!(
            CliError::NotFound {
                kind: ResourceKind::Channel,
                name: "x".into()
            }
            .exit_code(),
            5
        );
    }
    #[test]
    fn exit_code_token_mismatch_is_6() {
        assert_eq!(
            CliError::TokenKindMismatch {
                needed: "user",
                actual: "bot",
                api: "search.messages"
            }
            .exit_code(),
            6,
        );
    }
    #[test]
    fn exit_code_invalid_arg_is_64() {
        assert_eq!(CliError::InvalidArg("x".into()).exit_code(), 64);
    }
    #[test]
    fn display_lowercase_no_period() {
        let s = CliError::NoToken {
            workspace: "acme".into(),
        }
        .to_string();
        assert!(s.starts_with(char::is_lowercase));
        assert!(!s.ends_with('.'));
    }
}
