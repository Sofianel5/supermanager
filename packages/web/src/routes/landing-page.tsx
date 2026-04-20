import { Link } from "react-router-dom";
import {
  copyLabelClass,
  copySheetClass,
  cx,
  messageClass,
  pageShellClass,
  secondaryButtonClass,
  sectionLabelClass,
  strongSurfaceClass,
  surfaceClass,
} from "../ui";
import { useCopyHandler } from "../utils";

export function LandingPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className={pageShellClass}>
      <section className="flex min-h-[42vh] flex-col justify-between gap-7 border-b border-border pb-9 pt-7 md:flex-row md:items-end animate-[rise-in_500ms_ease-out_both]">
        <div className="max-w-[720px]">
          <div className={sectionLabelClass}>supermanager</div>
          <h1 className="m-0 max-w-[11ch] text-[clamp(3rem,9vw,6.5rem)] font-bold leading-[0.95] tracking-[-0.06em] text-ink">
            Real-time visibility into your team&apos;s AI productivity.
          </h1>
        </div>
      </section>

      <section className="mt-7 grid gap-5 md:grid-cols-[minmax(0,1.2fr)_minmax(320px,0.8fr)]">
        <div className={cx(surfaceClass, "p-[22px]")}>
          <div className={sectionLabelClass}>How it works</div>
          <ol className="mt-4 grid list-inside list-decimal gap-4 pl-0 text-ink leading-7 marker:text-ink">
            <li>Sign in from the browser.</li>
            <li>Create your organization and first project.</li>
            <li>
              Run `supermanager login`, then create or join projects inside each
              repo.
            </li>
            <li>
              Claude Code and Codex hook turns flow into the private project
              dashboard.
            </li>
          </ol>
        </div>

        <div className={cx(strongSurfaceClass, "p-[22px]")}>
          <div className={sectionLabelClass}>Start</div>
          <p className={messageClass}>
            Projects are private to your organization. Sign in to manage projects and
            approve CLI logins.
          </p>
          <div className="mt-[18px] grid gap-3">
            <Link className={cx(secondaryButtonClass, "w-full")} to="/login">
              Continue to login
            </Link>
          </div>

          <div className="mt-6">
            <div className={sectionLabelClass}>Install</div>
          </div>
          <button
            className={cx(copySheetClass, "mt-4")}
            type="button"
            onClick={() =>
              copy(
                "install",
                "curl -fsSL https://supermanager.dev/install.sh | sh",
              )
            }
          >
            <span className={copyLabelClass}>
              {copiedValue === "install" ? "copied" : "click to copy"}
            </span>
            <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
              curl -fsSL https://supermanager.dev/install.sh | sh
            </code>
          </button>
        </div>
      </section>
    </main>
  );
}
