mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn search_messages_emits_items() {
    let sb = common::sandbox("acme").await;
    Mock::given(method("GET"))
        .and(path("/api/search.messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"messages":{"matches":[{"ts":"1700000000.000100","user":"U1","text":"hello"}],"paging":{"page":1,"pages":1}}}"#))
        .mount(&sb.server)
        .await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["search", "messages", "hello", "--json"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"items\""), "stdout: {out}");
    assert!(out.contains("\"text\":\"hello\""), "stdout: {out}");
}
