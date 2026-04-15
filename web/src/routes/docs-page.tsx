import { useCopyHandler } from "../utils";

const INSTALL_COMMAND = "curl -fsSL https://supermanager.dev/install.sh | sh";
const LOGIN_COMMAND = "supermanager login";
const CREATE_ROOM_COMMAND = 'supermanager create room "Frontend"';
const JOIN_ROOM_COMMAND = 'supermanager join "<room-id>"';

export function DocsPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className="landing-page docs-page">
      <header className="docs-page__header">
        <div className="section-label">Docs</div>
        <h1>Setup Supermanager Locally</h1>
        <p className="hero-text docs-page__intro">
          Install the CLI locally, authenticate with your org account, join
          shared rooms, and add the Supermanager MCP to your coding assistant.
        </p>
        <button
          className="copy-sheet docs-command docs-command--hero"
          type="button"
          onClick={() => void copy("install", INSTALL_COMMAND)}
        >
          <span className="copy-label">
            Install CLI {copiedValue === "install" ? "copied" : "click to copy"}
          </span>
          <code>{INSTALL_COMMAND}</code>
        </button>
      </header>

      <section className="docs-sections">
        <article className="docs-section">
          <div className="section-label">Authenticating</div>
          <p className="docs-section__body">
            Authenticate once with your org account. The same local identity is
            used by the web app, CLI workflows, and the MCP server.
          </p>
          <div className="docs-command-stack">
            <button
              className="copy-sheet docs-command"
              type="button"
              onClick={() => void copy("login", LOGIN_COMMAND)}
            >
              <span className="copy-label">
                Login {copiedValue === "login" ? "copied" : "click to copy"}
              </span>
              <code>{LOGIN_COMMAND}</code>
            </button>
          </div>
          <p className="docs-section__note">
            If your account belongs to multiple organizations, add{" "}
            <code>--org &quot;&lt;org-slug&gt;&quot;</code> to the login
            command.
          </p>
        </article>

        <article className="docs-section">
          <div className="section-label">Joining rooms</div>
          <p className="docs-section__body">
            Join a room from the CLI when you want a repo linked to a shared
            workspace in your org.
          </p>
          <div className="docs-command-stack">
            <button
              className="copy-sheet docs-command"
              type="button"
              onClick={() => void copy("create-room", CREATE_ROOM_COMMAND)}
            >
              <span className="copy-label">
                Create room{" "}
                {copiedValue === "create-room" ? "copied" : "click to copy"}
              </span>
              <code>{CREATE_ROOM_COMMAND}</code>
            </button>
            <button
              className="copy-sheet docs-command"
              type="button"
              onClick={() => void copy("join-room", JOIN_ROOM_COMMAND)}
            >
              <span className="copy-label">
                Join room{" "}
                {copiedValue === "join-room" ? "copied" : "click to copy"}
              </span>
              <code>{JOIN_ROOM_COMMAND}</code>
            </button>
          </div>
          <p className="docs-section__note">
            Typical flow: create the room once, then have teammates join it from
            their local repos so usage data rolls up under the same shared room.
          </p>
        </article>

        <article className="docs-section">
          <div className="section-label">
            Querying usage with the Supermanager MCP
          </div>
          <p className="docs-section__body">
            The Supermanager MCP is still planned. Once it ships, this section
            will document the real Codex and Claude Code setup commands for
            querying org usage from inside the assistant.
          </p>
          <div className="docs-section__examples">
            <div className="copy-label">Example use cases</div>
            <ul className="workflow-list docs-example-list">
              <li>Which rooms are most active this week?</li>
              <li>How is Ava using AI across rooms?</li>
              <li>
                What changed in the Frontend room over the last two weeks?
              </li>
            </ul>
          </div>
        </article>
      </section>
    </main>
  );
}
