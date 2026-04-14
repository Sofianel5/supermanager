import { Link } from "react-router-dom";
import { useCopyHandler } from "../utils";

export function LandingPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className="landing-page">
      <section className="landing-hero">
        <div className="hero-copy">
          <div className="eyebrow">supermanager</div>
          <h1>Real-time visibility into your team's AI productivity.</h1>
          <p className="hero-text">
            Sign in once, approve CLI access when prompted, and keep room creation
            and repo joins in the CLI.
          </p>
          <div className="landing-actions">
            <Link className="inline-link" to="/login">
              Sign in
            </Link>
          </div>
        </div>
      </section>

      <section className="landing-body">
        <div className="landing-column">
          <div className="section-label">How it works</div>
          <ol className="workflow-list">
            <li>Sign in from the browser.</li>
            <li>Create your organization and first room.</li>
            <li>
              Run `supermanager login`, then create or join rooms inside each repo.
            </li>
            <li>Claude Code and Codex hook turns flow into the private room dashboard.</li>
          </ol>
        </div>

        <div className="landing-column landing-column--form">
          <div className="section-label">Start</div>
          <p className="message">
            Rooms are private to your organization. Sign in to manage rooms and
            approve CLI logins.
          </p>
          <div className="auth-actions">
            <Link className="inline-link auth-button" to="/login">
              Continue to login
            </Link>
          </div>

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
