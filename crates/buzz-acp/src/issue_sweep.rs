//! Periodic sweep of the agent's open Multica issues. Each linked issue's
//! originating message is fetched from the relay and handed to the main loop
//! with its issue, so work survives a restart or a stopped turn.

use std::sync::Arc;

use anyhow::{ensure, Context, Result};
use serde_json::json;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::goal::GoalRuntime;
use crate::multica::{Issue, IssueRef};
use crate::relay::{BuzzEvent, RestClient};

/// Lists open issues every `poll_secs`, the first time immediately, and sends
/// each issue's originating event over `tx`.
pub fn spawn(runtime: Arc<GoalRuntime>, rest: RestClient, tx: mpsc::Sender<(BuzzEvent, IssueRef)>) {
    tokio::spawn(async move {
        let meta = &runtime.settings.multica;
        let mut tick = tokio::time::interval(meta.poll_secs);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let issues = match runtime
                .multica
                // Only issues the agent must keep working; explain statuses are
                // parked until someone moves them back.
                .list_open_issues(&runtime.settings.guard.continue_statuses)
                .await
            {
                Ok(issues) => issues,
                Err(error) => {
                    tracing::warn!(%error, "issue sweep failed");
                    continue;
                }
            };
            for issue in issues {
                match fetch(&rest, &issue, &meta.meta_channel, &meta.meta_event).await {
                    Ok(event) => {
                        if tx.send((event, IssueRef::from(&issue))).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, issue = %issue.identifier, "issue sweep skipped issue");
                    }
                }
            }
        }
    });
}

/// The message `issue` was created from, checked against its link metadata.
async fn fetch(
    rest: &RestClient,
    issue: &Issue,
    channel_key: &str,
    event_key: &str,
) -> Result<BuzzEvent> {
    let channel_id: Uuid = issue
        .metadata_str(channel_key)
        .and_then(|channel| channel.parse().ok())
        .context("issue channel metadata is not a channel id")?;
    let event_id = issue
        .metadata_str(event_key)
        .context("issue has no event metadata")?;
    let found = rest.query_raw(&[json!({ "ids": [event_id] })]).await?;
    let raw = found
        .as_array()
        .and_then(|events| events.first())
        .context("event not found on relay")?;
    let event: nostr::Event = serde_json::from_value(raw.clone())?;
    let channel = channel_id.to_string();
    let in_channel = event.tags.iter().any(|tag| {
        let tag = tag.as_slice();
        tag.first().map(String::as_str) == Some("h")
            && tag.get(1).map(String::as_str) == Some(channel.as_str())
    });
    ensure!(
        event.id.to_hex() == event_id && event.verify().is_ok() && in_channel,
        "event does not match its issue"
    );
    Ok(BuzzEvent {
        connection_generation: 0,
        channel_id,
        event,
    })
}
