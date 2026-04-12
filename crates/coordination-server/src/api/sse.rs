use std::{convert::Infallible, time::Duration};

use async_stream::stream;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use reporter_protocol::StoredHookEvent;
use serde_json::json;
use tokio::sync::broadcast;

use super::summarize::SummaryStatus;
use super::{AppState, auth, resolve_room};

pub async fn stream_feed(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let (_, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    let room = resolve_room(&state, &membership.room_id).await?;
    let room_id = room.room_id;

    let mut replay = if let Some(seq) = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
    {
        state
            .db
            .get_hook_events(&room_id, None, Some(seq), None)
            .await
            .map_err(super::internal_error)?
    } else {
        Vec::new()
    };
    // `get_hook_events` returns newest-first; reverse so replay fires in
    // chronological (oldest → newest) order, matching insertion order.
    replay.reverse();

    let mut hook_rx = state.hook_events.subscribe();
    let mut summary_rx = state.summary_events.subscribe();
    let target_room = room_id.clone();

    let initial_status = state
        .db
        .get_summary_status(&room_id)
        .await
        .ok()
        .and_then(|s| s.parse::<SummaryStatus>().ok())
        .unwrap_or(SummaryStatus::Ready);

    let event_stream = stream! {
        // Replay missed events
        for event in replay {
            yield Ok(hook_event(&event));
        }

        // Send current summary status on connect
        yield Ok(Event::default()
            .event("summary_status")
            .data(json!({ "status": initial_status.as_str() }).to_string()));

        loop {
            tokio::select! {
                hook_result = hook_rx.recv() => {
                    match hook_result {
                        Ok(evt) => {
                            if evt.room_id == target_room {
                                yield Ok(hook_event(&evt.event));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            yield Ok(Event::default()
                                .event("warning")
                                .data(format!("lagged:{skipped}")));
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                summary_result = summary_rx.recv() => {
                    match summary_result {
                        Ok(evt) => {
                            if evt.room_id == target_room {
                                yield Ok(Event::default()
                                    .event("summary_status")
                                    .data(json!({ "status": evt.status.as_str() }).to_string()));
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    };

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn hook_event(event: &StoredHookEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_owned());
    Event::default()
        .event("hook_event")
        .id(event.seq.to_string())
        .data(data)
}
