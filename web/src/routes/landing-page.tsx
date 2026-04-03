import { useState } from "react";

export function LandingPage() {
  const [copiedValue, setCopiedValue] = useState<string | null>(null);

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
            <li>Run `supermanager create room` inside a git repo to create a room and connect it.</li>
            <li>Run `supermanager join &lt;room-code&gt;` inside each additional git repo you want to connect.</li>
            <li>Claude Code and Codex hook turns automatically flow into the room dashboard.</li>
          </ol>
        </div>

        <div className="landing-column landing-column--form">
          <div className="section-label">Install</div>
          <button
            className="copy-sheet"
            type="button"
            onClick={() => copy("install", "curl -fsSL https://supermanager.dev/install.sh | sh")}
          >
            <span className="copy-label">
              {copiedValue === "install" ? "copied" : "click to copy"}
            </span>
            <code>curl -fsSL https://supermanager.dev/install.sh | sh</code>
          </button>
        </div>
      </section>
    </main>
  );
}
