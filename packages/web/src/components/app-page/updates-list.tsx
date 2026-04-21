import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { Update } from "../../api";
import { formatRelativeTime } from "../../lib/format-relative-time";
import { buildMemberHref, buildProjectHref } from "../../lib/organization";
import {
  cx,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  sectionLabelClass,
  surfaceClass,
} from "../../ui";

interface UpdatesListProps {
  emptyMessage?: string;
  error: string | null;
  isLoading: boolean;
  organizationSlug: string | null;
  showProjectChip?: boolean;
  showMemberChip?: boolean;
  totalCount: number;
  updates: Update[];
}

export function UpdatesList({
  emptyMessage,
  error,
  isLoading,
  organizationSlug,
  showProjectChip = false,
  showMemberChip = true,
  totalCount,
  updates,
}: UpdatesListProps) {
  const [clock, setClock] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  return (
    <section className="mt-7">
      <div className={cx(surfaceClass, "p-[22px]")}>
        <div className="mb-4 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <span className={sectionLabelClass}>Updates</span>
          <span className={`${pillBaseClass} border-border text-ink-dim`}>
            {totalCount} update{totalCount === 1 ? "" : "s"}
          </span>
        </div>

        {error ? (
          <p className={errorMessageClass}>{error}</p>
        ) : isLoading && updates.length === 0 ? (
          <p className={messageClass}>Loading updates...</p>
        ) : updates.length === 0 ? (
          <p className={messageClass}>
            {emptyMessage ??
              "No updates yet. The workflow agents will post here when something noteworthy happens."}
          </p>
        ) : (
          <ul className="m-0 grid list-none gap-3 p-0">
            {updates.map((update) => (
              <UpdateItem
                clock={clock}
                key={update.update_id}
                organizationSlug={organizationSlug}
                showMemberChip={showMemberChip}
                showProjectChip={showProjectChip}
                update={update}
              />
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

function UpdateItem({
  clock,
  organizationSlug,
  showMemberChip,
  showProjectChip,
  update,
}: {
  clock: number;
  organizationSlug: string | null;
  showMemberChip: boolean;
  showProjectChip: boolean;
  update: Update;
}) {
  return (
    <li className="relative border-t border-border pt-4 pl-[18px] before:absolute before:left-0 before:top-[20px] before:h-[7px] before:w-[7px] before:rounded-full before:bg-accent before:shadow-[0_0_16px_rgba(245,158,11,0.45)] first:border-t-0 first:pt-0 first:before:top-1">
      <div className="grid gap-2">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
          <div className="flex flex-wrap items-center gap-2">
            {showProjectChip && update.project_id ? (
              <Link
                className={cx(
                  pillBaseClass,
                  "border-border text-ink-dim no-underline transition hover:border-border-strong hover:text-ink",
                )}
                to={buildProjectHref(update.project_id)}
              >
                {update.project_id}
              </Link>
            ) : null}
            {showMemberChip && update.member_user_id ? (
              <Link
                className={cx(
                  pillBaseClass,
                  "border-border text-ink-dim no-underline transition hover:border-border-strong hover:text-ink",
                )}
                to={buildMemberHref(update.member_user_id, organizationSlug)}
              >
                member
              </Link>
            ) : null}
          </div>
          <time
            className="font-mono text-[0.72rem] text-ink-muted"
            dateTime={update.created_at}
          >
            {formatRelativeTime(update.created_at, clock)}
          </time>
        </div>
        <p className="m-0 whitespace-pre-wrap break-words text-[0.95rem] leading-7 text-[#dbe7ff]">
          {update.body_text}
        </p>
      </div>
    </li>
  );
}
