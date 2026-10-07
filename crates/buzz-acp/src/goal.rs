//! What a channel turn is for. Intake classifies a message as an ask or an
//! issue; an issue batch works its Multica issue, an ask batch is answered.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;

use crate::classifier::{Classifier, ClassifierSettings};
use crate::multica::{IssueRef, Multica, MulticaSettings};
use crate::pool::{PromptContext, PromptSource};
use crate::queue::{BatchEvent, FlushBatch};
use crate::settings::render;
use crate::stop_guard::{ErrorAction, GuardSettings, StatusClass};

/// The optional `[goal]` settings table. Absent leaves the feature off.
fn default_verify_retries() -> u32 {
    3
}
fn default_verify_retry_secs() -> Duration {
    Duration::from_secs(2)
}
fn default_not_found_notice() -> String {
    "The answer could not be confirmed on the relay.".to_string()
}
fn default_post_failed_notice() -> String {
    "The answer could not be posted.".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalSettings {
    #[serde(default)]
    pub multica: MulticaSettings,
    #[serde(default)]
    pub classifier: ClassifierSettings,
    #[serde(default)]
    pub guard: GuardSettings,
    #[serde(default)]
    pub phases: PhaseSettings,
    #[serde(default)]
    pub answer: AnswerSettings,
}

/// The task sent for a status. The gate does not choose this text.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseSettings {
    #[serde(default = "default_phase_todo")]
    pub todo: String,
    #[serde(default = "default_phase_in_progress")]
    pub in_progress: String,
    #[serde(default = "default_phase_in_review")]
    pub in_review: String,
    #[serde(default = "default_phase_blocked")]
    pub blocked: String,
    #[serde(default = "default_phase_done")]
    pub done: String,
}

fn default_phase_todo() -> String {
    "Status is {status}. Research the request and write the plan. Do not delegate. Name the Buzz project and the repository. Move the issue to in_progress when the plan is set.".to_string()
}
fn default_phase_in_progress() -> String {
    "Status is {status}. Do the work. Create the Buzz project if it does not exist and open the PR. `buzz projects create` and `buzz pr open` each return a link field. That value is a buzz:// deep link. Buzz opens it. It is not a GitHub URL. Write the project link to issue metadata buzz_project_link and the PR link to buzz_pr_link. Move the issue to in_review when the PR is open.".to_string()
}
fn default_phase_in_review() -> String {
    "Status is {status}. Review the work. Project: {project_link}. PR: {pr_link}. Write what is complete, what is missing, and both links. Move the issue to done when the review passes, or back to in_progress when it does not.".to_string()
}
fn default_phase_blocked() -> String {
    "Status is {status}. Name the blocker and who must act. Project: {project_link}. PR: {pr_link}."
        .to_string()
}
fn default_phase_done() -> String {
    "{key} is done.\n\
Issue: {url}\n\
Project: {project_link}\n\
PR: {pr_link}\n\
Write what shipped and what remains.\n\
The project value is the link field from `buzz projects create`.\n\
The PR value is the link field from `buzz pr open`.\n\
Each link is a buzz:// deep link. Buzz opens it. It is not a GitHub URL.\n\
If either value is empty, take it from that command's output and write it to the issue metadata before you finish.\n\
Metadata keys: buzz_project_link, buzz_pr_link."
        .to_string()
}

impl Default for PhaseSettings {
    fn default() -> Self {
        Self {
            todo: default_phase_todo(),
            in_progress: default_phase_in_progress(),
            in_review: default_phase_in_review(),
            blocked: default_phase_blocked(),
            done: default_phase_done(),
        }
    }
}

/// Delivery of an ask turn's answer to the thread.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerSettings {
    /// Relay checks after the first one, per post.
    #[serde(default = "default_verify_retries")]
    pub verify_retries: u32,
    #[serde(
        default = "default_verify_retry_secs",
        deserialize_with = "crate::settings::secs"
    )]
    pub verify_retry_secs: Duration,
    /// Posted when the answer is still not on the relay after a second post.
    #[serde(default = "default_not_found_notice")]
    pub not_found_notice: String,
    /// Posted when the answer could not be posted.
    #[serde(default = "default_post_failed_notice")]
    pub post_failed_notice: String,
}

impl Default for AnswerSettings {
    fn default() -> Self {
        Self {
            verify_retries: default_verify_retries(),
            verify_retry_secs: default_verify_retry_secs(),
            not_found_notice: default_not_found_notice(),
            post_failed_notice: default_post_failed_notice(),
        }
    }
}

impl Default for GoalSettings {
    fn default() -> Self {
        Self {
            multica: MulticaSettings::default(),
            classifier: ClassifierSettings::default(),
            guard: GuardSettings::default(),
            phases: PhaseSettings::default(),
            answer: AnswerSettings::default(),
        }
    }
}

fn apply_status_catalog(settings: &mut GoalSettings, catalog: &[crate::multica::CatalogStatus]) {
    if catalog.is_empty() {
        return;
    }
    let guard = &settings.guard;
    let defaults = GuardSettings::default();
    let lists_are_default = guard.continue_statuses == defaults.continue_statuses
        && guard.explain_statuses == defaults.explain_statuses
        && guard.release_statuses == defaults.release_statuses;
    if !lists_are_default {
        return;
    }
    let mut continue_statuses = Vec::new();
    let mut explain_statuses = Vec::new();
    let mut release_statuses = Vec::new();
    for status in catalog {
        match status_role(&status.key, &status.category) {
            StatusClass::Continue => continue_statuses.push(status.key.clone()),
            StatusClass::Explain => explain_statuses.push(status.key.clone()),
            StatusClass::Release => release_statuses.push(status.key.clone()),
        }
    }
    if continue_statuses.is_empty() || explain_statuses.is_empty() || release_statuses.is_empty() {
        return;
    }
    let keys: Vec<&str> = catalog.iter().map(|status| status.key.as_str()).collect();
    if !keys.contains(&settings.multica.issue_status.as_str()) {
        settings.multica.issue_status = continue_statuses[0].clone();
    }
    if !keys.contains(&settings.guard.budget_status.as_str()) {
        settings.guard.budget_status = explain_statuses[0].clone();
    }
    settings.guard.continue_statuses = continue_statuses;
    settings.guard.explain_statuses = explain_statuses;
    settings.guard.release_statuses = release_statuses;
}

/// Built-in waiting states stay explain. Finished states release. Everything
/// else, including a custom status the API reports as in progress, continues.
fn status_role(key: &str, category: &str) -> StatusClass {
    match key {
        "blocked" | "in_review" => StatusClass::Explain,
        "done" | "cancelled" => StatusClass::Release,
        "backlog" | "todo" | "in_progress" => StatusClass::Continue,
        _ => match category {
            "done" | "cancelled" | "closed" | "completed" | "canceled" => StatusClass::Release,
            "blocked" | "in_review" => StatusClass::Explain,
            _ => StatusClass::Continue,
        },
    }
}

impl GoalSettings {
    /// The guard's status lists must be non-empty and share no status.
    fn validate(&self) -> Result<()> {
        let guard = &self.guard;
        let lists = [
            &guard.continue_statuses,
            &guard.explain_statuses,
            &guard.release_statuses,
        ];
        ensure!(
            lists.iter().all(|list| !list.is_empty()),
            "goal.guard status lists must not be empty"
        );
        let all: Vec<&String> = lists.into_iter().flatten().collect();
        ensure!(
            all.iter().collect::<HashSet<_>>().len() == all.len(),
            "goal.guard status lists must not overlap"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GoalKind {
    #[default]
    Ask,
    Issue,
}

/// The goal settings with the clients built from them. Shared by every prompt
/// task and the issue sweep.
pub struct GoalRuntime {
    pub settings: GoalSettings,
    pub multica: Multica,
    pub classifier: Classifier,
    /// Event ids classified as work and not yet named. The queue still owns
    /// the message. The first prompt on its session asks for the title and
    /// description.
    pending_names: Mutex<HashSet<String>>,
}

impl GoalRuntime {
    /// `None` when no `[goal]` table is configured; an error when it is but a
    /// key or the settings are unusable.
    pub async fn build(
        settings: Option<&GoalSettings>,
        multica_key: Option<String>,
        classifier_key: Option<String>,
    ) -> Result<Option<Arc<Self>>> {
        let Some(settings) = settings else {
            return Ok(None);
        };
        let mut settings = settings.clone();
        let multica_key = multica_key.context("BUZZ_ACP_MULTICA_API_KEY is not set")?;
        let catalog = crate::multica::resolve_account(&mut settings.multica, &multica_key).await?;
        apply_status_catalog(&mut settings, &catalog);
        settings.validate()?;
        settings.classifier.use_fastino();
        let classifier_key = classifier_key.context("BUZZ_ACP_CLASSIFIER_KEY is not set")?;
        Ok(Some(Arc::new(Self {
            multica: Multica::new(settings.multica.clone(), multica_key)?,
            classifier: Classifier::new(settings.classifier.clone(), classifier_key)?,
            settings,
            pending_names: Mutex::new(HashSet::new()),
        })))
    }
}

/// The goal of one channel turn: the issue its batch works (`None` for an ask)
/// and what the stop guard needs with it. A naming turn has no issue yet.
pub struct GoalTurn {
    runtime: Arc<GoalRuntime>,
    issue: Option<IssueRef>,
    continuations: u32,
    /// Event ids this turn must name, in the order their text was joined.
    naming_ids: Vec<String>,
    /// The chat text sent with the name prompt. Compared against the reply so
    /// a copy of the message is not stored as the issue.
    naming_request: Option<String>,
    /// Thread root for the issue link and for `--reply-to` on updates.
    update_root: Option<String>,
    /// Status whose task was last sent.
    last_phase: Option<String>,
}

fn issue_url(settings: &MulticaSettings, issue: &IssueRef) -> String {
    render(
        &settings.issue_url_template,
        &[("key", &issue.identifier), ("id", &issue.id)],
    )
}

/// Whether `run_harness` is done with the source message. Only a fully linked
/// issue is consumed. A create failure falls through to the queue so the
/// request is not dropped.
fn intake_consumes_event(linked: bool) -> bool {
    linked
}

/// Classifies a message that passed the author check and scope resolution.
/// An ask returns `false` and is queued for an answer. A task is marked for
/// naming and also returns `false`: the message stays on this session, and
/// the first prompt writes the title and description before Multica create.
/// Nothing is consumed here. A create failure is reported from the naming turn.
pub async fn intake(
    runtime: &GoalRuntime,
    _ctx: &PromptContext,
    _channel_id: Uuid,
    event: &BatchEvent,
) -> bool {
    let text = event.event.content.as_str();
    if runtime.classifier.classify(text).await != GoalKind::Issue {
        return intake_consumes_event(false);
    }
    let event_id = event.event.id.to_hex();
    runtime
        .pending_names
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(event_id.clone());
    tracing::info!(event_id, "work mention queued for naming");
    intake_consumes_event(false)
}

impl GoalTurn {
    /// The goal of a channel turn; `None` for heartbeats and when the feature
    /// is off. The issue comes from the batch.
    pub fn new(
        ctx: &PromptContext,
        source: &PromptSource,
        batch: Option<&FlushBatch>,
    ) -> Option<Self> {
        let (Some(runtime), PromptSource::Channel(_), Some(batch)) = (&ctx.goal, source, batch)
        else {
            return None;
        };
        let (naming_ids, naming_request) = match pending_name(runtime, batch) {
            Some(pending) => (pending.ids, Some(pending.request)),
            None => (Vec::new(), None),
        };
        Some(Self {
            runtime: runtime.clone(),
            issue: batch.issue().cloned(),
            continuations: 0,
            naming_ids,
            naming_request,
            update_root: reply_root(batch),
            last_phase: None,
        })
    }

    /// The name prompt and the user request. `None` when this turn is an ask
    /// or the batch already carries an issue.
    pub fn name_blocks(&self) -> Option<(String, String)> {
        let request = self.naming_request.as_ref()?;
        Some((
            self.runtime.settings.guard.name_prompt.clone(),
            format!("User request:\n{request}"),
        ))
    }

    /// Parses the naming reply and creates the issue. On success the turn
    /// works that issue. On failure the notice is posted, no issue exists,
    /// and the reply must not be posted to the channel.
    pub async fn accept_name(
        &mut self,
        ctx: &PromptContext,
        batch: &FlushBatch,
        reply: &str,
    ) -> bool {
        let Some(request) = self.naming_request.clone() else {
            return false;
        };
        let ids = self.naming_ids.clone();
        self.clear_naming();
        match self.draft(reply, &request) {
            Ok((title, description)) => {
                self.create_from_draft(ctx, batch, &ids, &title, &description)
                    .await
            }
            Err(error) => {
                tracing::warn!(%error, "naming reply rejected");
                self.post_create_failed(ctx, batch, &ids).await;
                false
            }
        }
    }

    /// The naming prompt ended without a usable reply. Posts the notice and
    /// leaves the message to be answered as an ask.
    pub async fn reject_name(&mut self, ctx: &PromptContext, batch: &FlushBatch) {
        let ids = self.naming_ids.clone();
        self.clear_naming();
        self.post_create_failed(ctx, batch, &ids).await;
    }

    /// The naming prompt was cancelled. Drops the pending mark and posts nothing.
    pub fn drop_name(&mut self) {
        self.clear_naming();
    }

    /// First prompt for a resumed issue. A naming turn has no issue yet.
    pub async fn opening_prompt(&mut self) -> Option<String> {
        if self.issue.is_none() {
            return None;
        }
        let facts = match self.phase_facts().await {
            Ok(facts) => facts,
            Err(error) => {
                tracing::warn!(%error, "phase status read failed");
                PhaseFacts {
                    status: self.runtime.settings.multica.issue_status.clone(),
                    project_link: String::new(),
                    pr_link: String::new(),
                }
            }
        };
        self.last_phase = Some(facts.status.clone());
        Some(self.compose(&facts.status, &facts.project_link, &facts.pr_link))
    }

    /// The task for the phase recorded at create. `None` until an issue exists.
    pub fn phase_prompt(&self) -> Option<String> {
        let status = self.last_phase.as_deref()?;
        if self.issue.is_none() {
            return None;
        }
        Some(self.compose(status, "", ""))
    }

    /// After a work turn stops: send the task for a new status, repeat the
    /// task while a continue status stays put, and stop after a one-shot phase.
    /// Restarts the in-flight deadline when another prompt is sent.
    pub async fn next_continuation(
        &mut self,
        ctx: &PromptContext,
        source: &PromptSource,
        _response: &str,
    ) -> Option<String> {
        if self.issue.is_none() {
            return None;
        }
        let facts = match self.phase_facts().await {
            Ok(facts) => facts,
            Err(error) => {
                tracing::warn!(%error, "phase status read failed");
                if self.runtime.settings.guard.on_error != ErrorAction::Continue {
                    return None;
                }
                let status = self.last_phase.clone()?;
                return self.repeat(ctx, source, &status, "", "").await;
            }
        };
        if matches!(facts.status.as_str(), "cancelled" | "canceled") {
            return None;
        }
        if self.last_phase.as_deref() != Some(facts.status.as_str()) {
            let status = facts.status.clone();
            self.last_phase = Some(status.clone());
            self.arm_deadline(ctx, source);
            return Some(self.compose(&status, &facts.project_link, &facts.pr_link));
        }
        if self.runtime.settings.guard.classify(&facts.status) != StatusClass::Continue {
            return None;
        }
        self.repeat(
            ctx,
            source,
            &facts.status,
            &facts.project_link,
            &facts.pr_link,
        )
        .await
    }

    async fn repeat(
        &mut self,
        ctx: &PromptContext,
        source: &PromptSource,
        status: &str,
        project_link: &str,
        pr_link: &str,
    ) -> Option<String> {
        self.continuations += 1;
        if self.continuations > self.runtime.settings.guard.max_continuations {
            self.spend_budget().await;
            return None;
        }
        self.arm_deadline(ctx, source);
        Some(self.compose(status, project_link, pr_link))
    }

    fn arm_deadline(&self, ctx: &PromptContext, source: &PromptSource) {
        if let (Some(tx), PromptSource::Channel(scope)) = (&ctx.continuation_tx, source) {
            let _ = tx.send(scope.clone());
        }
    }

    async fn phase_facts(&self) -> Result<PhaseFacts> {
        let id = self
            .issue
            .as_ref()
            .map(|issue| issue.id.clone())
            .context("phase facts without an issue")?;
        let issue = self.runtime.multica.get_issue(&id).await?;
        let project_link = issue
            .metadata_str("buzz_project_link")
            .unwrap_or("")
            .to_string();
        let pr_link = issue.metadata_str("buzz_pr_link").unwrap_or("").to_string();
        Ok(PhaseFacts {
            status: issue.status,
            project_link,
            pr_link,
        })
    }

    fn compose(&self, status: &str, project_link: &str, pr_link: &str) -> String {
        let Some(issue) = self.issue.as_ref() else {
            return String::new();
        };
        let url = issue_url(&self.runtime.settings.multica, issue);
        let key = issue.identifier.as_str();
        let header = render(
            &self.runtime.settings.guard.issue_instructions,
            &[("key", key), ("url", url.as_str())],
        );
        let body = render(
            self.phase_template(status),
            &[
                ("key", key),
                ("url", url.as_str()),
                ("status", status),
                ("project_link", project_link),
                ("pr_link", pr_link),
            ],
        );
        let text = if header.is_empty() {
            body
        } else {
            format!("{header}\n{body}")
        };
        self.with_updates(text)
    }

    fn phase_template(&self, status: &str) -> &str {
        let phases = &self.runtime.settings.phases;
        match status {
            "todo" => phases.todo.as_str(),
            "in_progress" => phases.in_progress.as_str(),
            "in_review" => phases.in_review.as_str(),
            "blocked" => phases.blocked.as_str(),
            "done" => phases.done.as_str(),
            _ => match self.runtime.settings.guard.classify(status) {
                StatusClass::Continue => phases.in_progress.as_str(),
                StatusClass::Explain => phases.blocked.as_str(),
                StatusClass::Release => phases.done.as_str(),
            },
        }
    }

    fn with_updates(&self, text: String) -> String {
        let line = self
            .runtime
            .settings
            .guard
            .post_placement
            .updates_line(self.update_root.as_deref());
        format!("{text}\n{line}")
    }

    /// After a clean end of turn of an ask batch: posts the turn's streamed
    /// text to the thread and verifies it on the relay. Nothing for an issue
    /// batch or an empty response.
    pub async fn deliver_answer(&self, ctx: &PromptContext, batch: &FlushBatch, text: &str) {
        let Some(first) = batch.events.first() else {
            return;
        };
        if self.issue.is_some() || text.is_empty() {
            return;
        }
        crate::delivery::deliver_answer(
            &ctx.rest_client,
            &ctx.agent_keys,
            batch.channel_id,
            first,
            &self.runtime.settings.answer,
            text,
        )
        .await;
    }

    /// Moves the issue to the budget status and says so on it. Failures are
    /// logged; the turn ends either way.
    async fn spend_budget(&self) {
        let Some(issue) = &self.issue else {
            return;
        };
        let guard = &self.runtime.settings.guard;
        tracing::warn!(issue = %issue.identifier, "continuation budget spent");
        let comment = render(
            &guard.budget_comment,
            &[("key", &issue.identifier), ("status", &guard.budget_status)],
        );
        let multica = &self.runtime.multica;
        if let Err(error) = multica.set_status(&issue.id, &guard.budget_status).await {
            tracing::error!(%error, issue = %issue.identifier, "failed to set budget status");
        }
        if let Err(error) = multica.add_comment(&issue.id, &comment).await {
            tracing::error!(%error, issue = %issue.identifier, "failed to comment budget status");
        }
    }

    fn clear_naming(&mut self) {
        let ids = std::mem::take(&mut self.naming_ids);
        self.naming_request = None;
        let mut pending = self
            .runtime
            .pending_names
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for id in ids {
            pending.remove(&id);
        }
    }

    fn draft(&self, reply: &str, request: &str) -> Result<(String, String)> {
        let parsed = parse_name(reply)?;
        let max_chars = self.runtime.settings.multica.title_max_chars;
        let title = one_line(&parsed.title, max_chars);
        let description = parsed.description.trim().to_string();
        if title.is_empty() || description.is_empty() {
            anyhow::bail!("naming reply is missing a title or description");
        }
        if copies_message(&title, &description, request, max_chars) {
            anyhow::bail!("naming reply copied the chat message");
        }
        Ok((title, description))
    }

    async fn create_from_draft(
        &mut self,
        ctx: &PromptContext,
        batch: &FlushBatch,
        ids: &[String],
        title: &str,
        description: &str,
    ) -> bool {
        let Some(anchor) = anchor_event(batch, ids) else {
            self.post_create_failed(ctx, batch, ids).await;
            return false;
        };
        let channel = batch.channel_id.to_string();
        let root = anchor.reply_thread();
        self.update_root = Some(root.clone());
        let event_id = anchor.event.id.to_hex();
        let created = self
            .runtime
            .multica
            .create_named_issue(title, description, &channel, &root, &event_id)
            .await;
        let issue = match created {
            Ok(issue) => issue,
            Err(error) => {
                tracing::error!(%error, "failed to create issue");
                self.post_create_failed(ctx, batch, ids).await;
                return false;
            }
        };
        let issue = IssueRef::from(&issue);
        let meta = &self.runtime.settings.multica;
        let link = render(
            &meta.link_text,
            &[
                ("key", &issue.identifier),
                ("url", &issue_url(meta, &issue)),
            ],
        );
        if let Err(error) = crate::delivery::post_placed(
            &ctx.rest_client,
            &ctx.agent_keys,
            batch.channel_id,
            anchor,
            &link,
            self.runtime.settings.guard.post_placement.in_thread(),
        )
        .await
        {
            tracing::error!(%error, issue = %issue.identifier, "failed to post issue link");
        }
        for event in named_events(batch, ids) {
            crate::pool::reaction_add(
                &ctx.rest_client,
                &event.routing_event_id(),
                &meta.reaction_created,
            )
            .await;
        }
        tracing::info!(issue = %issue.identifier, "created named issue");
        let created_status = self.runtime.settings.multica.issue_status.clone();
        self.last_phase = Some(created_status);
        self.issue = Some(issue);
        true
    }

    async fn post_create_failed(&self, ctx: &PromptContext, batch: &FlushBatch, ids: &[String]) {
        let Some(event) = notice_event(batch, ids) else {
            return;
        };
        let notice = &self.runtime.settings.multica.create_failed_notice;
        if let Err(error) = crate::delivery::post_placed(
            &ctx.rest_client,
            &ctx.agent_keys,
            batch.channel_id,
            event,
            notice,
            self.runtime.settings.guard.post_placement.in_thread(),
        )
        .await
        {
            tracing::error!(%error, "failed to post create-failed notice");
        }
    }
}

struct PendingName {
    ids: Vec<String>,
    request: String,
}

#[derive(Deserialize)]
struct NameDraft {
    title: String,
    description: String,
}

struct PhaseFacts {
    status: String,
    project_link: String,
    pr_link: String,
}

fn reply_root(batch: &FlushBatch) -> Option<String> {
    batch
        .events
        .last()
        .or_else(|| batch.cancelled_events.last())
        .map(|event| event.reply_thread())
}

fn pending_name(runtime: &GoalRuntime, batch: &FlushBatch) -> Option<PendingName> {
    if batch.issue().is_some() {
        return None;
    }
    let pending = runtime
        .pending_names
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut chosen = Vec::new();
    for event in batch.events.iter().chain(batch.cancelled_events.iter()) {
        let id = event.event.id.to_hex();
        if !pending.contains(&id) {
            continue;
        }
        let text = event.event.content.as_str().trim();
        if text.is_empty() {
            continue;
        }
        chosen.push((event.event.created_at, id, text.to_string()));
    }
    drop(pending);
    if chosen.is_empty() {
        return None;
    }
    chosen.sort_by_key(|item| item.0);
    let ids = chosen.iter().map(|item| item.1.clone()).collect();
    let request = chosen
        .into_iter()
        .map(|item| item.2)
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(PendingName { ids, request })
}

fn named_events<'a>(batch: &'a FlushBatch, ids: &[String]) -> Vec<&'a BatchEvent> {
    batch
        .events
        .iter()
        .chain(batch.cancelled_events.iter())
        .filter(|event| ids.iter().any(|id| id == &event.event.id.to_hex()))
        .collect()
}

fn anchor_event<'a>(batch: &'a FlushBatch, ids: &[String]) -> Option<&'a BatchEvent> {
    let mut events = named_events(batch, ids);
    events.sort_by_key(|event| event.event.created_at);
    events.pop()
}

fn notice_event<'a>(batch: &'a FlushBatch, ids: &[String]) -> Option<&'a BatchEvent> {
    anchor_event(batch, ids)
        .or_else(|| batch.events.last())
        .or_else(|| batch.cancelled_events.last())
}

fn parse_name(reply: &str) -> Result<NameDraft> {
    let text = strip_fence(reply.trim());
    if let Ok(draft) = serde_json::from_str::<NameDraft>(text) {
        return Ok(draft);
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if end > start {
            return serde_json::from_str(&text[start..=end])
                .context("naming reply is not a title and description");
        }
    }
    anyhow::bail!("naming reply is not a title and description")
}

fn strip_fence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let rest = if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("json") {
        &rest[4..]
    } else {
        rest
    };
    let rest = rest.trim_start_matches(['\r', '\n', ' ']);
    rest.trim_end().strip_suffix("```").unwrap_or(rest).trim()
}

fn one_line(title: &str, max_chars: usize) -> String {
    let line = title.lines().next().unwrap_or("").trim();
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn copies_message(title: &str, description: &str, request: &str, max_chars: usize) -> bool {
    let request_collapsed = collapsed(request);
    let first = request
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let first = collapsed(first);
    let title = collapsed(title);
    let description = collapsed(description);
    title_copies(&title, &request_collapsed, max_chars)
        || title_copies(&title, &first, max_chars)
        || (!description.is_empty() && description == request_collapsed)
}

/// Exact copy, or the title-length prefix left when a copied line was cut to
/// `max_chars`. A shorter title that merely shares a few words is kept.
fn title_copies(title: &str, source: &str, max_chars: usize) -> bool {
    if title.is_empty() || source.is_empty() {
        return false;
    }
    if title == source {
        return true;
    }
    let truncated = one_line(source, max_chars);
    source.chars().count() > title.chars().count() && title == truncated
}

fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The documented `[goal]` example from the shipped settings file, uncommented.
#[cfg(test)]
pub(crate) fn example_settings() -> GoalSettings {
    #[derive(Deserialize)]
    struct Doc {
        goal: GoalSettings,
    }
    let example = include_str!("../buzz-acp.settings.toml")
        .lines()
        .skip_while(|line| *line != "#[goal]")
        .filter_map(|line| line.strip_prefix('#').filter(|rest| !rest.starts_with(' ')))
        .collect::<Vec<_>>()
        .join("\n");
    toml::from_str::<Doc>(&example)
        .expect("commented [goal] example must parse")
        .goal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_failure_leaves_the_message_queued() {
        assert!(intake_consumes_event(true));
        assert!(!intake_consumes_event(false));
    }

    #[test]
    fn commented_example_parses_and_validates() {
        example_settings().validate().unwrap();
    }

    #[test]
    fn overlapping_status_lists_are_rejected() {
        let mut settings = example_settings();
        let shared = settings.guard.release_statuses[0].clone();
        settings.guard.explain_statuses.push(shared);
        assert!(settings.validate().is_err());
    }
}
