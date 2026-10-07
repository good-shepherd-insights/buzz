//! Multica REST client. The thread-to-issue link lives only in issue
//! metadata, so every lookup goes through the API; nothing is cached here.

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Map, Value};

pub const DEFAULT_API_URL: &str = "https://api.multica.ai";
pub const DEFAULT_APP_URL: &str = "https://multica.ai";

fn default_api_url() -> String {
    DEFAULT_API_URL.to_string()
}
fn default_issue_status() -> String {
    "todo".to_string()
}
fn default_link_text() -> String {
    "Tracking this as {key}: {url}".to_string()
}
fn default_create_failed_notice() -> String {
    "Could not create an issue for this request.".to_string()
}
fn default_reaction_created() -> String {
    "🎫".to_string()
}
fn default_meta_channel() -> String {
    "buzz_channel".to_string()
}
fn default_meta_root() -> String {
    "buzz_root".to_string()
}
fn default_meta_event() -> String {
    "buzz_event".to_string()
}
fn default_poll_secs() -> Duration {
    Duration::from_secs(60)
}
fn default_timeout_secs() -> Duration {
    Duration::from_secs(10)
}
fn default_title_max_chars() -> usize {
    120
}
fn default_list_page_size() -> usize {
    100
}
fn default_reconcile_prompt_tag() -> String {
    "issue".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MulticaSettings {
    #[serde(default = "default_api_url")]
    pub url: String,
    /// Empty: the token's only workspace is used.
    #[serde(default)]
    pub workspace_id: String,
    /// Empty: the issue is created with no assignee. Set this to assign
    /// this bridge's own Multica agent.
    #[serde(default)]
    pub agent_id: String,
    #[serde(default = "default_issue_status")]
    pub issue_status: String,
    /// Empty: `https://multica.ai/{slug}/issues/{key}` from the workspace slug.
    #[serde(default)]
    pub issue_url_template: String,
    #[serde(default = "default_link_text")]
    pub link_text: String,
    #[serde(default = "default_create_failed_notice")]
    pub create_failed_notice: String,
    /// Reaction added on the source message when the issue is created.
    /// Stays on the message. Seen and working reactions are separate.
    #[serde(default = "default_reaction_created")]
    pub reaction_created: String,
    #[serde(default = "default_meta_channel")]
    pub meta_channel: String,
    #[serde(default = "default_meta_root")]
    pub meta_root: String,
    #[serde(default = "default_meta_event")]
    pub meta_event: String,
    #[serde(default)]
    pub allow_duplicate: bool,
    #[serde(
        default = "default_poll_secs",
        deserialize_with = "crate::settings::secs"
    )]
    pub poll_secs: Duration,
    #[serde(
        default = "default_timeout_secs",
        deserialize_with = "crate::settings::secs"
    )]
    pub timeout_secs: Duration,
    #[serde(default = "default_title_max_chars")]
    pub title_max_chars: usize,
    #[serde(default = "default_list_page_size")]
    pub list_page_size: usize,
    #[serde(default = "default_reconcile_prompt_tag")]
    pub reconcile_prompt_tag: String,
}

#[derive(Debug, Deserialize)]
struct NamedId {
    id: String,
    #[serde(default)]
    slug: String,
}

#[derive(Debug, Deserialize)]
struct StatusRow {
    key: String,
    #[serde(default)]
    category: String,
    archived_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StatusCatalog {
    statuses: Vec<StatusRow>,
}

/// A status key and the category the API returned for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogStatus {
    pub key: String,
    pub category: String,
}

fn unset_id(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value == "00000000-0000-0000-0000-000000000000"
}

/// Fills an empty host, workspace, agent, and issue link from the token.
/// Returns the workspace status catalog so the caller can keep its own
/// status lists when those were set.
pub async fn resolve_account(
    settings: &mut MulticaSettings,
    api_key: &str,
) -> Result<Vec<CatalogStatus>> {
    if settings.url.trim().is_empty() {
        settings.url = DEFAULT_API_URL.to_string();
    }
    let client = reqwest::Client::builder()
        .timeout(settings.timeout_secs)
        .build()
        .context("failed to build Multica HTTP client")?;
    let base = settings.url.trim_end_matches('/').to_string();
    let workspaces: Vec<NamedId> =
        get_json(&client, &base, api_key, None, "/api/workspaces").await?;
    let workspace = if unset_id(&settings.workspace_id) {
        match workspaces.as_slice() {
            [one] => one,
            [] => anyhow::bail!("Multica token is not a member of a workspace"),
            many => {
                let listed = many
                    .iter()
                    .map(|workspace| format!("{} ({})", workspace.slug, workspace.id))
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "Multica token can see more than one workspace; set goal.multica.workspace_id to one of: {listed}"
                );
            }
        }
    } else {
        workspaces
            .iter()
            .find(|workspace| workspace.id == settings.workspace_id)
            .with_context(|| {
                format!(
                    "goal.multica.workspace_id {} is not visible to this token",
                    settings.workspace_id
                )
            })?
    };
    settings.workspace_id = workspace.id.clone();
    if settings.issue_url_template.trim().is_empty() {
        settings.issue_url_template =
            format!("{DEFAULT_APP_URL}/{}/issues/{{key}}", workspace.slug);
    }

    let catalog: StatusCatalog = get_json(
        &client,
        &base,
        api_key,
        Some(&settings.workspace_id),
        "/api/issue-statuses",
    )
    .await?;
    Ok(catalog
        .statuses
        .into_iter()
        .filter(|status| status.archived_at.is_none())
        .map(|status| CatalogStatus {
            key: status.key,
            category: status.category,
        })
        .collect())
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    base: &str,
    api_key: &str,
    workspace_id: Option<&str>,
    path: &str,
) -> Result<T> {
    let mut request = client.get(format!("{base}{path}")).bearer_auth(api_key);
    if let Some(workspace_id) = workspace_id {
        request = request.header("X-Workspace-ID", workspace_id);
    }
    let body = request
        .send()
        .await
        .with_context(|| format!("Multica {path} failed"))?
        .error_for_status()
        .with_context(|| format!("Multica {path} rejected the token"))?
        .text()
        .await?;
    serde_json::from_str(&body).with_context(|| format!("Multica {path} returned unexpected JSON"))
}

impl Default for MulticaSettings {
    fn default() -> Self {
        Self {
            url: default_api_url(),
            workspace_id: String::new(),
            agent_id: String::new(),
            issue_status: default_issue_status(),
            issue_url_template: String::new(),
            link_text: default_link_text(),
            create_failed_notice: default_create_failed_notice(),
            reaction_created: default_reaction_created(),
            meta_channel: default_meta_channel(),
            meta_root: default_meta_root(),
            meta_event: default_meta_event(),
            allow_duplicate: false,
            poll_secs: default_poll_secs(),
            timeout_secs: default_timeout_secs(),
            title_max_chars: default_title_max_chars(),
            list_page_size: default_list_page_size(),
            reconcile_prompt_tag: default_reconcile_prompt_tag(),
        }
    }
}

/// The issue fields the bridge uses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Issue {
    pub id: String,
    /// Human key, e.g. `MUL-123`.
    pub identifier: String,
    /// Status catalog key; the stop guard compares this.
    pub status: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl Issue {
    pub fn metadata_str(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).and_then(Value::as_str)
    }
}

/// The issue a batch works.
#[derive(Debug, Clone, PartialEq)]
pub struct IssueRef {
    pub id: String,
    /// Human key, e.g. `MUL-123`.
    pub identifier: String,
}

impl From<&Issue> for IssueRef {
    fn from(issue: &Issue) -> Self {
        Self {
            id: issue.id.clone(),
            identifier: issue.identifier.clone(),
        }
    }
}

/// `{"issues": [...], "total": n}`
#[derive(Deserialize)]
struct IssueList {
    issues: Vec<Issue>,
    total: usize,
}

pub struct Multica {
    http: reqwest::Client,
    settings: MulticaSettings,
    api_key: String,
}

impl Multica {
    pub fn new(settings: MulticaSettings, api_key: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(settings.timeout_secs)
            .build()
            .context("failed to build Multica HTTP client")?;
        Ok(Self {
            http,
            settings,
            api_key,
        })
    }

    /// One API call; returns the body of a success response.
    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Result<String> {
        let base = self.settings.url.trim_end_matches('/');
        let mut request = self
            .http
            .request(method, format!("{base}{path}"))
            .bearer_auth(&self.api_key)
            .header("X-Workspace-ID", &self.settings.workspace_id);
        if let Some(body) = body {
            request = request.json(&body);
        }
        Ok(request.send().await?.error_for_status()?.text().await?)
    }

    async fn list(
        &self,
        statuses: &[String],
        metadata: Option<Value>,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Issue>, usize)> {
        let mut query = vec![
            ("statuses", statuses.join(",")),
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
        ];
        if !self.settings.agent_id.trim().is_empty() {
            query.push(("assignee_id", self.settings.agent_id.clone()));
        }
        query.extend(metadata.map(|metadata| ("metadata", metadata.to_string())));
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(query)
            .finish();
        let body = self
            .call(Method::GET, &format!("/api/issues?{query}"), None)
            .await?;
        let list: IssueList = serde_json::from_str(&body)?;
        Ok((list.issues, list.total))
    }

    /// Creates the issue from a message, then links it to the thread through
    /// metadata. The title is the first line of `text`. Reconciliation only
    /// sees issues that carry every link, so a failed write deletes the issue
    /// and fails the call. The caller must not treat that as a created issue.
    /// The naming turn uses [`create_named_issue`](Self::create_named_issue).
    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn create_issue(
        &self,
        text: &str,
        channel_id: &str,
        root_event_id: &str,
        event_id: &str,
    ) -> Result<Issue> {
        let title: String = text
            .trim()
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(self.settings.title_max_chars)
            .collect();
        self.create_linked(&title, text, channel_id, root_event_id, event_id)
            .await
    }

    /// Creates the issue from the title and description the agent wrote, then
    /// links it the same way as [`create_issue`](Self::create_issue).
    pub async fn create_named_issue(
        &self,
        title: &str,
        description: &str,
        channel_id: &str,
        root_event_id: &str,
        event_id: &str,
    ) -> Result<Issue> {
        self.create_linked(title, description, channel_id, root_event_id, event_id)
            .await
    }

    async fn create_linked(
        &self,
        title: &str,
        description: &str,
        channel_id: &str,
        root_event_id: &str,
        event_id: &str,
    ) -> Result<Issue> {
        let mut body = json!({
            "title": title,
            "description": description,
            "status": self.settings.issue_status,
            "allow_duplicate": self.settings.allow_duplicate,
        });
        if !self.settings.agent_id.trim().is_empty() {
            body["assignee_type"] = json!("agent");
            body["assignee_id"] = json!(self.settings.agent_id);
        }
        let issue: Issue =
            serde_json::from_str(&self.call(Method::POST, "/api/issues", Some(body)).await?)?;
        let meta = &self.settings;
        let links = [
            (&meta.meta_channel, channel_id),
            (&meta.meta_root, root_event_id),
            (&meta.meta_event, event_id),
        ];
        for (key, value) in links {
            let path = format!("/api/issues/{}/metadata/{key}", issue.id);
            if let Err(error) = self
                .call(Method::PUT, &path, Some(json!({ "value": value })))
                .await
            {
                let link_error = error.context(format!(
                    "failed to link issue {} metadata {key}",
                    issue.identifier
                ));
                return Err(self.abandon_unlinked(&issue, link_error).await);
            }
        }
        Ok(issue)
    }

    /// Removes an issue whose thread link was not written. A leftover issue
    /// has no metadata, so the sweep never dispatches it.
    async fn abandon_unlinked(&self, issue: &Issue, error: anyhow::Error) -> anyhow::Error {
        let path = format!("/api/issues/{}", issue.id);
        match self.call(Method::DELETE, &path, None).await {
            Ok(_) => error,
            Err(cleanup) => error.context(format!(
                "failed to delete unlinked issue {}: {cleanup}",
                issue.identifier
            )),
        }
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<Issue> {
        let path = format!("/api/issues/{issue_id}");
        Ok(serde_json::from_str(
            &self.call(Method::GET, &path, None).await?,
        )?)
    }

    /// Issues in `statuses` that carry all three link metadata keys, paged
    /// until the reported total is reached. A set `agent_id` limits the query
    /// to this bridge's own assignee.
    pub async fn list_open_issues(&self, statuses: &[String]) -> Result<Vec<Issue>> {
        let settings = &self.settings;
        let keys = [
            &settings.meta_channel,
            &settings.meta_root,
            &settings.meta_event,
        ];
        let mut linked = Vec::new();
        let mut offset = 0;
        loop {
            let (page, total) = self
                .list(statuses, None, settings.list_page_size, offset)
                .await?;
            offset += page.len();
            let exhausted = page.is_empty() || offset >= total;
            linked.extend(
                page.into_iter()
                    .filter(|issue| keys.iter().all(|key| issue.metadata_str(key).is_some())),
            );
            if exhausted {
                return Ok(linked);
            }
        }
    }

    pub async fn set_status(&self, issue_id: &str, status: &str) -> Result<()> {
        let path = format!("/api/issues/{issue_id}");
        let body = json!({ "status": status });
        self.call(Method::PUT, &path, Some(body)).await?;
        Ok(())
    }

    pub async fn add_comment(&self, issue_id: &str, content: &str) -> Result<()> {
        let path = format!("/api/issues/{issue_id}/comments");
        let body = json!({ "content": content, "type": "comment" });
        self.call(Method::POST, &path, Some(body)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn issue_json() -> Value {
        json!({
            "id": "11111111-1111-1111-1111-111111111111",
            "identifier": "MUL-12",
            "status": "in_progress",
            "metadata": {"chan": "c", "root": "r", "event": "e"},
            "title": "ignored extra field",
        })
    }

    #[test]
    fn parses_issue_fields() {
        let issue: Issue = serde_json::from_value(issue_json()).unwrap();
        assert_eq!(issue.identifier, "MUL-12");
        assert_eq!(issue.status, "in_progress");
        assert_eq!(issue.metadata_str("root"), Some("r"));
        assert_eq!(issue.metadata_str("absent"), None);
    }

    #[test]
    fn issue_without_metadata_has_empty_map() {
        let mut value = issue_json();
        value.as_object_mut().unwrap().remove("metadata");
        let issue: Issue = serde_json::from_value(value).unwrap();
        assert!(issue.metadata.is_empty());
    }

    #[test]
    fn issue_missing_required_field_is_error() {
        let mut value = issue_json();
        value.as_object_mut().unwrap().remove("identifier");
        assert!(serde_json::from_value::<Issue>(value).is_err());
    }

    #[test]
    fn parses_issue_list() {
        let list: IssueList =
            serde_json::from_value(json!({"issues": [issue_json()], "total": 7})).unwrap();
        assert_eq!(list.issues.len(), 1);
        assert_eq!(list.total, 7);
        assert!(serde_json::from_value::<IssueList>(json!({"issues": []})).is_err());
    }

    #[derive(Clone, Copy)]
    enum Script {
        Linked,
        MetadataFails,
        CreateFails,
        DeleteFails,
    }

    // Hardcoded HTTP paths below are not allowed. They stay only so this fake
    // server can match requests, and they will be changed.
    fn reply(script: Script, request: &str) -> (u16, &'static str) {
        const CREATED: &str = r#"{"id":"11111111-1111-1111-1111-111111111111","identifier":"MUL-12","status":"todo"}"#;
        if request.starts_with("POST /api/issues") {
            return if matches!(script, Script::CreateFails) {
                (500, r#"{"error":"create"}"#)
            } else {
                (201, CREATED)
            };
        }
        if request.starts_with("PUT ") && request.contains("/metadata/") {
            return if matches!(script, Script::MetadataFails | Script::DeleteFails) {
                (500, r#"{"error":"metadata"}"#)
            } else {
                (200, "{}")
            };
        }
        if request.starts_with("DELETE /api/issues/") {
            return if matches!(script, Script::DeleteFails) {
                (500, r#"{"error":"delete"}"#)
            } else {
                (204, "")
            };
        }
        (404, r#"{"error":"unexpected"}"#)
    }

    async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<String> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        let header_end = loop {
            let n = sock.read(&mut tmp).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            if buf.len() > 65_536 {
                return None;
            }
        };
        let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
        let mut content_len = 0usize;
        for line in head.lines() {
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_len = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body_len = buf.len() - header_end;
        while body_len < content_len {
            let n = sock.read(&mut tmp).await.ok()?;
            if n == 0 {
                break;
            }
            body_len += n;
        }
        head.lines().next().map(|line| {
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("");
            let path = parts.next().unwrap_or("");
            format!("{method} {path}")
        })
    }

    /// Serves one script and records `METHOD path` for each request.
    async fn serve(script: Script) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = log.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let log = recorded.clone();
                tokio::spawn(async move {
                    let Some(request) = read_request(&mut sock).await else {
                        return;
                    };
                    log.lock().expect("request log").push(request.clone());
                    let (status, body) = reply(script, &request);
                    let raw = format!(
                        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(raw.as_bytes()).await;
                });
            }
        });
        (url, log)
    }

    fn client(url: &str) -> Multica {
        let mut settings = crate::goal::example_settings().multica;
        settings.url = url.to_string();
        Multica::new(settings, "test-key".into()).unwrap()
    }

    #[tokio::test]
    async fn create_issue_succeeds_only_after_every_link() {
        let (url, log) = serve(Script::Linked).await;
        let issue = client(&url)
            .create_issue("fix the bridge", "channel", "root", "event")
            .await
            .unwrap();
        assert_eq!(issue.identifier, "MUL-12");
        let meta = crate::goal::example_settings().multica;
        let requests = log.lock().expect("request log").clone();
        // Hardcoded HTTP paths in this assertion are not allowed and will be changed.
        assert_eq!(
            requests,
            vec![
                "POST /api/issues".to_string(),
                format!(
                    "PUT /api/issues/{}/metadata/{}",
                    issue.id, meta.meta_channel
                ),
                format!("PUT /api/issues/{}/metadata/{}", issue.id, meta.meta_root),
                format!("PUT /api/issues/{}/metadata/{}", issue.id, meta.meta_event),
            ]
        );
    }

    #[tokio::test]
    async fn metadata_failure_deletes_the_issue_and_fails_creation() {
        let (url, log) = serve(Script::MetadataFails).await;
        let error = client(&url)
            .create_issue("fix the bridge", "channel", "root", "event")
            .await
            .expect_err("unlinked issue must not be returned");
        let error = format!("{error:#}");
        assert!(error.contains("MUL-12"), "{error}");
        assert!(error.contains("metadata"), "{error}");
        let meta = crate::goal::example_settings().multica;
        let requests = log.lock().expect("request log").clone();
        // Hardcoded HTTP paths in this assertion are not allowed and will be changed.
        assert_eq!(
            requests,
            vec![
                "POST /api/issues".to_string(),
                format!(
                    "PUT /api/issues/11111111-1111-1111-1111-111111111111/metadata/{}",
                    meta.meta_channel
                ),
                "DELETE /api/issues/11111111-1111-1111-1111-111111111111".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn create_failure_does_not_delete_anything() {
        let (url, log) = serve(Script::CreateFails).await;
        let error = client(&url)
            .create_issue("fix the bridge", "channel", "root", "event")
            .await
            .expect_err("create failure");
        assert!(!format!("{error:#}").contains("metadata"), "{error}");
        let requests = log.lock().expect("request log").clone();
        // Hardcoded HTTP paths in this assertion are not allowed and will be changed.
        assert_eq!(requests, vec!["POST /api/issues".to_string()]);
    }

    #[tokio::test]
    async fn failed_cleanup_stays_in_the_error() {
        let (url, _log) = serve(Script::DeleteFails).await;
        let error = client(&url)
            .create_issue("fix the bridge", "channel", "root", "event")
            .await
            .expect_err("delete failure must not look like success");
        let error = format!("{error:#}");
        assert!(
            error.contains("failed to delete unlinked issue MUL-12"),
            "{error}"
        );
    }
}
