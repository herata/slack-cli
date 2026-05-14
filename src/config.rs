//! Configuration file I/O.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use etcetera::{choose_base_strategy, BaseStrategy};
use serde::{Deserialize, Serialize};

use crate::error::CliError;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub default_workspace: Option<String>,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub utc: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            color: default_color(),
            utc: false,
        }
    }
}

fn default_format() -> String {
    "auto".into()
}
fn default_color() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceConfig {
    pub team_id: Option<String>,
    pub team_domain: Option<String>,
    pub token_source: TokenSource,
    pub token: Option<String>,
    pub token_env: Option<String>,
    #[serde(default)]
    pub meta: WorkspaceMeta,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TokenSource {
    Keyring,
    File,
    Env,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorkspaceMeta {
    pub token_kind: Option<String>,
    pub user_id: Option<String>,
    pub updated_at: Option<String>,
}

pub fn resolve_path(cli: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(p) = cli {
        return Ok(p.to_path_buf());
    }
    if let Ok(envp) = std::env::var("SLACK_CLI_CONFIG") {
        return Ok(PathBuf::from(envp));
    }
    let strat = choose_base_strategy().map_err(|e| CliError::Config(e.to_string()))?;
    Ok(strat.config_dir().join("slack-cli").join("config.toml"))
}

pub fn load(path: &Path) -> Result<Config, CliError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e.into()),
    };
    toml_edit::de::from_str(&raw).map_err(|e| CliError::Config(e.to_string()))
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), CliError> {
    let parent = path
        .parent()
        .ok_or_else(|| CliError::Config("path has no parent".into()))?;
    std::fs::create_dir_all(parent)?;

    let serialized =
        toml_edit::ser::to_string_pretty(cfg).map_err(|e| CliError::Config(e.to_string()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(serialized.as_bytes())?;
    tmp.as_file().sync_all()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))?;
    }

    tmp.persist(path).map_err(|e| CliError::Io(e.error))?;
    Ok(())
}

/// Pick the workspace to use given CLI/env/default precedence.
pub fn pick_workspace<'a>(
    cfg: &'a Config,
    cli: Option<&str>,
) -> Result<(&'a str, &'a WorkspaceConfig), CliError> {
    let env_ws = std::env::var("SLACK_CLI_WORKSPACE").ok();
    let name = cli
        .map(str::to_owned)
        .or(env_ws)
        .or_else(|| cfg.default_workspace.clone())
        .or_else(|| {
            if cfg.workspaces.len() == 1 {
                cfg.workspaces.keys().next().cloned()
            } else {
                None
            }
        });

    let name = name.ok_or_else(|| CliError::AmbiguousWorkspace {
        available: cfg.workspaces.keys().cloned().collect(),
    })?;
    let (key, ws) =
        cfg.workspaces
            .get_key_value(&name)
            .ok_or_else(|| CliError::AmbiguousWorkspace {
                available: cfg.workspaces.keys().cloned().collect(),
            })?;
    Ok((key.as_str(), ws))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let cfg = load(&p).unwrap();
        assert!(cfg.default_workspace.is_none());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a").join("b").join("config.toml");
        let mut cfg = Config {
            default_workspace: Some("acme".into()),
            ..Default::default()
        };
        cfg.workspaces.insert(
            "acme".into(),
            WorkspaceConfig {
                team_id: Some("T1".into()),
                team_domain: Some("acme".into()),
                token_source: TokenSource::Keyring,
                token: None,
                token_env: None,
                meta: WorkspaceMeta::default(),
            },
        );
        save(&p, &cfg).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.default_workspace.as_deref(), Some("acme"));
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        save(&p, &Config::default()).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn pick_workspace_uses_default() {
        let mut cfg = Config {
            default_workspace: Some("acme".into()),
            ..Default::default()
        };
        cfg.workspaces.insert(
            "acme".into(),
            WorkspaceConfig {
                team_id: None,
                team_domain: None,
                token_source: TokenSource::Keyring,
                token: None,
                token_env: None,
                meta: WorkspaceMeta::default(),
            },
        );
        let (name, _) = pick_workspace(&cfg, None).unwrap();
        assert_eq!(name, "acme");
    }
}
