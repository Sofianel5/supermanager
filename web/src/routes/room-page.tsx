import {
  startTransition,
  useEffect,
  useEffectEvent,
  useState,
} from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Link, useParams } from "react-router-dom";
import {
  api,
  getApiBaseUrl,
  RoomMetadataResponse,
  StoredHookEvent,
} from "../api";

const FEED_LIMIT = 10;
const DEFAULT_SERVER_URL = "https://supermanager.fly.dev";
const DEFAULT_APP_URL = "https://supermanager.dev";

type SummaryStatus = "idle" | "ready" | "generating" | "error";
type ConnectionStatus = "connecting" | "live" | "reconnecting";

export function RoomPage() {
  const { roomId = "" } = useParams();
  const [room, setRoom] = useState<RoomMetadataResponse | null>(null);
  const [events, setEvents] = useState<StoredHookEvent[]>([]);
  const [summary, setSummary] = useState("No summary yet.");
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>("idle");
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus>("connecting");
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedValue, setCopiedValue] = useState<string | null>(null);
  const [clock, setClock] = useState(() => Date.now());

  const visibleEvents = expanded ? events : events.slice(0, FEED_LIMIT);
  const hiddenEvents = Math.max(events.length - FEED_LIMIT, 0);
  const canonicalRoomId = room?.room_id || roomId;
  const joinCommand = buildJoinCommand(canonicalRoomId);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  const refreshSummary = useEffectEvent(async () => {
    const nextSummary = await api.getSummary(roomId);
    setSummary(nextSummary || "No summary yet.");
    setSummaryStatus("ready");
  });

  const appendEvent = useEffectEvent((event: StoredHookEvent) => {
    startTransition(() => {
      setEvents((current) => [event, ...current]);
    });
  });

  const handleSummaryStatus = useEffectEvent(async (status: string) => {
    if (status === "generating") {
      setSummaryStatus("generating");
      return;
    }
    if (status === "error") {
      setSummaryStatus("error");
      return;
    }
    await refreshSummary();
  });

  useEffect(() => {
    if (!roomId) {
      setError("Room not found.");
      return;
    }

    let cancelled = false;
    setConnectionStatus("connecting");
    setError(null);

    Promise.all([
      api.getRoom(roomId),
      api.getFeed(roomId),
      api.getSummary(roomId),
    ])
      .then(([nextRoom, feed, nextSummary]) => {
        if (cancelled) {
          return;
        }

        startTransition(() => {
          setRoom(nextRoom);
          setEvents(feed.events);
          setSummary(nextSummary || "No summary yet.");
          setSummaryStatus("ready");
        });
      })
      .catch((loadError: unknown) => {
        if (!cancelled) {
          setError(readMessage(loadError));
        }
      });

    const stream = api.openRoomStream(roomId);

    stream.onopen = () => {
      if (!cancelled) {
        setConnectionStatus("live");
      }
    };

    stream.addEventListener("hook_event", (event) => {
      try {
        appendEvent(JSON.parse(event.data) as StoredHookEvent);
      } catch {
        // Ignore malformed events from the stream.
      }
    });

    stream.addEventListener("summary_status", (event) => {
      try {
        const payload = JSON.parse(event.data) as { status?: string };
        void handleSummaryStatus(payload.status || "ready");
      } catch {
        // Ignore malformed summary status events.
      }
    });

    stream.onerror = () => {
      if (!cancelled) {
        setConnectionStatus("reconnecting");
      }
    };

    return () => {
      cancelled = true;
      stream.close();
    };
  }, [roomId]);

  async function copy(label: string, value: string) {
    await navigator.clipboard.writeText(value);
    setCopiedValue(label);
    window.setTimeout(() => {
      setCopiedValue((current) => (current === label ? null : current));
    }, 1800);
  }

  if (error) {
    return (
      <main className="room-page room-page--error">
        <div className="section-label">Room</div>
        <h1>{roomId || "unknown"}</h1>
        <p className="message message--error">{error}</p>
        <Link className="inline-link" to="/">
          Back to room creation
        </Link>
      </main>
    );
  }

  return (
    <main className="room-page">
      <header className="room-header">
        <div>
          <div className="section-label">supermanager</div>
          <h1>{room?.name || roomId}</h1>
          <p className="room-meta">
            <span>{canonicalRoomId}</span>
            <span className={`connection-pill connection-pill--${connectionStatus}`}>
              {connectionStatus}
            </span>
          </p>
        </div>
        <div className="room-header__actions">
          <Link className="inline-link" to="/">
            Create another room
          </Link>
          <details className="room-info-dropdown">
            <summary className="room-info-dropdown__trigger">Room info</summary>
            <div className="room-section room-info-dropdown__panel">
              <CopyPanel
                copiedValue={copiedValue}
                label="Install CLI"
                onCopy={copy}
                value="curl -fsSL https://supermanager.dev/install.sh | sh"
              />
              <CopyPanel
                copiedValue={copiedValue}
                label="Room code"
                onCopy={copy}
                value={canonicalRoomId}
              />
              <CopyPanel
                copiedValue={copiedValue}
                label="Join another repo"
                onCopy={copy}
                value={joinCommand}
              />
            </div>
          </details>
        </div>
      </header>

      <section className="room-layout">
        <div className="room-section">
          <div className="room-section__head">
            <span className="section-label">Summary</span>
            <span className={`summary-pill summary-pill--${summaryStatus}`}>
              {summaryStatus === "idle" ? "loading" : summaryStatus}
            </span>
          </div>
          <SummaryContent summary={summary} summaryStatus={summaryStatus} />
        </div>

        <div className="room-section">
          <div className="room-section__head">
            <span className="section-label">Raw feed</span>
            <span className="section-count">
              {events.length} update{events.length === 1 ? "" : "s"}
            </span>
          </div>

          <div className="feed-list">
            {visibleEvents.length > 0 ? (
              visibleEvents.map((event) => (
                <article className="feed-item" key={event.event_id}>
                  <div className="feed-item__head">
                    <strong>{event.employee_name}</strong>
                    <time dateTime={event.received_at}>
                      {formatRelativeTime(event.received_at, clock)}
                    </time>
                  </div>
                  <p className="feed-item__meta">
                    <span>{event.repo_root}</span>
                    {event.branch && <span>{event.branch}</span>}
                    <span>{event.client}</span>
                  </p>
                  <pre>{formatPayload(event.payload)}</pre>
                </article>
              ))
            ) : (
              <p className="message">No hook updates have landed yet.</p>
            )}
          </div>

          {hiddenEvents > 0 && (
            <button
              className="secondary-button"
              type="button"
              onClick={() => setExpanded((current) => !current)}
            >
              {expanded ? "Show less" : `Show ${hiddenEvents} more`}
            </button>
          )}
        </div>
      </section>
    </main>
  );
}

function CopyPanel({
  copiedValue,
  label,
  onCopy,
  value,
}: {
  copiedValue: string | null;
  label: string;
  onCopy: (label: string, value: string) => Promise<void>;
  value: string;
}) {
  return (
    <button className="copy-sheet" type="button" onClick={() => onCopy(label, value)}>
      <span className="copy-label">
        {label} {copiedValue === label ? "copied" : "click to copy"}
      </span>
      <code>{value}</code>
    </button>
  );
}

function SummaryContent({
  summary,
  summaryStatus,
}: {
  summary: string;
  summaryStatus: SummaryStatus;
}) {
  if (summaryStatus === "error" && !summary.trim()) {
    return (
      <p className="summary-copy summary-copy--error">
        Summary generation failed.
      </p>
    );
  }

  return (
    <div className="summary-copy">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{summary}</ReactMarkdown>
    </div>
  );
}

function formatPayload(payload: unknown) {
  try {
    return JSON.stringify(payload, null, 2);
  } catch {
    return String(payload);
  }
}

function formatRelativeTime(isoTimestamp: string, now: number) {
  const timestamp = Date.parse(isoTimestamp);
  if (Number.isNaN(timestamp)) {
    return isoTimestamp;
  }

  const seconds = Math.max(0, Math.floor((now - timestamp) / 1000));
  if (seconds < 5) {
    return "just now";
  }
  if (seconds < 60) {
    return `${seconds}s ago`;
  }
  if (seconds < 3600) {
    return `${Math.floor(seconds / 60)}m ago`;
  }
  if (seconds < 86400) {
    return `${Math.floor(seconds / 3600)}h ago`;
  }
  return `${Math.floor(seconds / 86400)}d ago`;
}

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}

function buildJoinCommand(roomId: string) {
  const apiBaseUrl = getApiBaseUrl();
  const appUrl = window.location.origin;
  if (apiBaseUrl === DEFAULT_SERVER_URL && appUrl === DEFAULT_APP_URL) {
    return `supermanager join ${roomId}`;
  }

  return `supermanager join ${roomId} --server "${apiBaseUrl}" --app-url "${appUrl}"`;
}
