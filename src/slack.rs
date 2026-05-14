//! Thin Slack Web API client. No SDK; direct `reqwest` calls.
//!
//! The client exposes one method per Slack API endpoint we use. Errors are
//! normalized into `CliError` variants so callers never see raw Slack codes.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{CliError, ResourceKind};

const DEFAULT_BASE_URL: &str = "https://slack.com";

/// HTTP client for the Slack Web API.
///
/// All public methods deserialize the response with `parse_slack`, which
/// translates `ok: false` payloads into `CliError` variants.
#[derive(Debug)]
pub struct SlackClient {
    pub(crate) base_url: String,
    http: reqwest::Client,
}

impl SlackClient {
    /// Create a client targeting `https://slack.com`. Honors the
    /// `SLACK_CLI_BASE_URL_OVERRIDE` env var so integration tests can swap
    /// the target without recompiling.
    pub fn new(token: SecretString) -> Result<Self, CliError> {
        let base = std::env::var("SLACK_CLI_BASE_URL_OVERRIDE")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        Self::build(base, token)
    }

    /// Create a client targeting an explicit base URL (used by tests).
    pub fn with_base_url(base_url: String, token: SecretString) -> Result<Self, CliError> {
        Self::build(base_url, token)
    }

    fn build(base_url: String, token: SecretString) -> Result<Self, CliError> {
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
            .map_err(|e| CliError::Config(format!("bad token: {e}")))?;
        auth.set_sensitive(true);
        // `HeaderValue` owns its bytes, so the `SecretString` can drop here
        // and `Authorization` continues to flow with every request.
        drop(token);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(concat!("slack-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CliError::Network(e.to_string()))?;
        Ok(Self { base_url, http })
    }

    /// Slack `auth.test`. Returns the authenticated team / user / bot info.
    #[instrument(skip(self))]
    pub async fn auth_test(&self) -> Result<AuthTest, CliError> {
        let url = format!("{}/api/auth.test", self.base_url);
        let resp = self.send_with_retry(self.http.post(&url)).await?;
        parse_slack::<AuthTest>(resp).await
    }

    /// Send a built request with at most one retry on HTTP 429. The
    /// `Retry-After` value is clamped to 60 s; no exponential backoff
    /// (caller can re-invoke). The body must be clonable via
    /// `RequestBuilder::try_clone` (i.e., in-memory, not a stream).
    async fn send_with_retry(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, CliError> {
        let clone = req
            .try_clone()
            .expect("RequestBuilder is cloneable when body is in-memory");
        let first = req
            .send()
            .await
            .map_err(|e| CliError::Network(e.to_string()))?;
        if first.status().as_u16() != 429 {
            return Ok(first);
        }
        let wait = first
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1)
            .min(60);
        tokio::time::sleep(Duration::from_secs(wait)).await;
        clone
            .send()
            .await
            .map_err(|e| CliError::Network(e.to_string()))
    }

    /// Slack `conversations.list`. Paginated; pass the returned cursor to
    /// continue. `types` is the comma-separated channel types filter.
    #[instrument(skip(self))]
    pub async fn conversations_list(
        &self,
        types: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ConversationsList, CliError> {
        let mut url = format!(
            "{}/api/conversations.list?types={}&limit={}",
            self.base_url,
            urlencoding::encode(types),
            limit,
        );
        if let Some(c) = cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding::encode(c));
        }
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<ConversationsList>(resp).await
    }

    /// Slack `conversations.info`. Returns metadata for one channel.
    #[instrument(skip(self))]
    pub async fn conversations_info(&self, channel: &str) -> Result<ConversationsInfo, CliError> {
        let url = format!(
            "{}/api/conversations.info?channel={}",
            self.base_url,
            urlencoding::encode(channel),
        );
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<ConversationsInfo>(resp).await
    }

    /// Slack `conversations.history`. `oldest` / `latest` are Slack ts values
    /// bounding the message range. Paginated via cursor.
    #[instrument(skip(self))]
    pub async fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        cursor: Option<&str>,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> Result<History, CliError> {
        let mut url = format!(
            "{}/api/conversations.history?channel={}&limit={}",
            self.base_url,
            urlencoding::encode(channel),
            limit,
        );
        if let Some(c) = cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding::encode(c));
        }
        if let Some(t) = oldest {
            url.push_str("&oldest=");
            url.push_str(&urlencoding::encode(t));
        }
        if let Some(t) = latest {
            url.push_str("&latest=");
            url.push_str(&urlencoding::encode(t));
        }
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<History>(resp).await
    }

    /// Slack `conversations.replies`. Returns the parent plus replies for one
    /// thread, in order. Paginated via cursor.
    #[instrument(skip(self))]
    pub async fn conversations_replies(
        &self,
        channel: &str,
        ts: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<History, CliError> {
        let mut url = format!(
            "{}/api/conversations.replies?channel={}&ts={}&limit={}",
            self.base_url,
            urlencoding::encode(channel),
            urlencoding::encode(ts),
            limit,
        );
        if let Some(c) = cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding::encode(c));
        }
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<History>(resp).await
    }

    /// Slack `users.list`. Paginated via cursor.
    #[instrument(skip(self))]
    pub async fn users_list(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<UsersList, CliError> {
        let mut url = format!("{}/api/users.list?limit={}", self.base_url, limit,);
        if let Some(c) = cursor {
            url.push_str("&cursor=");
            url.push_str(&urlencoding::encode(c));
        }
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<UsersList>(resp).await
    }

    /// Slack `users.info`. Returns one user's profile.
    #[instrument(skip(self))]
    pub async fn users_info(&self, user: &str) -> Result<UsersInfo, CliError> {
        let url = format!(
            "{}/api/users.info?user={}",
            self.base_url,
            urlencoding::encode(user),
        );
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<UsersInfo>(resp).await
    }

    /// Slack `search.messages`. User-token required. Page is 1-indexed.
    #[instrument(skip(self))]
    pub async fn search_messages(
        &self,
        query: &str,
        page: u32,
        count: u32,
    ) -> Result<SearchMessagesResp, CliError> {
        let url = format!(
            "{}/api/search.messages?query={}&page={}&count={}",
            self.base_url,
            urlencoding::encode(query),
            page,
            count,
        );
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<SearchMessagesResp>(resp).await
    }

    /// Slack `search.files`. User-token required. Page is 1-indexed.
    #[instrument(skip(self))]
    pub async fn search_files(
        &self,
        query: &str,
        page: u32,
        count: u32,
    ) -> Result<SearchFilesResp, CliError> {
        let url = format!(
            "{}/api/search.files?query={}&page={}&count={}",
            self.base_url,
            urlencoding::encode(query),
            page,
            count,
        );
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<SearchFilesResp>(resp).await
    }

    /// Slack `files.list`. Optional channel filter. Page is 1-indexed.
    #[instrument(skip(self))]
    pub async fn files_list(
        &self,
        channel: Option<&str>,
        count: u32,
        page: Option<u32>,
    ) -> Result<FilesList, CliError> {
        let mut url = format!("{}/api/files.list?count={}", self.base_url, count,);
        if let Some(c) = channel {
            url.push_str("&channel=");
            url.push_str(&urlencoding::encode(c));
        }
        if let Some(p) = page {
            use std::fmt::Write as _;
            let _ = write!(url, "&page={p}");
        }
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<FilesList>(resp).await
    }

    /// Slack `files.info`. Returns one file's metadata.
    #[instrument(skip(self))]
    pub async fn files_info(&self, file_id: &str) -> Result<FilesInfo, CliError> {
        let url = format!(
            "{}/api/files.info?file={}",
            self.base_url,
            urlencoding::encode(file_id),
        );
        let resp = self.send_with_retry(self.http.get(&url)).await?;
        parse_slack::<FilesInfo>(resp).await
    }

    /// Slack `chat.postMessage`. The request carries a UUID v4 `client_msg_id`
    /// so retries are de-duplicated server-side.
    #[instrument(skip(self, req))]
    pub async fn chat_post_message(&self, req: &PostMessage) -> Result<PostMessageResp, CliError> {
        let url = format!("{}/api/chat.postMessage", self.base_url);
        let resp = self.send_with_retry(self.http.post(&url).json(req)).await?;
        parse_slack::<PostMessageResp>(resp).await
    }

    /// Slack `chat.update`. Replaces the text of a previously posted message.
    #[instrument(skip(self))]
    pub async fn chat_update(
        &self,
        channel: &str,
        ts: &str,
        text: &str,
    ) -> Result<ChatTsResp, CliError> {
        let url = format!("{}/api/chat.update", self.base_url);
        let body = serde_json::json!({"channel": channel, "ts": ts, "text": text});
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        parse_slack::<ChatTsResp>(resp).await
    }

    /// Slack `chat.delete`. Removes a previously posted message.
    #[instrument(skip(self))]
    pub async fn chat_delete(&self, channel: &str, ts: &str) -> Result<ChatTsResp, CliError> {
        let url = format!("{}/api/chat.delete", self.base_url);
        let body = serde_json::json!({"channel": channel, "ts": ts});
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        parse_slack::<ChatTsResp>(resp).await
    }

    /// Slack `reactions.add`. Idempotent: `already_reacted` is treated as Ok.
    #[instrument(skip(self))]
    pub async fn reactions_add(&self, channel: &str, ts: &str, name: &str) -> Result<(), CliError> {
        self.reaction_call("/api/reactions.add", channel, ts, name, "already_reacted")
            .await
    }

    /// Slack `reactions.remove`. Idempotent: `no_reaction` is treated as Ok.
    #[instrument(skip(self))]
    pub async fn reactions_remove(
        &self,
        channel: &str,
        ts: &str,
        name: &str,
    ) -> Result<(), CliError> {
        self.reaction_call("/api/reactions.remove", channel, ts, name, "no_reaction")
            .await
    }

    /// Shared reaction-call body. The `idempotent_error` code (e.g.
    /// `already_reacted`, `no_reaction`) is downgraded to success so callers
    /// can retry safely.
    async fn reaction_call(
        &self,
        endpoint: &str,
        channel: &str,
        ts: &str,
        name: &str,
        idempotent_error: &str,
    ) -> Result<(), CliError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let body = serde_json::json!({"channel": channel, "timestamp": ts, "name": name});
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        let status = resp.status();
        let request_id = resp
            .headers()
            .get("x-slack-req-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let raw = resp
            .text()
            .await
            .map_err(|e| CliError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(CliError::SlackApi {
                code: format!("http_{}", status.as_u16()),
                request_id,
            });
        }
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| CliError::SlackApi {
            code: format!("decode: {e}"),
            request_id: request_id.clone(),
        })?;
        if v.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            return Ok(());
        }
        let code = v.get("error").and_then(|x| x.as_str()).unwrap_or("unknown");
        if code == idempotent_error {
            return Ok(());
        }
        Err(map_slack_error(code, request_id))
    }

    /// Slack `conversations.create`. Returns the freshly created channel.
    #[instrument(skip(self))]
    pub async fn conversations_create(
        &self,
        name: &str,
        is_private: bool,
        description: Option<&str>,
    ) -> Result<ConversationsCreateResp, CliError> {
        let url = format!("{}/api/conversations.create", self.base_url);
        let mut body = serde_json::json!({"name": name, "is_private": is_private});
        if let Some(d) = description {
            body["description"] = serde_json::Value::String(d.to_owned());
        }
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        parse_slack::<ConversationsCreateResp>(resp).await
    }

    /// Slack `conversations.archive`.
    #[instrument(skip(self))]
    pub async fn conversations_archive(&self, channel: &str) -> Result<(), CliError> {
        let url = format!("{}/api/conversations.archive", self.base_url);
        let body = serde_json::json!({"channel": channel});
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        parse_slack_unit(resp).await
    }

    /// Slack `conversations.invite`. `users` is joined into a comma-separated
    /// list per the canonical wire form Slack documents.
    #[instrument(skip(self))]
    pub async fn conversations_invite(
        &self,
        channel: &str,
        users: &[String],
    ) -> Result<ConversationsCreateResp, CliError> {
        let url = format!("{}/api/conversations.invite", self.base_url);
        let body = serde_json::json!({"channel": channel, "users": users.join(",")});
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        parse_slack::<ConversationsCreateResp>(resp).await
    }

    /// Slack `conversations.leave`.
    #[instrument(skip(self))]
    pub async fn conversations_leave(&self, channel: &str) -> Result<(), CliError> {
        let url = format!("{}/api/conversations.leave", self.base_url);
        let body = serde_json::json!({"channel": channel});
        let resp = self
            .send_with_retry(self.http.post(&url).json(&body))
            .await?;
        parse_slack_unit(resp).await
    }

    /// Slack `files.upload`. Multipart-encoded; `bytes` is the full file body
    /// (callers typically pass `std::fs::read(path)?` or a stdin buffer).
    #[instrument(skip(self, bytes))]
    pub async fn files_upload(
        &self,
        channel: Option<&str>,
        filename: &str,
        bytes: Vec<u8>,
        title: Option<&str>,
        initial_comment: Option<&str>,
        thread_ts: Option<&str>,
    ) -> Result<FileDto, CliError> {
        let url = format!("{}/api/files.upload", self.base_url);
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename.to_owned());
        let mut form = reqwest::multipart::Form::new().part("file", part);
        if let Some(c) = channel {
            form = form.text("channels", c.to_owned());
        }
        if let Some(t) = title {
            form = form.text("title", t.to_owned());
        }
        if let Some(c) = initial_comment {
            form = form.text("initial_comment", c.to_owned());
        }
        if let Some(t) = thread_ts {
            form = form.text("thread_ts", t.to_owned());
        }
        // `reqwest::multipart::Form` cannot be cloned, so `RequestBuilder::try_clone`
        // returns `None`. We deliberately bypass `send_with_retry` here; on 429
        // the caller will see a `SlackApi { code: "http_429", .. }` and can
        // re-invoke. Wiring it through `send_with_retry` would panic on the
        // `expect("...cloneable...")` retry path.
        let resp = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| CliError::Network(e.to_string()))?;
        let parsed: FilesUploadResp = parse_slack(resp).await?;
        Ok(parsed.file)
    }

    /// Download a Slack-hosted file. The URL must already include the host
    /// (e.g., `https://files.slack.com/...`); the default `Authorization`
    /// header attached to the client is reused. Binary body — bypasses
    /// `parse_slack`.
    #[instrument(skip(self))]
    pub async fn download_authorized(&self, url: &str) -> Result<Vec<u8>, CliError> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| CliError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(CliError::SlackApi {
                code: format!("http_{}", resp.status().as_u16()),
                request_id: None,
            });
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CliError::Network(e.to_string()))?;
        Ok(bytes.to_vec())
    }
}

/// Response shape for `auth.test`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthTest {
    pub ok: bool,
    pub url: Option<String>,
    pub team: Option<String>,
    pub user: Option<String>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub bot_id: Option<String>,
    pub error: Option<String>,
}

/// Topic / purpose container used by `ChannelDto`. Slack returns a small
/// nested object with `value`, `creator`, `last_set`; only `value` is needed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TopicPurpose {
    pub value: String,
}

/// Channel metadata, common to `conversations.list` and `conversations.info`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_archived: bool,
    pub num_members: Option<u32>,
    pub topic: Option<TopicPurpose>,
    pub purpose: Option<TopicPurpose>,
}

/// Message metadata returned by history, replies, and search.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageDto {
    pub ts: String,
    pub user: Option<String>,
    #[serde(default)]
    pub text: String,
    pub thread_ts: Option<String>,
    pub reply_count: Option<u32>,
}

/// User profile metadata returned by `users.list` and `users.info`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserDto {
    pub id: String,
    pub name: String,
    pub real_name: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub is_bot: bool,
}

/// File metadata returned by `files.list`, `files.info`, and `search.files`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mimetype: String,
    pub user: Option<String>,
    pub url_private: Option<String>,
}

/// Slack cursor-pagination metadata block.
#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMetadata {
    pub next_cursor: Option<String>,
}

/// Response shape for `conversations.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConversationsList {
    pub ok: bool,
    pub channels: Vec<ChannelDto>,
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response shape for `conversations.info`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConversationsInfo {
    pub ok: bool,
    pub channel: ChannelDto,
}

/// Response shape shared by `conversations.history` and `conversations.replies`.
#[derive(Debug, Clone, Deserialize)]
pub struct History {
    pub ok: bool,
    pub messages: Vec<MessageDto>,
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response shape for `users.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct UsersList {
    pub ok: bool,
    pub members: Vec<UserDto>,
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response shape for `users.info`.
#[derive(Debug, Clone, Deserialize)]
pub struct UsersInfo {
    pub ok: bool,
    pub user: UserDto,
}

/// Page-style pagination block used by `search.*` and `files.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchPaging {
    pub page: u32,
    pub pages: u32,
}

/// Inner `messages` payload of `search.messages`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchMessagesPayload {
    pub matches: Vec<MessageDto>,
    pub paging: SearchPaging,
}

/// Response shape for `search.messages`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchMessagesResp {
    pub ok: bool,
    pub messages: SearchMessagesPayload,
}

/// Inner `files` payload of `search.files`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchFilesPayload {
    pub matches: Vec<FileDto>,
    pub paging: SearchPaging,
}

/// Response shape for `search.files`.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchFilesResp {
    pub ok: bool,
    pub files: SearchFilesPayload,
}

/// Response shape for `files.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct FilesList {
    pub ok: bool,
    pub files: Vec<FileDto>,
    pub paging: Option<SearchPaging>,
}

/// Response shape for `files.info`.
#[derive(Debug, Clone, Deserialize)]
pub struct FilesInfo {
    pub ok: bool,
    pub file: FileDto,
}

/// Request body for `chat.postMessage`. `client_msg_id` is auto-populated with
/// a UUID v4 by `PostMessage::new` so retries are de-duplicated server-side.
#[derive(Debug, Clone, Serialize)]
pub struct PostMessage {
    pub channel: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_broadcast: Option<bool>,
    pub client_msg_id: String,
}

impl PostMessage {
    /// Build a fresh request with a random UUID v4 `client_msg_id`.
    #[must_use]
    pub fn new(channel: String, text: String) -> Self {
        Self {
            channel,
            text,
            thread_ts: None,
            reply_broadcast: None,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

/// Response shape for `chat.postMessage`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PostMessageResp {
    pub ok: bool,
    pub ts: String,
    pub channel: String,
    pub message: MessageDto,
}

/// Response shape shared by `chat.update` and `chat.delete`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatTsResp {
    pub ok: bool,
    pub channel: String,
    pub ts: String,
}

/// Response shape for `conversations.create` and `conversations.invite`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationsCreateResp {
    pub ok: bool,
    pub channel: ChannelDto,
}

/// Minimal `{"ok": true}` response used by archive / leave.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OkResp {
    pub ok: bool,
}

/// Response shape for `files.upload`.
#[derive(Debug, Clone, Deserialize)]
pub struct FilesUploadResp {
    pub ok: bool,
    pub file: FileDto,
}

/// Decode a Slack response whose body we only need to validate as
/// `{"ok": true}`. Discards the parsed payload.
pub(crate) async fn parse_slack_unit(resp: reqwest::Response) -> Result<(), CliError> {
    let _: OkResp = parse_slack(resp).await?;
    Ok(())
}

/// Decode a Slack response, mapping `ok: false` to `CliError`.
pub(crate) async fn parse_slack<T: for<'de> Deserialize<'de>>(
    resp: reqwest::Response,
) -> Result<T, CliError> {
    let request_id = resp
        .headers()
        .get("x-slack-req-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| CliError::Network(e.to_string()))?;

    if !status.is_success() {
        return Err(CliError::SlackApi {
            code: format!("http_{}", status.as_u16()),
            request_id,
        });
    }

    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| CliError::SlackApi {
        code: format!("decode: {e}"),
        request_id: request_id.clone(),
    })?;

    if v.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let code = v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_owned();
        return Err(map_slack_error(&code, request_id));
    }

    serde_json::from_value(v).map_err(|e| CliError::SlackApi {
        code: format!("decode: {e}"),
        request_id,
    })
}

/// Translate a Slack `error` code into a `CliError` variant.
pub(crate) fn map_slack_error(code: &str, request_id: Option<String>) -> CliError {
    match code {
        "not_authed" | "invalid_auth" | "account_inactive" | "token_revoked" | "token_expired" => {
            CliError::AuthFailed {
                reason: code.to_owned(),
            }
        }
        "channel_not_found" => CliError::NotFound {
            kind: ResourceKind::Channel,
            name: "<remote>".into(),
        },
        "user_not_found" => CliError::NotFound {
            kind: ResourceKind::User,
            name: "<remote>".into(),
        },
        "file_not_found" => CliError::NotFound {
            kind: ResourceKind::File,
            name: "<remote>".into(),
        },
        "ratelimited" => CliError::RateLimited {
            retry_after_secs: 1,
        },
        "missing_scope" | "not_allowed_token_type" => CliError::TokenKindMismatch {
            needed: "?",
            actual: "?",
            api: "?",
        },
        _ => CliError::SlackApi {
            code: code.to_owned(),
            request_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test(flavor = "current_thread")]
    async fn auth_test_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .and(header("authorization", "Bearer xoxp-test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"team":"acme","user":"alice","team_id":"T1","user_id":"U1"}"#,
            ))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let r = c.auth_test().await.unwrap();
        assert_eq!(r.team_id.as_deref(), Some("T1"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auth_test_invalid_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"ok":false,"error":"invalid_auth"}"#),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let err = c.auth_test().await.unwrap_err();
        assert!(matches!(err, CliError::AuthFailed { .. }));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn override_via_env() {
        // SAFETY: env mutations in tests can interfere with other tests, but
        // we run on a current_thread runtime which serializes them with the
        // outer test framework.
        std::env::set_var("SLACK_CLI_BASE_URL_OVERRIDE", "http://127.0.0.1:1");
        let c = SlackClient::new(SecretString::from("xoxp-test")).unwrap();
        assert!(c.base_url.starts_with("http://127.0.0.1:1"));
        std::env::remove_var("SLACK_CLI_BASE_URL_OVERRIDE");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retry_once_after_429() {
        let server = MockServer::start().await;
        let body_ok = r#"{"ok":true,"team":"acme","team_id":"T1","user_id":"U1"}"#;

        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/auth.test"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body_ok))
            .mount(&server)
            .await;

        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let r = c.auth_test().await.unwrap();
        assert_eq!(r.team_id.as_deref(), Some("T1"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_list_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.list"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/conversations.list.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c
            .conversations_list("public_channel,private_channel", 50, None)
            .await
            .unwrap();
        assert_eq!(res.channels.len(), 1);
        assert_eq!(res.channels[0].name, "general");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_info_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.info"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/conversations.info.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.conversations_info("C001").await.unwrap();
        assert_eq!(res.channel.id, "C001");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_history_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.history"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/conversations.history.ok.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c
            .conversations_history("C001", 50, None, None, None)
            .await
            .unwrap();
        assert_eq!(res.messages.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_replies_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.replies"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/conversations.replies.ok.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c
            .conversations_replies("C001", "1700000000.000100", None, 50)
            .await
            .unwrap();
        assert_eq!(res.messages.len(), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn users_list_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/users.list"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/users.list.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.users_list(50, None).await.unwrap();
        assert_eq!(res.members[0].name, "alice");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn users_info_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/users.info"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/users.info.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.users_info("U001").await.unwrap();
        assert_eq!(res.user.id, "U001");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_messages_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search.messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/search.messages.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.search_messages("hi", 1, 50).await.unwrap();
        assert_eq!(res.messages.matches.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_files_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search.files"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/search.files.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.search_files("r.png", 1, 50).await.unwrap();
        assert_eq!(res.files.matches.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn files_list_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/files.list"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/files.list.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.files_list(None, 50, None).await.unwrap();
        assert_eq!(res.files[0].id, "F001");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn files_info_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/files.info"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/files.info.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c.files_info("F001").await.unwrap();
        assert_eq!(res.file.name, "r.png");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn chat_post_message_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat.postMessage"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/chat.postMessage.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let post_req = PostMessage::new("C001".into(), "hello".into());
        let posted = c.chat_post_message(&post_req).await.unwrap();
        assert_eq!(posted.ts, "1700000000.000100");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn post_message_has_uuid_client_msg_id() {
        let req = PostMessage::new("C001".into(), "x".into());
        // UUID v4 string format: 8-4-4-4-12 hex chars.
        assert_eq!(req.client_msg_id.len(), 36);
        assert!(req.client_msg_id.chars().filter(|c| *c == '-').count() == 4);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn chat_update_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat.update"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/chat.update.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c
            .chat_update("C001", "1700000000.000100", "edit")
            .await
            .unwrap();
        assert_eq!(res.ts, "1700000000.000100");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn chat_delete_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat.delete"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/chat.delete.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        c.chat_delete("C001", "1700000000.000100").await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reactions_add_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/reactions.add"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/reactions.add.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        c.reactions_add("C001", "1700000000.000100", "thumbsup")
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reactions_add_already_reacted_is_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/reactions.add"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/reactions.add.already_reacted.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        c.reactions_add("C001", "1700000000.000100", "thumbsup")
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_create_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/conversations.create"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/conversations.create.ok.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c
            .conversations_create("new-chan", false, None)
            .await
            .unwrap();
        assert_eq!(res.channel.id, "C999");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_archive_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/conversations.archive"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/conversations.archive.ok.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        c.conversations_archive("C001").await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_invite_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/conversations.invite"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/conversations.invite.ok.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let res = c
            .conversations_invite("C001", &["U002".into()])
            .await
            .unwrap();
        assert_eq!(res.channel.id, "C001");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conversations_leave_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/conversations.leave"))
            .respond_with(ResponseTemplate::new(200).set_body_string(include_str!(
                "../tests/fixtures/conversations.leave.ok.json"
            )))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        c.conversations_leave("C001").await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn files_upload_parses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/files.upload"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(include_str!("../tests/fixtures/files.upload.ok.json")),
            )
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let f = c
            .files_upload(Some("C001"), "r.png", vec![1, 2, 3], None, None, None)
            .await
            .unwrap();
        assert_eq!(f.id, "F999");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn download_authorized_returns_bytes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/files/r.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"\x89PNG\r\n".to_vec()))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let bytes = c
            .download_authorized(&format!("{}/files/r.png", server.uri()))
            .await
            .unwrap();
        assert_eq!(&bytes[..4], b"\x89PNG");
    }
}
