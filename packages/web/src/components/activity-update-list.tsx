import { useEffect, useState } from "react";
import type { ActivityUpdate } from "../api";
import { formatRelativeTime } from "../lib/format-relative-time";
import {
  cx,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  sectionLabelClass,
  surfaceClass,
} from "../ui";

interface ActivityUpdateListProps {
  emptyMessage: string;
  errorMessage?: string | null;
  isLoading?: boolean;
  loadingMessage?: string;
  title?: string;
  updates: ActivityUpdate[] | null | undefined;
}

export function ActivityUpdateList({
  emptyMessage,
  errorMessage = null,
  isLoading = false,
  loadingMessage = "Loading activity...",
  title = "Activity",
  updates,
}: ActivityUpdateListProps) {
  const [clock, setClock] = useState(() => Date.now());
  const items = updates ?? [];

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  return (
    <div className={cx(surfaceClass, "p-[22px]")}>
      <div className="mb-4 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <span className={sectionLabelClass}>{title}</span>
        <span className={`${pillBaseClass} border-border text-ink-dim`}>
          {items.length} update{items.length === 1 ? "" : "s"}
        </span>
      </div>

      {errorMessage ? <p className={errorMessageClass}>{errorMessage}</p> : null}

      {isLoading && items.length === 0 ? (
        <p className={messageClass}>{loadingMessage}</p>
      ) : items.length > 0 ? (
        <div className="grid gap-3.5">
          {items.map((update, index) => (
            <article
              className="relative border-t border-border pt-4 pl-[18px] first:border-t-0 first:pt-0 before:absolute before:left-0 before:top-[21px] before:h-[7px] before:w-[7px] before:rounded-full before:bg-accent before:shadow-[0_0_16px_rgba(245,158,11,0.45)] first:before:top-1"
              key={`${update.created_at}:${index}:${update.statement_text}`}
            >
              <div className="grid gap-2">
                <time
                  className="font-mono text-[0.72rem] text-ink-muted"
                  dateTime={update.created_at}
                >
                  {formatRelativeTime(update.created_at, clock)}
                </time>
                <p className="m-0 text-[0.98rem] leading-7 text-[#dbe7ff]">
                  {update.statement_text}
                </p>
              </div>
            </article>
          ))}
        </div>
      ) : (
        <p className={messageClass}>{emptyMessage}</p>
      )}
    </div>
  );
}
