import { useCopyHandler } from "../utils";
import {
  copyLabelClass,
  copySheetClass,
  cx,
  pageShellClass,
  sectionLabelClass,
} from "../ui";

const INSTALL_COMMAND = "curl -fsSL https://supermanager.dev/install.sh | sh";
const LOGIN_COMMAND = "supermanager login";
const CONFIGURE_ORG_COMMAND = "supermanager orgs configure";
const MCP_INSTALL_COMMAND = "supermanager mcp install";
const CREATE_ROOM_COMMAND = 'supermanager create room "Frontend"';
const JOIN_ROOM_COMMAND = 'supermanager join "<room-id>"';

export function DocsPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className={cx(pageShellClass, "pt-14")}>
      <header className="grid gap-5 border-b border-border pb-7">
        <div className={sectionLabelClass}>Docs</div>
        <h1 className="m-0 max-w-[11ch] text-[clamp(3.4rem,9vw,6rem)] font-bold leading-[0.95] tracking-[-0.06em] text-ink">
          Setup Supermanager Locally
        </h1>
        <p className="max-w-[48rem] text-[1.08rem] leading-8 text-ink-dim">
          Install the CLI locally, authenticate with your org account, join
          shared rooms, and add the Supermanager MCP to your coding assistant.
        </p>
        <button
          className={cx(copySheetClass, "mt-2 w-full")}
          type="button"
          onClick={() => void copy("install", INSTALL_COMMAND)}
        >
          <span className={copyLabelClass}>
            Install CLI {copiedValue === "install" ? "copied" : "click to copy"}
          </span>
          <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
            {INSTALL_COMMAND}
          </code>
        </button>
      </header>

      <section className="grid gap-5 pt-7">
        <article className="border-t border-border pt-6 first:border-t-0 first:pt-0">
          <div className={sectionLabelClass}>Authenticating</div>
          <p className="max-w-[64rem] text-[1.08rem] leading-8 text-ink-dim">
            Authenticate once with your org account. The same local identity is
            used by the web app, CLI workflows, and the MCP server.
          </p>
          <div className="mt-[18px] grid gap-3">
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("login", LOGIN_COMMAND)}
            >
              <span className={copyLabelClass}>
                Login {copiedValue === "login" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {LOGIN_COMMAND}
              </code>
            </button>
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("configure-org", CONFIGURE_ORG_COMMAND)}
            >
              <span className={copyLabelClass}>
                Configure org{" "}
                {copiedValue === "configure-org" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {CONFIGURE_ORG_COMMAND}
              </code>
            </button>
          </div>
          <p className="mt-4 max-w-[64rem] text-base leading-7 text-ink-dim">
            Login is global. If your account belongs to multiple organizations,
            run <code>supermanager orgs configure</code> after logging in, or
            pass <code>--org &quot;&lt;org-slug&gt;&quot;</code> to room
            commands.
          </p>
          <div className="mt-[18px] grid gap-3">
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("mcp-install", MCP_INSTALL_COMMAND)}
            >
              <span className={copyLabelClass}>
                Install MCP{" "}
                {copiedValue === "mcp-install" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {MCP_INSTALL_COMMAND}
              </code>
            </button>
          </div>
        </article>

        <article className="border-t border-border pt-6">
          <div className={sectionLabelClass}>Joining rooms</div>
          <p className="max-w-[64rem] text-[1.08rem] leading-8 text-ink-dim">
            Join a room from the CLI when you want a repo linked to a shared
            workspace in your org.
          </p>
          <div className="mt-[18px] grid gap-3">
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("create-room", CREATE_ROOM_COMMAND)}
            >
              <span className={copyLabelClass}>
                Create room{" "}
                {copiedValue === "create-room" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {CREATE_ROOM_COMMAND}
              </code>
            </button>
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("join-room", JOIN_ROOM_COMMAND)}
            >
              <span className={copyLabelClass}>
                Join room{" "}
                {copiedValue === "join-room" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {JOIN_ROOM_COMMAND}
              </code>
            </button>
          </div>
          <p className="mt-4 max-w-[64rem] text-base leading-7 text-ink-dim">
            Typical flow: create the room once, then have teammates join it from
            their local repos so usage data rolls up under the same shared room.
          </p>
        </article>

        <article className="border-t border-border pt-6" id="mcp-setup">
          <div className={sectionLabelClass}>
            Querying usage with the Supermanager MCP
          </div>
          <p className="max-w-[64rem] text-[1.08rem] leading-8 text-ink-dim">
            The Supermanager MCP is available from the server&apos;s{" "}
            <code>/mcp</code> endpoint for querying rooms, summaries, raw hook
            events, and semantic event search from inside the assistant.
          </p>
          <div className="mt-[18px]">
            <div className={copyLabelClass}>Example use cases</div>
            <ul className="mt-2.5 grid list-disc gap-4 pl-6 text-ink-dim leading-7 marker:text-ink-dim">
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
