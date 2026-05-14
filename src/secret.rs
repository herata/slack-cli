//! Token storage with three backends: keyring (OS), file (config.toml), env.

use secrecy::{ExposeSecret, SecretString};

use crate::config::{TokenSource, WorkspaceConfig};
use crate::error::CliError;

pub fn validate_prefix(raw: &str) -> Result<(), CliError> {
    if raw.starts_with("xoxp-") || raw.starts_with("xoxb-") {
        Ok(())
    } else {
        Err(CliError::InvalidArg(
            "token must start with 'xoxp-' or 'xoxb-'".into(),
        ))
    }
}

pub fn load(workspace_name: &str, ws: &WorkspaceConfig) -> Result<SecretString, CliError> {
    if let Ok(t) = std::env::var("SLACK_CLI_TOKEN") {
        validate_prefix(&t)?;
        return Ok(SecretString::from(t));
    }
    match ws.token_source {
        TokenSource::Env => {
            let name = ws.token_env.as_deref().ok_or_else(|| {
                CliError::Config(format!("workspace '{workspace_name}' has no token_env"))
            })?;
            let t = std::env::var(name).map_err(|_| CliError::NoToken {
                workspace: workspace_name.into(),
            })?;
            validate_prefix(&t)?;
            Ok(SecretString::from(t))
        }
        TokenSource::File => {
            #[cfg(target_os = "windows")]
            {
                let _ = workspace_name;
                let _ = ws;
                Err(CliError::FileBackendUnsupported)
            }
            #[cfg(not(target_os = "windows"))]
            {
                let t = ws.token.as_deref().ok_or_else(|| CliError::NoToken {
                    workspace: workspace_name.into(),
                })?;
                validate_prefix(t)?;
                Ok(SecretString::from(t.to_owned()))
            }
        }
        TokenSource::Keyring => keyring_load(workspace_name),
    }
}

pub fn save(
    workspace_name: &str,
    ws: &mut WorkspaceConfig,
    token: &SecretString,
) -> Result<(), CliError> {
    validate_prefix(token.expose_secret())?;
    match ws.token_source {
        TokenSource::Env => Ok(()),
        TokenSource::File => {
            #[cfg(target_os = "windows")]
            {
                let _ = workspace_name;
                let _ = ws;
                Err(CliError::FileBackendUnsupported)
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = workspace_name;
                ws.token = Some(token.expose_secret().to_owned());
                Ok(())
            }
        }
        TokenSource::Keyring => keyring_save(workspace_name, token),
    }
}

pub fn delete(workspace_name: &str, ws: &mut WorkspaceConfig) -> Result<(), CliError> {
    match ws.token_source {
        TokenSource::Env => Ok(()),
        TokenSource::File => {
            let _ = workspace_name;
            ws.token = None;
            Ok(())
        }
        TokenSource::Keyring => keyring_delete(workspace_name),
    }
}

fn keyring_entry(workspace_name: &str) -> Result<keyring::Entry, CliError> {
    keyring::Entry::new("slack-cli", workspace_name)
        .map_err(|e| CliError::Config(format!("keyring init failed: {e}")))
}

fn keyring_load(workspace_name: &str) -> Result<SecretString, CliError> {
    let entry = keyring_entry(workspace_name)?;
    match entry.get_password() {
        Ok(t) => {
            validate_prefix(&t)?;
            Ok(SecretString::from(t))
        }
        Err(keyring::Error::NoEntry) => Err(CliError::NoToken {
            workspace: workspace_name.into(),
        }),
        Err(e) => {
            tracing::warn!(error = %e, "keyring load failed");
            Err(CliError::Config(format!("keyring error: {e}")))
        }
    }
}

fn keyring_save(workspace_name: &str, token: &SecretString) -> Result<(), CliError> {
    keyring_entry(workspace_name)?
        .set_password(token.expose_secret())
        .map_err(|e| CliError::Config(format!("keyring error: {e}")))
}

fn keyring_delete(workspace_name: &str) -> Result<(), CliError> {
    match keyring_entry(workspace_name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(CliError::Config(format!("keyring error: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TokenSource, WorkspaceConfig, WorkspaceMeta};

    // Only the file-backend test currently consumes this; gate it the same
    // way to avoid an unused-helper lint on Windows builds.
    #[cfg(not(target_os = "windows"))]
    fn ws(src: TokenSource) -> WorkspaceConfig {
        WorkspaceConfig {
            team_id: None,
            team_domain: None,
            token_source: src,
            token: None,
            token_env: None,
            meta: WorkspaceMeta::default(),
        }
    }

    #[test]
    fn validate_accepts_xoxp_and_xoxb() {
        assert!(validate_prefix("xoxp-1-2-3").is_ok());
        assert!(validate_prefix("xoxb-1-2-3").is_ok());
    }

    #[test]
    fn validate_rejects_garbage() {
        assert!(validate_prefix("hello").is_err());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn file_save_then_load() {
        let mut w = ws(TokenSource::File);
        let secret = SecretString::from("xoxp-1-2-3".to_owned());
        save("acme", &mut w, &secret).unwrap();
        let loaded = load("acme", &w).unwrap();
        assert_eq!(loaded.expose_secret(), "xoxp-1-2-3");
    }
}
