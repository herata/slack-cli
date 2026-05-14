#![allow(clippy::needless_return)]

mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn whoami_emits_envelope() {
    let sb = common::sandbox("acme").await;
    Mock::given(method("POST"))
        .and(path("/api/auth.test"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"team":"acme","user":"alice","team_id":"T1","user_id":"U1"}"#,
        ))
        .mount(&sb.server)
        .await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["auth", "whoami", "--json"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"schema\":\"slack-cli/v0\""), "stdout: {out}");
    assert!(out.contains("\"team_id\":\"T1\""), "stdout: {out}");
}

#[tokio::test(flavor = "current_thread")]
async fn auth_list_with_empty_config_is_ok() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, "").unwrap();

    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin("slack-cli"));
    cmd.env("SLACK_CLI_CONFIG", &config_path);
    cmd.env("NO_COLOR", "1");
    cmd.env("RUST_LOG", "error");
    cmd.args(["auth", "list", "--json"]);
    cmd.assert().success();
}
