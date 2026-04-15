import { Link } from "react-router-dom";
import { useCopyHandler } from "../utils";

const INSTALL_COMMAND = "curl -fsSL https://supermanager.dev/install.sh | sh";
const LOGIN_COMMAND = 'supermanager login --server "https://api.supermanager.dev"';

export function InstallPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className="landing-page install-page">
      <section className="room-header">
        <div>
          <div className="section-label">CLI install</div>
          <h1>Install supermanager CLI</h1>
          <p className="hero-text">
            Install the binary once, then authenticate from any repo machine before
            joining rooms.
          </p>
        </div>
        <div className="room-header__actions app-toolbar">
          <Link className="secondary-button" to="/app">
            Back to rooms
          </Link>
        </div>
      </section>

      <section className="landing-body landing-body--single">
        <div className="landing-column install-instructions">
          <div className="section-label">Commands</div>
          <button
            className="copy-sheet"
            type="button"
            onClick={() => void copy("install", INSTALL_COMMAND)}
          >
            <span className="copy-label">
              Install CLI {copiedValue === "install" ? "copied" : "click to copy"}
            </span>
            <code>{INSTALL_COMMAND}</code>
          </button>
          <button
            className="copy-sheet"
            type="button"
            onClick={() => void copy("login", LOGIN_COMMAND)}
          >
            <span className="copy-label">
              Login {copiedValue === "login" ? "copied" : "click to copy"}
            </span>
            <code>{LOGIN_COMMAND}</code>
          </button>

          <div className="section-label">Steps</div>
          <ol className="workflow-list">
            <li>Install the CLI on the machine that reports repo activity.</li>
            <li>Run the login command and approve device access in the browser when prompted.</li>
            <li>Create or join a room from inside a git repository.</li>
          </ol>
        </div>
      </section>
    </main>
  );
}
