//! Intake classification. A request that no open issue covers is labelled as
//! needing an answer or tracked work, so the bridge knows which goal to run.
//!
//! The default call is Fastino GLiNER-2.5-Decide
//! (`POST https://api.fastino.ai/v1/chat/completions`). Docs:
//! <https://docs.fastino.ai/concepts/gliner-2-5-decide> and
//! <https://docs.fastino.ai/inference>.

use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::goal::GoalKind;

/// Chat-completions route from the Decide guide.
fn default_classifier_url() -> String {
    "https://api.fastino.ai/v1/chat/completions".to_string()
}
/// Classification model id from the Decide guide.
fn default_classifier_model() -> String {
    "fastino/GLiNER-2.5-Decide".to_string()
}
fn default_ask_label() -> String {
    "ask".to_string()
}
fn default_issue_label() -> String {
    "work".to_string()
}
fn default_min_confidence() -> f64 {
    0.7
}
/// The inference guide requires a read timeout of at least 300 seconds
/// because an idle model can cold-start.
fn default_classifier_timeout() -> Duration {
    Duration::from_secs(300)
}

/// Schema task name. Decide returns this name as the top-level content key.
const INTENT_TASK: &str = "intent";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassifierSettings {
    /// Blank uses `https://api.fastino.ai/v1/chat/completions`. It does not
    /// skip the call.
    #[serde(default = "default_classifier_url")]
    pub url: String,
    #[serde(default = "default_classifier_model")]
    pub model: String,
    #[serde(default = "default_ask_label")]
    pub ask_label: String,
    #[serde(default = "default_issue_label")]
    pub issue_label: String,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    /// Goal used when the call fails or the confidence is too low.
    #[serde(default)]
    pub fallback: GoalKind,
    #[serde(
        default = "default_classifier_timeout",
        deserialize_with = "crate::settings::secs"
    )]
    pub timeout_secs: Duration,
}

impl Default for ClassifierSettings {
    fn default() -> Self {
        Self {
            url: default_classifier_url(),
            model: default_classifier_model(),
            ask_label: default_ask_label(),
            issue_label: default_issue_label(),
            min_confidence: default_min_confidence(),
            fallback: GoalKind::Ask,
            timeout_secs: default_classifier_timeout(),
        }
    }
}

/// Calls the decision endpoint for each unlinked request.
pub struct Classifier {
    client: reqwest::Client,
    config: ClassifierSettings,
    key: String,
}

impl ClassifierSettings {
    /// A blank URL or model is Fastino GLiNER-2.5-Decide. Classification
    /// cannot be turned off.
    pub(crate) fn use_fastino(&mut self) {
        if self.url.trim().is_empty() {
            self.url = default_classifier_url();
        }
        if self.model.trim().is_empty() {
            self.model = default_classifier_model();
        }
    }
}

impl Classifier {
    pub fn new(mut config: ClassifierSettings, key: String) -> Result<Self> {
        if key.trim().is_empty() {
            return Err(anyhow!("BUZZ_ACP_CLASSIFIER_KEY is not set"));
        }
        config.use_fastino();
        let client = reqwest::Client::builder()
            .timeout(config.timeout_secs)
            .build()
            .context("failed to build classifier HTTP client")?;
        Ok(Self {
            client,
            config,
            key,
        })
    }

    /// Classifies `text` with Fastino. A transport or parse failure, or a
    /// confidence under the threshold, yields the configured fallback.
    pub async fn classify(&self, text: &str) -> GoalKind {
        match self.decide(text).await {
            Ok((kind, confidence)) if confidence >= self.config.min_confidence => kind,
            Ok((_, confidence)) => {
                tracing::info!(confidence, "classification below threshold; using fallback");
                self.config.fallback
            }
            Err(error) => {
                tracing::warn!(%error, "classification failed; using fallback");
                self.config.fallback
            }
        }
    }

    async fn decide(&self, text: &str) -> Result<(GoalKind, f64)> {
        if self.config.ask_label.is_empty()
            || self.config.issue_label.is_empty()
            || self.config.ask_label == self.config.issue_label
        {
            return Err(anyhow!(
                "classifier labels must be two different non-empty names"
            ));
        }
        // Decide requires `classifications` to be a list of tasks, and each
        // task needs at least two labels. `store` is off so channel text is
        // not kept as a Fastino inference record.
        let body = json!({
            "model": self.config.model,
            "messages": [{"role": "user", "content": text}],
            "schema": {
                "classifications": [{
                    "task": INTENT_TASK,
                    "labels": [self.config.ask_label, self.config.issue_label],
                    "multi_label": false
                }]
            },
            "include_confidence": true,
            "store": false,
        });
        let started = Instant::now();
        let budget = self.config.timeout_secs;
        loop {
            if started.elapsed() >= budget {
                return Err(anyhow!("classifier retry budget exhausted"));
            }
            let response = self
                .client
                .post(&self.config.url)
                .header("X-API-Key", &self.key)
                .json(&body)
                .send()
                .await?;
            let status = response.status();
            // An idle Decide deployment returns 425 while it warms. 429 and
            // 503 use the same bounded backoff. The inference guide says to
            // honor Retry-After and not to retry auth or validation errors.
            if matches!(status.as_u16(), 425 | 429 | 503) {
                let wait = retry_after(response.headers());
                let _ = response.bytes().await;
                let Some(wait) = wait_within(started, budget, wait) else {
                    return Err(anyhow!(
                        "classifier HTTP {status} exhausted the {budget:?} timeout"
                    ));
                };
                tracing::info!(
                    %status,
                    wait_secs = wait.as_secs(),
                    "classifier backing off"
                );
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                let snippet: String = body.chars().take(300).collect();
                return Err(anyhow!("classifier HTTP {status}: {snippet}"));
            }
            let parsed: Value = response.json().await?;
            return parse_decision(&parsed, &self.config);
        }
    }
}

/// `Retry-After` as delay-seconds. Fastino's warm-up response uses this form.
/// An HTTP-date, or a missing header, waits 30 seconds.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map_or(Duration::from_secs(30), Duration::from_secs)
}

/// How long to sleep before another attempt, or `None` when the wait would
/// run past `budget`.
fn wait_within(started: Instant, budget: Duration, wait: Duration) -> Option<Duration> {
    let remaining = budget.checked_sub(started.elapsed())?;
    if wait >= remaining {
        None
    } else {
        Some(wait)
    }
}

/// Reads the goal and confidence from a chat-completions response.
///
/// GLiNER-2.5-Decide puts this JSON in `choices[0].message.content`:
/// `{"intent": {"label": "...", "confidence": 0.9}}`.
/// A `classifications.intent` object of the same shape is also accepted.
fn parse_decision(response: &Value, config: &ClassifierSettings) -> Result<(GoalKind, f64)> {
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("response has no message content"))?;
    let decision: Value = serde_json::from_str(content).context("message content is not JSON")?;
    let intent = decision
        .pointer("/classifications/intent")
        .or_else(|| decision.pointer(&format!("/{INTENT_TASK}")))
        .ok_or_else(|| anyhow!("response has no intent classification"))?;
    let label = intent
        .get("label")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("intent classification has no label"))?;
    let confidence = intent
        .get("confidence")
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("intent classification has no confidence"))?;
    let kind = if label == config.ask_label {
        GoalKind::Ask
    } else if label == config.issue_label {
        GoalKind::Issue
    } else {
        return Err(anyhow!("unknown intent label {label:?}"));
    };
    Ok((kind, confidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(label: &str, confidence: f64) -> Value {
        let content =
            json!({"classifications": {"intent": {"label": label, "confidence": confidence}}});
        json!({"choices": [{"message": {"content": content.to_string()}}]})
    }

    fn config() -> ClassifierSettings {
        crate::goal::example_settings().classifier
    }

    #[test]
    fn maps_configured_labels() {
        let config = config();
        let (kind, confidence) =
            parse_decision(&response(&config.ask_label, 0.9), &config).unwrap();
        assert_eq!(kind, GoalKind::Ask);
        assert_eq!(confidence, 0.9);
        let (kind, _) = parse_decision(&response(&config.issue_label, 0.5), &config).unwrap();
        assert_eq!(kind, GoalKind::Issue);
    }

    #[test]
    fn unknown_label_is_error() {
        assert!(parse_decision(&response("nope", 0.9), &config()).is_err());
    }

    #[test]
    fn malformed_responses_are_errors() {
        let config = config();
        assert!(parse_decision(&json!({}), &config).is_err());
        assert!(parse_decision(
            &json!({"choices": [{"message": {"content": "not json"}}]}),
            &config
        )
        .is_err());
        let no_confidence = json!({"classifications": {"intent": {"label": config.ask_label}}});
        assert!(parse_decision(
            &json!({"choices": [{"message": {"content": no_confidence.to_string()}}]}),
            &config
        )
        .is_err());
    }
}
