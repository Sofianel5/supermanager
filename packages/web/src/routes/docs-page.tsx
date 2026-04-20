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
const MCP_INSTALL_COMMAND = "supermanager mcp install";
const CREATE_ROOM_COMMAND = "supermanager create room";
const JOIN_ROOM_COMMAND = 'supermanager join "<room-id>"';
const CODEX_MCP_QUESTION_COMMAND =
  'codex "List the rooms I can access in Supermanager and summarize what changed today."';
const CLAUDE_MCP_QUESTION_COMMAND =
  'claude "List the rooms I can access in Supermanager and summarize what changed today."';

const heroCopyClass = "max-w-[66rem] text-[1.08rem] leading-8 text-ink-dim";
const bodyCopyClass = "max-w-[58rem] text-base leading-7 text-ink-dim";
const stepNumberClass =
  "font-mono text-[11px] uppercase tracking-[0.12em] text-ink-muted";
const inlineCodeClass = "font-mono text-[0.84em] text-ink";

type CommandBlockProps = {
  copyKey: string;
  label: string;
  command: string;
  copiedValue: string | null;
  onCopy: (label: string, value: string) => Promise<void>;
};

function CommandBlock({
  copyKey,
  label,
  command,
  copiedValue,
  onCopy,
}: CommandBlockProps) {
  return (
    <button
      className={copySheetClass}
      type="button"
      onClick={() => void onCopy(copyKey, command)}
    >
      <span className={copyLabelClass}>
        {label} {copiedValue === copyKey ? "copied" : "click to copy"}
      </span>
      <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
        {command}
      </code>
    </button>
  );
}

export function DocsPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className={cx(pageShellClass, "pt-14")}>
      <header className="grid gap-5 border-b border-border pb-10">
        <div className={sectionLabelClass}>Docs</div>
        <h1 className="m-0 max-w-[12ch] text-[clamp(3.4rem,9vw,6rem)] font-bold leading-[0.95] tracking-[-0.06em] text-ink">
          Set Up Supermanager for Claude and Codex
        </h1>
        <p className={heroCopyClass}>
          Supermanager turns your team&apos;s Claude Code and Codex sessions
          into live shared context.
        </p>
      </header>

      <section className="grid gap-0">
        <article className="grid gap-4 border-b border-border py-8">
          <div className={sectionLabelClass}>Quickstart</div>
          <p className={heroCopyClass}>
            Claude Code and Codex sessions are powerful but lonely. Supermanager
            makes them a team asset. Get started by installing the CLI and
            connecting to a room - a shared destination where your team&apos;s
            Claude Code and Codex activity shows up in one dashboard.
          </p>
        </article>

        <ol className="grid">
          <li className="grid gap-4 border-b border-border py-8 md:grid-cols-[84px_minmax(0,1fr)] md:gap-6">
            <div className={stepNumberClass}>Step 01</div>
            <div className="grid gap-4">
              <div className="grid gap-3">
                <h2 className="m-0 text-[1.65rem] font-semibold leading-tight text-ink">
                  Install the CLI
                </h2>
                <p className={bodyCopyClass}>
                  The Supermanager CLI runs alongside Claude Code and Codex on
                  your machine. Installing it gives you the{" "}
                  <code className={inlineCodeClass}>supermanager</code> command.
                  Activity starts showing up in Supermanager after you connect a
                  repo in Step 3.
                </p>
              </div>
              <div className="max-w-[38rem]">
                <CommandBlock
                  copyKey="install"
                  label="Install CLI"
                  command={INSTALL_COMMAND}
                  copiedValue={copiedValue}
                  onCopy={copy}
                />
              </div>
            </div>
          </li>

          <li className="grid gap-4 border-b border-border py-8 md:grid-cols-[84px_minmax(0,1fr)] md:gap-6">
            <div className={stepNumberClass}>Step 02</div>
            <div className="grid gap-4">
              <div className="grid gap-3">
                <h2 className="m-0 text-[1.65rem] font-semibold leading-tight text-ink">
                  Sign in
                </h2>
                <p className={bodyCopyClass}>
                  Sign in with your Supermanager account so the CLI knows who
                  you are.
                </p>
              </div>
              <div className="max-w-[38rem]">
                <CommandBlock
                  copyKey="login"
                  label="Sign in"
                  command={LOGIN_COMMAND}
                  copiedValue={copiedValue}
                  onCopy={copy}
                />
              </div>
            </div>
          </li>

          <li className="grid gap-4 border-b border-border py-8 md:grid-cols-[84px_minmax(0,1fr)] md:gap-6">
            <div className={stepNumberClass}>Step 03</div>
            <div className="grid gap-5">
              <div className="grid gap-3">
                <h2 className="m-0 text-[1.65rem] font-semibold leading-tight text-ink">
                  Connect to a room
                </h2>
                <p className={bodyCopyClass}>
                  A room is a shared destination for Claude Code and Codex
                  activity. This is the step that starts sending activity from
                  the repo to Supermanager. Most teams use one room per repo. If
                  that doesn’t fit how you work, you can also use one room for a
                  whole team or project.
                </p>
                <p className={bodyCopyClass}>
                  Connecting to a room installs Claude/Codex hooks within your
                  project that send activity to Supermanager. From inside the
                  repo you want to track, either create a new room or join one a
                  teammate has already set up:
                </p>
              </div>

              <div className="grid gap-6 md:grid-cols-2 md:gap-8">
                <div className="grid gap-3">
                  <h3 className="m-0 text-[1.15rem] font-semibold text-ink">
                    Create a room
                  </h3>
                  <p className="m-0 max-w-[32rem] text-base leading-7 text-ink-dim">
                    You'll get a link to the room dashboard where you can send
                    join instructions to teammates.
                  </p>
                  <div className="max-w-[36rem]">
                    <CommandBlock
                      copyKey="create-room"
                      label="Create room"
                      command={CREATE_ROOM_COMMAND}
                      copiedValue={copiedValue}
                      onCopy={copy}
                    />
                  </div>
                </div>

                <div className="grid gap-3 md:border-l md:border-border md:pl-8">
                  <h3 className="m-0 text-[1.15rem] font-semibold text-ink">
                    Join a room
                  </h3>
                  <p className="m-0 max-w-[32rem] text-base leading-7 text-ink-dim">
                    Use this if a teammate has already created a room for your
                    project. Ask them for the join command or room ID.
                  </p>
                  <div className="max-w-[36rem]">
                    <CommandBlock
                      copyKey="join-room"
                      label="Join room"
                      command={JOIN_ROOM_COMMAND}
                      copiedValue={copiedValue}
                      onCopy={copy}
                    />
                  </div>
                </div>
              </div>
            </div>
          </li>
        </ol>

        <article className="grid gap-5 py-8" id="mcp-setup">
          <div className={sectionLabelClass}>Optional</div>
          <h2 className="m-0 text-[2rem] font-semibold leading-tight text-ink">
            Going deeper with the MCP
          </h2>
          <p className={heroCopyClass}>
            The dashboard is the fast read. The Supermanager MCP is for
            questions that need evidence, history, or cross-room context.
          </p>
          <p className={bodyCopyClass}>
            Install the Supermanager MCP when you want Claude or Codex to get
            answers beyond what you can see in the dashboard. It gives them
            read-only access to the rooms, summaries, and historical events you
            can already access across your organization.
          </p>
          <div className="max-w-[38rem]">
            <CommandBlock
              copyKey="mcp-install"
              label="Install MCP"
              command={MCP_INSTALL_COMMAND}
              copiedValue={copiedValue}
              onCopy={copy}
            />
          </div>
          <div className="grid gap-4 border-t border-border pt-6">
            <div className={sectionLabelClass}>Try It</div>
            <p className={bodyCopyClass}>
              After you install the MCP, open a session in Codex or Claude Code
              with a starter question. Both can use your org-wide Supermanager
              access automatically.
            </p>
            <div className="grid gap-6 md:grid-cols-2 md:gap-8">
              <div className="grid gap-3">
                <h3 className="m-0 text-[1.15rem] font-semibold text-ink">
                  Open in Codex
                </h3>
                <div className="max-w-[36rem]">
                  <CommandBlock
                    copyKey="codex-mcp-question"
                    label="Open in Codex"
                    command={CODEX_MCP_QUESTION_COMMAND}
                    copiedValue={copiedValue}
                    onCopy={copy}
                  />
                </div>
              </div>

              <div className="grid gap-3 md:border-l md:border-border md:pl-8">
                <h3 className="m-0 text-[1.15rem] font-semibold text-ink">
                  Open in Claude Code
                </h3>
                <div className="max-w-[36rem]">
                  <CommandBlock
                    copyKey="claude-mcp-question"
                    label="Open in Claude Code"
                    command={CLAUDE_MCP_QUESTION_COMMAND}
                    copiedValue={copiedValue}
                    onCopy={copy}
                  />
                </div>
              </div>
            </div>
          </div>
          <div className="grid gap-8 border-t border-border pt-6">
            <div className="grid gap-4">
              <div className={sectionLabelClass}>What It Can Access</div>
              <ul className="grid list-disc gap-3 pl-6 text-base leading-7 text-ink-dim marker:text-ink-dim">
                <li>
                  <code className={inlineCodeClass}>list_rooms</code> to see the
                  rooms you can access in your organization.
                </li>
                <li>
                  <code className={inlineCodeClass}>
                    get_organization_summary
                  </code>{" "}
                  to read the current org-wide summary.
                </li>
                <li>
                  <code className={inlineCodeClass}>get_room_summary</code> to
                  read the current summary for one room.
                </li>
                <li>
                  <code className={inlineCodeClass}>get_room_feed</code> to
                  inspect raw activity from a single room.
                </li>
                <li>
                  <code className={inlineCodeClass}>query_events</code> to
                  filter historical events by room, person, repo, branch,
                  client, or timeframe.
                </li>
                <li>
                  <code className={inlineCodeClass}>search_events</code> to
                  semantically search across historical events.
                </li>
              </ul>
            </div>

            <div className="grid gap-4 border-t border-border pt-6">
              <div className={sectionLabelClass}>Example Questions</div>
              <ul className="grid list-disc gap-3 pl-6 text-base leading-7 text-ink-dim marker:text-ink-dim">
                <li>What changed across the Frontend and API rooms today?</li>
                <li>
                  What did the team try last week before landing on this
                  approach?
                </li>
                <li>Show me recent Codex activity on the release branch.</li>
                <li>
                  Find sessions related to the auth migration across the org.
                </li>
                <li>What happened in the Design System room before this PR?</li>
              </ul>
            </div>
          </div>
        </article>
      </section>
    </main>
  );
}
