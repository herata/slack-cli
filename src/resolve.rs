//! Name → ID resolution with a 60 s in-process cache.
//!
//! Slack channels and users are addressed by opaque IDs (`Cxxx` / `Uxxx`),
//! but the CLI lets humans type `#general` or `@alice`. `Resolver` keeps a
//! per-process cache so repeated lookups in a single invocation don't
//! re-page through `conversations.list` / `users.list`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::{CliError, ResourceKind};
use crate::id::{ChannelId, ChannelRef, UserId, UserRef};
use crate::slack::SlackClient;

const TTL: Duration = Duration::from_secs(60);
const PAGE_LIMIT: u32 = 200;

#[derive(Debug, Default)]
pub struct Resolver {
    channels: HashMap<String, (ChannelId, Instant)>,
    users: HashMap<String, (UserId, Instant)>,
}

impl Resolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn channel(
        &mut self,
        client: &SlackClient,
        r: &ChannelRef,
    ) -> Result<ChannelId, CliError> {
        match r {
            ChannelRef::Id(id) => Ok(id.clone()),
            ChannelRef::Name(name) => {
                if let Some((id, ts)) = self.channels.get(name) {
                    if ts.elapsed() < TTL {
                        return Ok(id.clone());
                    }
                }
                let id = lookup_channel(client, name).await?;
                self.channels
                    .insert(name.clone(), (id.clone(), Instant::now()));
                Ok(id)
            }
        }
    }

    pub async fn user(&mut self, client: &SlackClient, r: &UserRef) -> Result<UserId, CliError> {
        match r {
            UserRef::Id(id) => Ok(id.clone()),
            UserRef::Name(name) => {
                if let Some((id, ts)) = self.users.get(name) {
                    if ts.elapsed() < TTL {
                        return Ok(id.clone());
                    }
                }
                let id = lookup_user(client, name).await?;
                self.users
                    .insert(name.clone(), (id.clone(), Instant::now()));
                Ok(id)
            }
        }
    }
}

async fn lookup_channel(client: &SlackClient, name: &str) -> Result<ChannelId, CliError> {
    let mut cursor: Option<String> = None;
    let mut matches: Vec<(String, bool)> = Vec::new();
    loop {
        let page = client
            .conversations_list(
                "public_channel,private_channel",
                PAGE_LIMIT,
                cursor.as_deref(),
            )
            .await?;
        for ch in &page.channels {
            if ch.name == name {
                matches.push((ch.id.clone(), ch.is_private));
            }
        }
        cursor = page
            .response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|s| !s.is_empty());
        if cursor.is_none() {
            break;
        }
    }
    match matches.len() {
        0 => Err(CliError::NotFound {
            kind: ResourceKind::Channel,
            name: name.into(),
        }),
        1 => Ok(ChannelId(matches.remove(0).0)),
        _ => Err(CliError::Ambiguous {
            kind: ResourceKind::Channel,
            name: name.into(),
            candidates: matches
                .into_iter()
                .map(|(id, is_private)| {
                    format!("{id} ({})", if is_private { "private" } else { "public" })
                })
                .collect(),
        }),
    }
}

async fn lookup_user(client: &SlackClient, name: &str) -> Result<UserId, CliError> {
    let mut cursor: Option<String> = None;
    let mut matches: Vec<String> = Vec::new();
    loop {
        let page = client.users_list(PAGE_LIMIT, cursor.as_deref()).await?;
        for u in &page.members {
            if u.name == name && !u.deleted {
                matches.push(u.id.clone());
            }
        }
        cursor = page
            .response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|s| !s.is_empty());
        if cursor.is_none() {
            break;
        }
    }
    match matches.len() {
        0 => Err(CliError::NotFound {
            kind: ResourceKind::User,
            name: name.into(),
        }),
        1 => Ok(UserId(matches.remove(0))),
        _ => Err(CliError::Ambiguous {
            kind: ResourceKind::User,
            name: name.into(),
            candidates: matches,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test(flavor = "current_thread")]
    async fn channel_id_passthrough_no_http_call() {
        // No mock mounted; if HTTP fires, the test fails on connection refused.
        let server = MockServer::start().await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r
            .channel(&c, &ChannelRef::Id(ChannelId("C0".into())))
            .await
            .unwrap();
        assert_eq!(id.as_str(), "C0");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn channel_name_resolves_via_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"channels":[{"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}],"response_metadata":{"next_cursor":""}}"#))
            .mount(&server).await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r
            .channel(&c, &ChannelRef::Name("general".into()))
            .await
            .unwrap();
        assert_eq!(id.as_str(), "C1");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn channel_name_cache_hit_skips_http() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"channels":[{"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}],"response_metadata":{"next_cursor":""}}"#))
            .expect(1)
            .mount(&server).await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id1 = r
            .channel(&c, &ChannelRef::Name("general".into()))
            .await
            .unwrap();
        let id2 = r
            .channel(&c, &ChannelRef::Name("general".into()))
            .await
            .unwrap();
        assert_eq!(id1.as_str(), id2.as_str());
        // The Mock::expect(1) assertion verifies cache hit; verified on drop.
    }

    #[tokio::test(flavor = "current_thread")]
    async fn channel_name_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"channels":[],"response_metadata":{"next_cursor":""}}"#,
            ))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let err = r
            .channel(&c, &ChannelRef::Name("nope".into()))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            CliError::NotFound {
                kind: ResourceKind::Channel,
                ..
            }
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn channel_name_ambiguous() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/conversations.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"channels":[
                    {"id":"C1","name":"general","is_private":false,"is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}},
                    {"id":"C2","name":"general","is_private":true, "is_archived":false,"num_members":1,"topic":{"value":""},"purpose":{"value":""}}
                ],"response_metadata":{"next_cursor":""}}"#))
            .mount(&server).await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let err = r
            .channel(&c, &ChannelRef::Name("general".into()))
            .await
            .unwrap_err();
        match err {
            CliError::Ambiguous {
                kind, candidates, ..
            } => {
                assert_eq!(kind, ResourceKind::Channel);
                assert_eq!(candidates.len(), 2);
            }
            _ => panic!("expected Ambiguous"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_id_passthrough_no_http_call() {
        let server = MockServer::start().await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r.user(&c, &UserRef::Id(UserId("U0".into()))).await.unwrap();
        assert_eq!(id.as_str(), "U0");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_name_resolves_via_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/users.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"members":[{"id":"U1","name":"alice","real_name":"A","deleted":false,"is_bot":false}],"response_metadata":{"next_cursor":""}}"#))
            .mount(&server).await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r.user(&c, &UserRef::Name("alice".into())).await.unwrap();
        assert_eq!(id.as_str(), "U1");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn user_name_skips_deleted() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/users.list"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"ok":true,"members":[
                    {"id":"U1","name":"alice","real_name":"A","deleted":true, "is_bot":false},
                    {"id":"U2","name":"alice","real_name":"A","deleted":false,"is_bot":false}
                ],"response_metadata":{"next_cursor":""}}"#,
            ))
            .mount(&server)
            .await;
        let c = SlackClient::with_base_url(server.uri(), SecretString::from("xoxp-test")).unwrap();
        let mut r = Resolver::new();
        let id = r.user(&c, &UserRef::Name("alice".into())).await.unwrap();
        assert_eq!(id.as_str(), "U2");
    }
}
