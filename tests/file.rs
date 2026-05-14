mod common;

use assert_cmd::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "current_thread")]
async fn file_upload_dry_run_makes_no_post() {
    let sb = common::sandbox("acme").await;
    // files.upload must NEVER fire.
    Mock::given(method("POST"))
        .and(path("/api/files.upload"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&sb.server)
        .await;
    // Write a tiny file we can reference by path.
    let path = sb.dir.path().join("hello.txt");
    std::fs::write(&path, b"hi").unwrap();
    let assert = common::cmd(&sb, "acme", "xoxp-test")
        .args([
            "file",
            "upload",
            path.to_str().unwrap(),
            "--dry-run",
            "--json",
        ])
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("\"dry_run\":true"), "stdout: {out}");
    assert!(
        out.contains("\"would_call\":\"files.upload\""),
        "stdout: {out}"
    );
}
