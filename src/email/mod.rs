// Email: Gmail API poller + triage engine
// MVP: poll every 60s, rule-based classification, apply Gmail labels

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::time::{self, Duration};
use tracing::{debug, error, info, warn};

const GMAIL_API: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Gmail poller that uses OAuth2 refresh tokens
pub struct GmailPoller {
    client: Client,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    access_token: Option<String>,
    poll_interval: Duration,
    /// label_id for "aide.sh" label
    aide_label_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct MessageList {
    messages: Option<Vec<MessageRef>>,
    #[serde(rename = "resultSizeEstimate")]
    #[allow(dead_code)]
    result_size_estimate: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MessageRef {
    id: String,
    #[allow(dead_code)]
    #[serde(rename = "threadId")]
    thread_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(rename = "labelIds")]
    pub label_ids: Option<Vec<String>>,
    pub payload: Option<Payload>,
    pub snippet: Option<String>,
    #[serde(rename = "internalDate")]
    pub internal_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Payload {
    pub headers: Option<Vec<Header>>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
struct LabelList {
    labels: Option<Vec<Label>>,
}

#[derive(Debug, Deserialize)]
struct Label {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct ModifyRequest {
    #[serde(rename = "addLabelIds")]
    add_label_ids: Vec<String>,
}

/// Summary of a polled email for triage
#[derive(Debug, Clone)]
pub struct EmailSummary {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub labels: Vec<String>,
}

impl GmailPoller {
    pub fn new(
        client_id: String,
        client_secret: String,
        refresh_token: String,
        poll_interval: Duration,
    ) -> Self {
        Self {
            client: Client::new(),
            client_id,
            client_secret,
            refresh_token,
            access_token: None,
            poll_interval,
            aide_label_id: None,
        }
    }

    /// Load credentials from a token JSON file + env vars
    pub fn from_token_file(
        token_path: &Path,
        client_id: &str,
        client_secret: &str,
        poll_interval: Duration,
    ) -> Result<Self> {
        let data = std::fs::read_to_string(token_path)
            .with_context(|| format!("reading {}", token_path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&data)?;
        let refresh_token = json["refresh_token"]
            .as_str()
            .context("no refresh_token in token file")?
            .to_string();

        Ok(Self::new(
            client_id.to_string(),
            client_secret.to_string(),
            refresh_token,
            poll_interval,
        ))
    }

    /// Refresh the OAuth2 access token
    async fn refresh_access_token(&mut self) -> Result<()> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("refresh_token", self.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let resp = self
            .client
            .post(TOKEN_URL)
            .form(&params)
            .send()
            .await
            .context("token refresh request failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("token refresh failed: {}", body);
        }

        let token: TokenResponse = resp.json().await?;
        self.access_token = Some(token.access_token);
        debug!("access token refreshed");
        Ok(())
    }

    /// Ensure we have a valid access token
    async fn ensure_token(&mut self) -> Result<String> {
        if self.access_token.is_none() {
            self.refresh_access_token().await?;
        }
        self.access_token
            .clone()
            .context("no access token available")
    }

    /// Make an authenticated GET request, auto-refreshing on 401
    async fn gmail_get(&mut self, url: &str) -> Result<reqwest::Response> {
        let token = self.ensure_token().await?;
        let resp = self
            .client
            .get(url)
            .bearer_auth(&token)
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.refresh_access_token().await?;
            let token = self.access_token.as_ref().unwrap();
            return Ok(self.client.get(url).bearer_auth(token).send().await?);
        }

        Ok(resp)
    }

    /// Resolve the "aide.sh" label ID
    async fn resolve_aide_label(&mut self) -> Result<()> {
        if self.aide_label_id.is_some() {
            return Ok(());
        }

        let url = format!("{}/labels", GMAIL_API);
        let resp = self.gmail_get(&url).await?;
        let list: LabelList = resp.json().await?;

        if let Some(labels) = list.labels {
            for label in labels {
                if label.name == "aide.sh" {
                    self.aide_label_id = Some(label.id.clone());
                    info!(label_id = %label.id, "resolved aide.sh label");
                    return Ok(());
                }
            }
        }

        warn!("aide.sh label not found in Gmail");
        Ok(())
    }

    /// List unread messages (UNREAD, INBOX)
    async fn list_unread(&mut self) -> Result<Vec<MessageRef>> {
        let url = format!(
            "{}/messages?q=is:unread+in:inbox&maxResults=20",
            GMAIL_API
        );
        let resp = self.gmail_get(&url).await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("list messages failed: {}", body);
        }

        let list: MessageList = resp.json().await?;
        Ok(list.messages.unwrap_or_default())
    }

    /// Get full message details
    async fn get_message(&mut self, id: &str) -> Result<Message> {
        let url = format!(
            "{}/messages/{}?format=metadata&metadataHeaders=From&metadataHeaders=Subject",
            GMAIL_API, id
        );
        let resp = self.gmail_get(&url).await?;
        let msg: Message = resp.json().await?;
        Ok(msg)
    }

    /// Extract header value from a message
    fn header_value(msg: &Message, name: &str) -> String {
        msg.payload
            .as_ref()
            .and_then(|p| p.headers.as_ref())
            .and_then(|headers| {
                headers
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case(name))
                    .map(|h| h.value.clone())
            })
            .unwrap_or_default()
    }

    /// Poll once: fetch unread, return summaries
    pub async fn poll_once(&mut self) -> Result<Vec<EmailSummary>> {
        let refs = self.list_unread().await?;
        let mut summaries = Vec::new();

        for msg_ref in refs.iter().take(20) {
            match self.get_message(&msg_ref.id).await {
                Ok(msg) => {
                    summaries.push(EmailSummary {
                        id: msg.id.clone(),
                        from: Self::header_value(&msg, "From"),
                        subject: Self::header_value(&msg, "Subject"),
                        snippet: msg.snippet.unwrap_or_default(),
                        labels: msg.label_ids.unwrap_or_default(),
                    });
                }
                Err(e) => {
                    warn!(msg_id = %msg_ref.id, error = %e, "failed to fetch message");
                }
            }
        }

        Ok(summaries)
    }

    /// Run the polling loop (daemon mode)
    pub async fn run_poll_loop(&mut self) -> Result<()> {
        info!(
            interval_secs = self.poll_interval.as_secs(),
            "starting Gmail poll loop"
        );

        self.resolve_aide_label().await?;

        let mut interval = time::interval(self.poll_interval);

        loop {
            interval.tick().await;

            match self.poll_once().await {
                Ok(emails) => {
                    if !emails.is_empty() {
                        info!(count = emails.len(), "unread emails found");
                        for email in &emails {
                            info!(
                                from = %email.from,
                                subject = %email.subject,
                                "unread"
                            );
                        }
                    } else {
                        debug!("no unread emails");
                    }
                }
                Err(e) => {
                    error!(error = %e, "gmail poll failed");
                }
            }
        }
    }
}

/// Load Gmail poller credentials from vault env or fallback to files
pub fn load_gmail_credentials(
    env: &HashMap<String, String>,
) -> Option<(String, String, String)> {
    let client_id = env
        .get("AIDE_GOOGLE_CLIENT_ID")
        .or_else(|| env.get("GOOGLE_CLIENT_ID"))?
        .clone();
    let client_secret = env
        .get("AIDE_GOOGLE_CLIENT_SECRET")
        .or_else(|| env.get("GOOGLE_CLIENT_SECRET"))?
        .clone();
    let refresh_token = env.get("AIDE_GMAIL_REFRESH_TOKEN")?.clone();

    Some((client_id, client_secret, refresh_token))
}
