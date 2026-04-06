use tokio::sync::broadcast;

use crate::store::Db;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryStatus {
    Generating,
    Ready,
    Error,
}

impl SummaryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generating => "generating",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for SummaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SummaryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "generating" => Ok(Self::Generating),
            "ready" => Ok(Self::Ready),
            "error" => Ok(Self::Error),
            other => Err(format!("unknown summary status: {other}")),
        }
    }
}

#[derive(Clone)]
pub struct SummaryStatusEvent {
    pub room_id: String,
    pub status: SummaryStatus,
}

pub fn broadcast_status(
    db: &Db,
    summary_events: &broadcast::Sender<SummaryStatusEvent>,
    room_id: &str,
    status: SummaryStatus,
) {
    let _ = db.set_summary_status(room_id, status.as_str());
    let _ = summary_events.send(SummaryStatusEvent {
        room_id: room_id.to_owned(),
        status,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_status_round_trips() {
        for (raw, expected) in [
            ("generating", SummaryStatus::Generating),
            ("ready", SummaryStatus::Ready),
            ("error", SummaryStatus::Error),
        ] {
            assert_eq!(raw.parse::<SummaryStatus>().unwrap(), expected);
            assert_eq!(expected.as_str(), raw);
        }
    }
}
