import {
  startTransition,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
} from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Link, useParams } from "react-router-dom";
import {
  api,
  getApiBaseUrl,
  type RoomMetadataResponse,
  type RoomSnapshot,
  type StoredHookEvent,
} from "../api";
import { CopyPanel } from "../components/copy-panel";
import { readMessage, useCopyHandler } from "../utils";

const FEED_LIMIT = 10;
const DEFAULT_SERVER_URL = "https://api.supermanager.dev";

type SummaryStatus = "idle" | "ready" | "generating" | "error";
type ConnectionStatus = "connecting" | "live" | "reconnecting";

export function RoomPage() {
  const { roomId = "" } = useParams();
  const roomInfoDropdownRef = useRef<HTMLDetailsElement | null>(null);
  const [room, setRoom] = useState<RoomMetadataResponse | null>(null);
  const [events, setEvents] = useState<StoredHookEvent[]>([]);
  const [snapshot, setSnapshot] = useState<RoomSnapshot>(() => emptyRoomSnapshot());
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>("idle");
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus>("connecting");
  const [hasMore, setHasMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { copiedValue, copy } = useCopyHandler();
  const [clock, setClock] = useState(() => Date.now());

  const canonicalRoomId = room?.room_id || roomId;
  const joinCommand = buildJoinCommand(canonicalRoomId, room?.organization_slug);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  const refreshSummary = useEffectEvent(async () => {
    const nextSnapshot = await api.getSummary(roomId);
    setSnapshot(nextSnapshot);
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
      api.getFeed(roomId, { limit: FEED_LIMIT }),
      api.getSummary(roomId),
    ])
      .then(([nextRoom, feed, nextSnapshot]) => {
        if (cancelled) {
          return;
        }

        startTransition(() => {
          setRoom(nextRoom);
          setEvents(feed.events);
          setHasMore(feed.events.length === FEED_LIMIT);
          setSnapshot(nextSnapshot);
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

  function closeRoomInfo() {
    const dropdown = roomInfoDropdownRef.current;
    if (!dropdown?.open) {
      return;
    }
    dropdown.open = false;
  }

  if (error) {
    return (
      <main className="room-page room-page--error">
        <div className="section-label">Room</div>
        <h1>{roomId || "unknown"}</h1>
        <p className="message message--error">{error}</p>
        <Link className="inline-link" to="/app">
          Back to workspace
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
            {room?.organization_slug && <span>{room.organization_slug}</span>}
            <span className={`connection-pill connection-pill--${connectionStatus}`}>
              {connectionStatus}
            </span>
          </p>
        </div>
        <div className="room-header__actions">
          <details className="room-info-dropdown" ref={roomInfoDropdownRef}>
            <summary className="room-info-dropdown__trigger">Room info</summary>
            <div
              aria-hidden="true"
              className="room-info-dropdown__backdrop"
              onClick={closeRoomInfo}
            />
            <div className="room-section room-info-dropdown__panel">
              <CopyPanel
                copiedValue={copiedValue}
                label="Install CLI"
                onCopy={copy}
                value="curl -fsSL https://supermanager.dev/install.sh | sh"
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
          <SummaryContent
            clock={clock}
            snapshot={snapshot}
            summaryStatus={summaryStatus}
          />
        </div>

        <div className="room-section">
          <div className="room-section__head">
            <span className="section-label">Raw feed</span>
            <span className="section-count">
              {events.length} update{events.length === 1 ? "" : "s"}
            </span>
          </div>

          <div className="feed-list">
            {events.length > 0 ? (
              events.map((event) => (
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

          {hasMore && (
            <button
              className="secondary-button"
              type="button"
              disabled={loadingMore}
              onClick={async () => {
                const oldest = events[events.length - 1];
                if (!oldest) {
                  return;
                }

                setLoadingMore(true);
                try {
                  const page = await api.getFeed(roomId, {
                    limit: FEED_LIMIT,
                    before: oldest.seq,
                  });
                  setEvents((current) => [...current, ...page.events]);
                  setHasMore(page.events.length === FEED_LIMIT);
                } finally {
                  setLoadingMore(false);
                }
              }}
            >
              {loadingMore ? "Loading..." : `Show ${FEED_LIMIT} more`}
            </button>
          )}
        </div>
      </section>
    </main>
  );
}

function SummaryContent({
  clock,
  snapshot,
  summaryStatus,
}: {
  clock: number;
  snapshot: RoomSnapshot;
  summaryStatus: SummaryStatus;
}) {
  const hasContent = hasSnapshotContent(snapshot);

  if (summaryStatus === "error" && !hasContent) {
    return (
      <p className="summary-empty summary-empty--error">
        Summary generation failed.
      </p>
    );
  }

  if (summaryStatus === "generating" && !hasContent) {
    return <p className="summary-empty">Generating room summary...</p>;
  }

  if (!hasContent) {
    return (
      <p className="summary-empty">
        No updates yet. New hook activity will build the room summary here.
      </p>
    );
  }

  return (
    <div className="summary-layout">
      <section className="summary-panel summary-panel--bluf">
        <div className="summary-panel__label">BLUF</div>
        {snapshot.bluf_markdown.trim() ? (
          <MarkdownBlock markdown={snapshot.bluf_markdown} />
        ) : (
          <p className="message">No BLUF yet.</p>
        )}
      </section>

      <details className="summary-disclosure">
        <summary>
          <span>Detailed overview</span>
          <span className="summary-disclosure__hint">hidden by default</span>
        </summary>
        <div className="summary-disclosure__body">
          {snapshot.overview_markdown.trim() ? (
            <MarkdownBlock markdown={snapshot.overview_markdown} />
          ) : (
            <p className="message">No detailed overview yet.</p>
          )}
        </div>
      </details>

      <section className="summary-panel">
        <div className="room-section__head room-section__head--compact">
          <span className="summary-panel__label">Employees</span>
          <span className="section-count">
            {snapshot.employees.length} card{snapshot.employees.length === 1 ? "" : "s"}
          </span>
        </div>

        {snapshot.employees.length > 0 ? (
          <div className="employee-grid">
            {snapshot.employees.map((employee) => (
              <article className="employee-card" key={employee.employee_name}>
                <div className="employee-card__head">
                  <h3>{employee.employee_name}</h3>
                  <time dateTime={employee.last_update_at}>
                    {formatRelativeTime(employee.last_update_at, clock)}
                  </time>
                </div>
                <MarkdownBlock markdown={employee.content_markdown} />
              </article>
            ))}
          </div>
        ) : (
          <p className="message">No employee cards yet.</p>
        )}
      </section>
    </div>
  );
}

function MarkdownBlock({ markdown }: { markdown: string }) {
  return (
    <div className="summary-copy">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{markdown}</ReactMarkdown>
    </div>
  );
}

function emptyRoomSnapshot(): RoomSnapshot {
  return {
    bluf_markdown: "",
    overview_markdown: "",
    employees: [],
  };
}

function hasSnapshotContent(snapshot: RoomSnapshot) {
  return Boolean(
    snapshot.bluf_markdown.trim() ||
      snapshot.overview_markdown.trim() ||
      snapshot.employees.some((employee) => employee.content_markdown.trim()),
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

function buildJoinCommand(roomId: string, organizationSlug?: string) {
  const apiBaseUrl = getApiBaseUrl();
  const parts = ["supermanager", "join", roomId];

  if (organizationSlug) {
    parts.push("--org", shellQuote(organizationSlug));
  }
  if (apiBaseUrl !== DEFAULT_SERVER_URL) {
    parts.push("--server", shellQuote(apiBaseUrl));
  }

  return parts.join(" ");
}

function shellQuote(value: string) {
  return `"${value.replaceAll("\"", "\\\"")}"`;
}
