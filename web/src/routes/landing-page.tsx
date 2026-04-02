import { FormEvent, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, PublicConfigResponse } from "../api";

export function LandingPage() {
  const navigate = useNavigate();
  const [config, setConfig] = useState<PublicConfigResponse | null>(null);
  const [name, setName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
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
      navigate({
        pathname: `/r/${createdRoom.room_id}`,
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
          <h1>Real-time visibility into your team's AI productivity.</h1>
          <p className="hero-text">
            Install the CLI, create a room from the terminal, connect the repos
            that matter, and watch a live feed and manager summary update as work lands.
          </p>
        </div>
      </section>

      <section className="landing-body">
        <div className="landing-column">
          <div className="section-label">How it works</div>
          <ol className="workflow-list">
            <li>Install the CLI once on each machine that should report in.</li>
            <li>Run `supermanager create room` to get a room code and dashboard URL.</li>
            <li>Run `supermanager join &lt;room-code&gt;` inside each repo you want to connect.</li>
            <li>Claude Code and Codex hook turns automatically flow into the room dashboard.</li>
          </ol>
        </div>

        <div className="landing-column landing-column--form">
          <div className="section-label">Create from browser</div>
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
    </main>
  );
}

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}
