import { FormEvent, startTransition, useEffect, useState } from "react";
import { api, CreateRoomResponse, PublicConfigResponse } from "../api";

export function LandingPage() {
  const [config, setConfig] = useState<PublicConfigResponse | null>(null);
  const [name, setName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [room, setRoom] = useState<CreateRoomResponse | null>(null);
  const [copiedValue, setCopiedValue] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    api
      .getPublicConfig()
      .then((nextConfig) => {
        if (!cancelled) {
          setConfig(nextConfig);
        }
      })
      .catch((loadError: unknown) => {
        if (!cancelled) {
          setError(readMessage(loadError));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("Enter a room name first.");
      return;
    }

    setIsCreating(true);
    setError(null);

    try {
      const createdRoom = await api.createRoom(trimmedName);
      startTransition(() => {
        setRoom(createdRoom);
      });
    } catch (requestError) {
      setError(readMessage(requestError));
    } finally {
      setIsCreating(false);
    }
  }

  async function copy(label: string, value: string) {
    await navigator.clipboard.writeText(value);
    setCopiedValue(label);
    window.setTimeout(() => {
      setCopiedValue((current) => (current === label ? null : current));
    }, 1800);
  }

  return (
    <main className="landing-page">
      <section className="landing-hero">
        <div className="hero-copy">
          <div className="eyebrow">supermanager</div>
          <h1>Real-time visibility into what every coding agent is doing.</h1>
          <p className="hero-text">
            Create a room, connect the repos that matter, and watch a live feed
            and manager summary update as work lands.
          </p>
        </div>

        <div className="hero-rail">
          <div className="hero-stat">
            <span>Flow</span>
            <strong>Create room</strong>
          </div>
          <div className="hero-stat">
            <span>Install once</span>
            <strong>Join per repo</strong>
          </div>
          <div className="hero-stat">
            <span>Signal</span>
            <strong>Live SSE feed</strong>
          </div>
        </div>
      </section>

      <section className="landing-body">
        <div className="landing-column">
          <div className="section-label">How it works</div>
          <ol className="workflow-list">
            <li>Create a room for a team, project, or incident.</li>
            <li>Install the CLI once on each machine that should report in.</li>
            <li>Run the generated join command inside each repo you want to connect.</li>
            <li>Claude Code and Codex hook turns automatically flow into the room.</li>
          </ol>
        </div>

        <div className="landing-column landing-column--form">
          <div className="section-label">Create room</div>
          <form className="room-form" onSubmit={handleSubmit}>
            <label htmlFor="room-name">Team or room name</label>
            <input
              id="room-name"
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="e.g. Platform migration"
            />
            <button type="submit" disabled={isCreating}>
              {isCreating ? "Creating room..." : "Create room"}
            </button>
          </form>

          {config && (
            <button
              className="copy-sheet"
              type="button"
              onClick={() => copy("install", config.install_command)}
            >
              <span className="copy-label">
                Install CLI once {copiedValue === "install" ? "copied" : "click to copy"}
              </span>
              <code>{config.install_command}</code>
            </button>
          )}

          {error && <p className="message message--error">{error}</p>}
        </div>
      </section>

      <section className="result-sheet">
        <div className="section-label">Room output</div>
        {room ? (
          <div className="result-grid">
            <CopyRow
              copiedValue={copiedValue}
              label="Dashboard"
              onCopy={copy}
              value={room.dashboard_url}
            />
            <CopyRow
              copiedValue={copiedValue}
              label="Join command"
              onCopy={copy}
              value={room.join_command}
            />
            <CopyRow
              copiedValue={copiedValue}
              label="Room ID"
              onCopy={copy}
              value={room.room_id}
            />
            <CopyRow
              copiedValue={copiedValue}
              label="Secret"
              onCopy={copy}
              value={room.secret}
            />
          </div>
        ) : (
          <p className="message">
            Room credentials and the exact join command appear here after you create one.
          </p>
        )}
      </section>
    </main>
  );
}

function CopyRow({
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

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}
