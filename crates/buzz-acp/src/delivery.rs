//! Bridge-owned channel posts. The goal loop announces a created issue in the
//! thread through [`post`], and delivers an ask turn's answer through
//! [`deliver_answer`], which counts a post as delivered only once [`confirm`]
//! finds it on the relay.

use std::time::Duration;

use anyhow::{Context, Result};
use buzz_core::kind::KIND_STREAM_MESSAGE;
use nostr::{EventBuilder, Keys, Kind, Tag};
use serde_json::json;
use uuid::Uuid;

use crate::goal::AnswerSettings;
use crate::queue::BatchEvent;
use crate::relay::RestClient;

/// Signs and submits a channel message replying to `event` in its thread.
pub async fn post(
    rest: &RestClient,
    keys: &Keys,
    channel_id: Uuid,
    event: &BatchEvent,
    content: &str,
) -> Result<String> {
    let (root, reply_to) = (event.reply_thread(), event.routing_event_id());
    let mut tags = vec![
        Tag::parse(["h", channel_id.to_string().as_str()])?,
        Tag::parse(["e", root.as_str(), "", "root"])?,
    ];
    if reply_to != root {
        tags.push(Tag::parse(["e", reply_to.as_str(), "", "reply"])?);
    }
    let message = EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE as u16), content)
        .tags(tags)
        .sign_with_keys(keys)
        .context("failed to sign channel message")?;
    rest.submit_event(&message)
        .await
        .context("failed to submit channel message")?;
    Ok(message.id.to_hex())
}

/// Posts `content` in the thread or at the channel root. [`post`] stays the
/// threaded call. `in_thread` selects the mention's thread root.
pub async fn post_placed(
    rest: &RestClient,
    keys: &Keys,
    channel_id: Uuid,
    event: &BatchEvent,
    content: &str,
    in_thread: bool,
) -> Result<String> {
    if in_thread {
        post(rest, keys, channel_id, event, content).await
    } else {
        post_channel(rest, keys, channel_id, content).await
    }
}

async fn post_channel(
    rest: &RestClient,
    keys: &Keys,
    channel_id: Uuid,
    content: &str,
) -> Result<String> {
    let tags = vec![Tag::parse(["h", channel_id.to_string().as_str()])?];
    let message = EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE as u16), content)
        .tags(tags)
        .sign_with_keys(keys)
        .context("failed to sign channel message")?;
    rest.submit_event(&message)
        .await
        .context("failed to submit channel message")?;
    Ok(message.id.to_hex())
}

/// Checks whether the relay holds `event_id`, retrying `retries` times with
/// `retry_delay` between checks. `true` only when the relay returns the event.
async fn confirm(rest: &RestClient, event_id: &str, retries: u32, retry_delay: Duration) -> bool {
    let filter = json!({"ids": [event_id], "kinds": [KIND_STREAM_MESSAGE]});
    for attempt in 0..=retries {
        if attempt > 0 {
            tokio::time::sleep(retry_delay).await;
        }
        match rest.query_raw(std::slice::from_ref(&filter)).await {
            Ok(events) if events.as_array().is_some_and(|found| !found.is_empty()) => {
                return true;
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, event_id, "delivery check failed"),
        }
    }
    false
}

/// Posts a notice to the thread. A failure is logged, never raised.
async fn notify(rest: &RestClient, keys: &Keys, channel_id: Uuid, event: &BatchEvent, text: &str) {
    if let Err(error) = post(rest, keys, channel_id, event, text).await {
        tracing::warn!(%error, "failed to post notice");
    }
}

/// Posts a turn's answer as a reply to `event` and confirms it on the relay.
/// A post not found is posted once more; still not found, or a post that is
/// rejected, is reported to the thread.
pub async fn deliver_answer(
    rest: &RestClient,
    keys: &Keys,
    channel_id: Uuid,
    event: &BatchEvent,
    settings: &AnswerSettings,
    answer: &str,
) {
    for _ in 0..2 {
        match post(rest, keys, channel_id, event, answer).await {
            Ok(event_id) => {
                if confirm(
                    rest,
                    &event_id,
                    settings.verify_retries,
                    settings.verify_retry_secs,
                )
                .await
                {
                    return;
                }
                tracing::error!(event_id, "answer not found on relay after posting");
            }
            Err(error) => {
                tracing::error!(%error, "answer post failed");
                notify(rest, keys, channel_id, event, &settings.post_failed_notice).await;
                return;
            }
        }
    }
    notify(rest, keys, channel_id, event, &settings.not_found_notice).await;
}
