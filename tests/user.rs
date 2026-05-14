mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn user_view_resolves_name() {
    let sb = common::sandbox("acme").await;
    Mock::given(method("GET"))
        .and(path("/api/users.list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"members":[{"id":"U1","name":"alice","real_name":"Alice","deleted":false,"is_bot":false}],"response_metadata":{"next_cursor":""}}"#))
        .mount(&sb.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/users.info"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"user":{"id":"U1","name":"alice","real_name":"Alice","deleted":false,"is_bot":false}}"#))
        .mount(&sb.server)
        .await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["user", "view", "@alice", "--json"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"id\":\"U1\""), "stdout: {out}");
}
