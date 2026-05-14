mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn channel_list_emits_items_envelope() {
    let sb = common::sandbox("acme").await;
    Mock::given(method("GET"))
        .and(path("/api/conversations.list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"channels":[{"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}],"response_metadata":{"next_cursor":""}}"#))
        .mount(&sb.server)
        .await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["channel", "list", "--json"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"items\""), "stdout: {out}");
    assert!(out.contains("\"name\":\"general\""), "stdout: {out}");
}

#[tokio::test(flavor = "current_thread")]
async fn channel_archive_without_yes_fails_64() {
    let sb = common::sandbox("acme").await;
    // No mock for /api/conversations.archive — request must NEVER fire.
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["channel", "archive", "C1"])
        .assert();
    assert.code(64);
}
