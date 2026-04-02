import {
  startTransition,
  type FormEvent,
  useEffect,
  useEffectEvent,
  useState,
} from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import {
  api,
  getApiBaseUrl,
  PublicConfigResponse,
  RoomMetadataResponse,
  StoredHookEvent,
} from "../api";
import {
  buildRoomHash,
  clearRoomSecret,
  resolveRoomSecret,
  stashRoomSecret,
} from "../room-credentials";

const FEED_LIMIT = 10;

type SummaryStatus = "idle" | "ready" | "generating" | "error";
type ConnectionStatus = "connecting" | "live" | "reconnecting";

export function RoomPage() {
  const { roomId = "" } = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const [config, setConfig] = useState<PublicConfigResponse | null>(null);
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
  const [roomSecret, setRoomSecret] = useState<string | null>(null);
  const [secretInput, setSecretInput] = useState("");
  const [secretError, setSecretError] = useState<string | null>(null);

  const visibleEvents = expanded ? events : events.slice(0, FEED_LIMIT);
  const hiddenEvents = Math.max(events.length - FEED_LIMIT, 0);
  const joinSecret = roomSecret ?? "YOUR_SECRET";
  const joinCommand = `supermanager join --server "${getApiBaseUrl()}" --app-url "${window.location.origin}" --room "${roomId}" --secret "${joinSecret}"`;

  useEffect(() => {
    const nextSecret = resolveRoomSecret(roomId, location.hash, location.state);
    if (!nextSecret) {
      setRoomSecret(null);
      return;
    }

    stashRoomSecret(roomId, nextSecret);
    setRoomSecret(nextSecret);
    setSecretInput(nextSecret);
  }, [location.hash, location.state, roomId]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  const refreshSummary = useEffectEvent(async () => {
    if (!roomSecret) {
      return;
    }

    const nextSummary = await api.getSummary(roomId, roomSecret);
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
    if (!roomSecret) {
      return;
    }

    let cancelled = false;
    setConnectionStatus("connecting");
    setError(null);
    setSecretError(null);

    Promise.all([
      api.getPublicConfig(),
      api.getRoom(roomId, roomSecret),
      api.getFeed(roomId, roomSecret),
      api.getSummary(roomId, roomSecret),
    ])
      .then(([nextConfig, nextRoom, feed, nextSummary]) => {
        if (cancelled) {
          return;
        }

        startTransition(() => {
          setConfig(nextConfig);
          setRoom(nextRoom);
          setEvents(feed.events);
          setSummary(nextSummary || "No summary yet.");
          setSummaryStatus("ready");
        });
      })
      .catch((loadError: unknown) => {
        if (!cancelled) {
          const message = readMessage(loadError);
          if (message === "missing secret" || message === "invalid secret") {
            clearRoomSecret(roomId);
            setRoomSecret(null);
            setSecretError("Invalid room secret.");
            navigate(
              {
                pathname: `/r/${roomId}`,
              },
              { replace: true },
            );
            return;
          }

          setError(message);
        }
      });

    const stream = api.openRoomStream(roomId, roomSecret);

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
  }, [navigate, roomId, roomSecret]);

  async function copy(label: string, value: string) {
    await navigator.clipboard.writeText(value);
    setCopiedValue(label);
    window.setTimeout(() => {
      setCopiedValue((current) => (current === label ? null : current));
    }, 1800);
  }

  function submitSecret(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextSecret = secretInput.trim();
    if (!nextSecret) {
      setSecretError("Enter the room secret.");
      return;
    }

    stashRoomSecret(roomId, nextSecret);
    setSecretError(null);
    setRoomSecret(nextSecret);
    navigate(
      {
        pathname: `/r/${roomId}`,
        hash: buildRoomHash(nextSecret),
      },
      { replace: true },
    );
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

  if (!roomSecret) {
    return (
      <main className="room-page room-page--error">
        <div className="section-label">Room access</div>
        <h1>{roomId || "unknown"}</h1>
        <p className="message">
          This room now requires its secret before the dashboard will load.
        </p>
        <form className="room-form room-form--gate" onSubmit={submitSecret}>
          <label htmlFor="room-secret">Room secret</label>
          <input
            id="room-secret"
            type="password"
            value={secretInput}
            onChange={(event) => setSecretInput(event.target.value)}
            placeholder="sm_sec_..."
            autoComplete="off"
          />
          <button type="submit">Open room</button>
        </form>
        {secretError && <p className="message message--error">{secretError}</p>}
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
            <span>{roomId}</span>
            <span className={`connection-pill connection-pill--${connectionStatus}`}>
              {connectionStatus}
            </span>
          </p>
        </div>
        <Link className="inline-link" to="/">
          Create another room
        </Link>
      </header>

      <section className="room-layout">
        <div className="room-section">
          <div className="room-section__head">
            <span className="section-label">Manager summary</span>
            <span className={`summary-pill summary-pill--${summaryStatus}`}>
              {summaryStatus === "idle" ? "loading" : summaryStatus}
            </span>
          </div>
          <SummaryContent summary={summary} summaryStatus={summaryStatus} />
        </div>

        <div className="room-section">
          <div className="section-label">
            {roomSecret ? "Room created" : "Connect agents"}
          </div>
          {config && (
            <CopyPanel
              copiedValue={copiedValue}
              label="Install CLI"
              onCopy={copy}
              value={config.install_command}
            />
          )}
          {roomSecret && (
            <CopyPanel
              copiedValue={copiedValue}
              label="Secret"
              onCopy={copy}
              value={roomSecret}
            />
          )}
          <CopyPanel
            copiedValue={copiedValue}
            label="Join command"
            onCopy={copy}
            value={joinCommand}
          />
          <p className="message">
            {roomSecret
              ? "Copy the secret and the exact join command now, then run it in each repo you want connected."
              : "Add the room secret to the join command before running it in a repo."}
          </p>
        </div>

        <div className="room-section">
          <div className="room-section__head">
            <span className="section-label">Activity feed</span>
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
