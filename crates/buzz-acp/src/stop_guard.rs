//! End-of-turn guard for issue turns. When the agent stops, the issue's status
//! decides whether it may: it must reach a release status, or explain in its
//! response.

use serde::Deserialize;

/// What the guard does with a status, or with an unlisted one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusClass {
    Continue,
    Explain,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorAction {
    Continue,
    Release,
}

fn default_continue_statuses() -> Vec<String> {
    ["todo", "in_progress"].map(str::to_string).to_vec()
}
fn default_explain_statuses() -> Vec<String> {
    vec!["blocked".to_string()]
}
fn default_release_statuses() -> Vec<String> {
    ["done", "cancelled"].map(str::to_string).to_vec()
}
fn default_unmapped() -> StatusClass {
    StatusClass::Explain
}
fn default_on_error() -> ErrorAction {
    ErrorAction::Release
}
fn default_max_continuations() -> u32 {
    5
}
fn default_budget_status() -> String {
    "blocked".to_string()
}
fn default_budget_comment() -> String {
    "Stopped {key} after the continuation budget; last status {status}.".to_string()
}
fn default_continue_prompt() -> String {
    "{key} is still {status}. Continue the work, or move the issue to its true status.".to_string()
}
fn default_explain_prompt() -> String {
    "{key} is {status}. Say in this thread what is blocking it before you stop.".to_string()
}
fn default_issue_instructions() -> String {
    "This thread is tracked as {key} ({url}). Keep its status current in Multica.".to_string()
}
fn default_name_prompt() -> String {
    "Write the Multica issue for the user request below.\n\
Return one JSON object and no other text:\n\
{\"title\":\"one line, 120 characters maximum\",\"description\":\"the work to do\"}\n\
The title names the work. The description states the work.\n\
Do not copy the chat message into either field."
        .to_string()
}
fn default_post_placement() -> PostPlacement {
    PostPlacement::Thread
}

/// Where the issue link and the work-loop updates are posted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostPlacement {
    Thread,
    Channel,
}

impl PostPlacement {
    /// The line appended to a work prompt. `root` is the mention's thread root.
    pub fn updates_line(self, root: Option<&str>) -> String {
        match self {
            Self::Thread => match root.map(str::trim).filter(|root| !root.is_empty()) {
                Some(root) => format!(
                    "Post this issue's updates in the thread. Use `buzz messages send --reply-to {root}`. Do not post them at the channel root."
                ),
                None => "Post this issue's updates in the thread with `buzz messages send --reply-to` set to the mention's thread root. Do not post them at the channel root.".to_string(),
            },
            Self::Channel => "Post this issue's updates at the channel root. Use `buzz messages send` with no `--reply-to`.".to_string(),
        }
    }

    pub fn in_thread(self) -> bool {
        matches!(self, Self::Thread)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardSettings {
    #[serde(default = "default_continue_statuses")]
    pub continue_statuses: Vec<String>,
    #[serde(default = "default_explain_statuses")]
    pub explain_statuses: Vec<String>,
    #[serde(default = "default_release_statuses")]
    pub release_statuses: Vec<String>,
    #[serde(default = "default_unmapped")]
    pub unmapped: StatusClass,
    #[serde(default = "default_on_error")]
    pub on_error: ErrorAction,
    #[serde(default = "default_max_continuations")]
    pub max_continuations: u32,
    #[serde(default = "default_budget_status")]
    pub budget_status: String,
    /// `{key}` and `{status}` placeholders, as in the prompts below.
    #[serde(default = "default_budget_comment")]
    pub budget_comment: String,
    /// Kept so older settings and `decide` tests still load. The live loop
    /// sends the phase task and does not read this.
    #[cfg_attr(not(test), allow(dead_code))]
    #[serde(default = "default_continue_prompt")]
    pub continue_prompt: String,
    /// Kept so older settings and `decide` tests still load. The live loop
    /// sends the phase task and does not read this.
    #[cfg_attr(not(test), allow(dead_code))]
    #[serde(default = "default_explain_prompt")]
    pub explain_prompt: String,
    /// Appended to an issue turn; `{key}` and `{url}` placeholders.
    #[serde(default = "default_issue_instructions")]
    pub issue_instructions: String,
    /// Sent with the user message before the issue exists. Replaced wholesale
    /// by `BUZZ_ACP_SET__GOAL__GUARD__NAME_PROMPT`.
    #[serde(default = "default_name_prompt")]
    pub name_prompt: String,
    /// `thread` or `channel`. The issue link and the work-loop updates both
    /// use it. `BUZZ_ACP_SET__GOAL__GUARD__POST_PLACEMENT` replaces it.
    #[serde(default = "default_post_placement")]
    pub post_placement: PostPlacement,
}

impl Default for GuardSettings {
    fn default() -> Self {
        Self {
            continue_statuses: default_continue_statuses(),
            explain_statuses: default_explain_statuses(),
            release_statuses: default_release_statuses(),
            unmapped: default_unmapped(),
            on_error: default_on_error(),
            max_continuations: default_max_continuations(),
            budget_status: default_budget_status(),
            budget_comment: default_budget_comment(),
            continue_prompt: default_continue_prompt(),
            explain_prompt: default_explain_prompt(),
            issue_instructions: default_issue_instructions(),
            name_prompt: default_name_prompt(),
            post_placement: default_post_placement(),
        }
    }
}

impl GuardSettings {
    pub(crate) fn classify(&self, status: &str) -> StatusClass {
        let listed = |list: &[String]| list.iter().any(|s| s == status);
        if listed(&self.continue_statuses) {
            StatusClass::Continue
        } else if listed(&self.explain_statuses) {
            StatusClass::Explain
        } else if listed(&self.release_statuses) {
            StatusClass::Release
        } else {
            self.unmapped
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn on_error(guard: &GuardSettings) -> Option<&String> {
    match guard.on_error {
        ErrorAction::Release => None,
        ErrorAction::Continue => Some(&guard.continue_prompt),
    }
}

/// The guard decision: the continuation prompt to send, or `None` when the
/// agent may stop. `status` is the issue status (`None` when it could not be
/// read); `responded` is whether the turn streamed a non-empty response.
#[cfg_attr(not(test), allow(dead_code))]
pub fn decide<'a>(
    status: Option<&str>,
    responded: bool,
    guard: &'a GuardSettings,
) -> Option<&'a String> {
    match status.map(|s| guard.classify(s)) {
        None => on_error(guard),
        Some(StatusClass::Continue) => Some(&guard.continue_prompt),
        Some(StatusClass::Explain) if !responded => Some(&guard.explain_prompt),
        Some(StatusClass::Explain | StatusClass::Release) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> GuardSettings {
        crate::goal::example_settings().guard
    }

    #[test]
    fn continue_status_continues() {
        let guard = guard();
        for responded in [false, true] {
            assert_eq!(
                decide(Some("in_progress"), responded, &guard),
                Some(&guard.continue_prompt)
            );
        }
    }

    #[test]
    fn explain_status_stops_only_when_responded() {
        let guard = guard();
        assert_eq!(decide(Some("blocked"), true, &guard), None);
        assert_eq!(
            decide(Some("blocked"), false, &guard),
            Some(&guard.explain_prompt)
        );
    }

    #[test]
    fn release_status_stops() {
        let guard = guard();
        assert_eq!(decide(Some("done"), false, &guard), None);
    }

    #[test]
    fn unmapped_status_follows_configured_action() {
        let mut guard = guard();
        guard.unmapped = StatusClass::Continue;
        assert_eq!(
            decide(Some("weird"), false, &guard),
            Some(&guard.continue_prompt)
        );
        guard.unmapped = StatusClass::Explain;
        assert_eq!(
            decide(Some("weird"), false, &guard),
            Some(&guard.explain_prompt)
        );
        assert_eq!(decide(Some("weird"), true, &guard), None);
        guard.unmapped = StatusClass::Release;
        assert_eq!(decide(Some("weird"), false, &guard), None);
    }

    #[test]
    fn read_failure_follows_on_error() {
        let mut guard = guard();
        guard.on_error = ErrorAction::Release;
        assert_eq!(decide(None, false, &guard), None);
        guard.on_error = ErrorAction::Continue;
        assert_eq!(decide(None, false, &guard), Some(&guard.continue_prompt));
    }
}
