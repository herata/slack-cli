mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn send_dry_run_makes_no_post() {
    let sb = common::sandbox("acme").await;
    // chat.postMessage must NEVER fire.
    Mock::given(method("POST"))
        .and(path("/api/chat.postMessage"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&sb.server)
        .await;
    // conversations.list (channel-name resolution) is OK to be called once.
    Mock::given(method("GET"))
        .and(path("/api/conversations.list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"channels":[{"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}],"response_metadata":{"next_cursor":""}}"#))
        .mount(&sb.server)
        .await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args([
            "message",
            "send",
            "#general",
            "--text",
            "hi",
            "--dry-run",
            "--json",
        ])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"dry_run\":true"), "stdout: {out}");
    assert!(
        out.contains("\"would_call\":\"chat.postMessage\""),
        "stdout: {out}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_happy_path() {
    let sb = common::sandbox("acme").await;
    Mock::given(method("GET"))
        .and(path("/api/conversations.list"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"channels":[{"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}],"response_metadata":{"next_cursor":""}}"#))
        .mount(&sb.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/chat.postMessage"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"ok":true,"ts":"1700000000.000100","channel":"C1","message":{"ts":"1700000000.000100","user":"U1","text":"hi"}}"#))
        .mount(&sb.server)
        .await;
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args(["message", "send", "#general", "--text", "hi", "--json"])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        out.contains("\"ts\":\"1700000000.000100\""),
        "stdout: {out}"
    );
}
