import { type InfiniteData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { MarkdownBlock } from "../components/markdown-block";
import { formatRelativeTime } from "../lib/format-relative-time";
import {
  api,
  type FeedResponse,
  type RoomSnapshot,
  type StoredHookEvent,
} from "../api";
import { CopyPanel } from "../components/copy-panel";
import { DropdownButton } from "../components/dropdown-button";
import {
  FEED_LIMIT,
  roomFeedQueryKey,
  roomSummaryQueryOptions,
  useRoomData,
} from "../queries/room";
import { findOrganizationBySlug, viewerQueryOptions } from "../queries/workspace";
import {
  accentSurfaceClass,
  cx,
  messageClass,
  pageShellClass,
  pillBaseClass,
  roomMetaClass,
  secondaryButtonClass,
  sectionLabelClass,
  subduedSurfaceClass,
  surfaceClass,
} from "../ui";
import { readMessage, useCopyHandler } from "../utils";

type SummaryStatus = "idle" | "ready" | "generating" | "error";
type ConnectionStatus = "connecting" | "live" | "reconnecting";

export function RoomPage() {
  const { roomId = "" } = useParams();
  const queryClient = useQueryClient();
  const [streamedSummaryStatus, setStreamedSummaryStatus] =
    useState<SummaryStatus>("idle");
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus>("connecting");
  const { copiedValue, copy } = useCopyHandler();
  const [clock, setClock] = useState(() => Date.now());
  const { feedQuery, roomQuery, summaryQuery } = useRoomData(roomId);
  const viewerQuery = useQuery(viewerQueryOptions());

  const room = roomQuery.data ?? null;
  const events = flattenFeedEvents(feedQuery.data?.pages);
  const totalEventCount = feedQuery.data?.pages[0]?.total_count ?? events.length;
  const snapshot = summaryQuery.data ?? emptyRoomSnapshot();
  const organization = findOrganizationBySlug(
    viewerQuery.data?.organizations ?? [],
    room?.organization_slug ?? null,
  );
  const summaryStatus =
    streamedSummaryStatus === "idle" && summaryQuery.data
      ? "ready"
      : streamedSummaryStatus;
  const feedError =
    feedQuery.isFetchNextPageError && feedQuery.error
      ? readMessage(feedQuery.error)
      : null;
  const error =
    !roomId
      ? "Room not found."
      : roomQuery.isError && !room
        ? readMessage(roomQuery.error)
        : feedQuery.isError && events.length === 0
          ? readMessage(feedQuery.error)
          : summaryQuery.isError && !summaryQuery.data
            ? readMessage(summaryQuery.error)
            : null;

  const canonicalRoomId = room?.room_id || roomId;
  const organizationLabel = formatOrganizationLabel(
    organization?.organization_name ?? null,
    room?.organization_slug ?? null,
  );
  const organizationHref = buildOrganizationHref(room?.organization_slug ?? null);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!roomId) {
      return;
    }

    let disposed = false;
    setConnectionStatus("connecting");
    setStreamedSummaryStatus("idle");

    const stream = api.openRoomStream(roomId);

    stream.onopen = () => {
      if (!disposed) {
        setConnectionStatus("live");
      }
    };

    stream.addEventListener("hook_event", (event) => {
      try {
        const nextEvent = JSON.parse(event.data) as StoredHookEvent;
        queryClient.setQueryData<InfiniteData<FeedResponse, number | undefined>>(
          roomFeedQueryKey(roomId),
          (current) => prependFeedEvent(current, nextEvent),
        );
      } catch {
        // Ignore malformed events from the stream.
      }
    });

    stream.addEventListener("summary_status", (event) => {
      try {
        const payload = JSON.parse(event.data) as { status?: string };
        const nextStatus = payload.status || "ready";

        if (nextStatus === "generating") {
          setStreamedSummaryStatus("generating");
          return;
        }

        if (nextStatus === "error") {
          setStreamedSummaryStatus("error");
          return;
        }

        void queryClient
          .fetchQuery({
            ...roomSummaryQueryOptions(roomId),
            staleTime: 0,
          })
          .then(() => {
            if (!disposed) {
              setStreamedSummaryStatus("ready");
            }
          })
          .catch(() => {
            if (!disposed) {
              setStreamedSummaryStatus("error");
            }
          });
      } catch {
        // Ignore malformed summary status events.
      }
    });

    stream.onerror = () => {
      if (!disposed) {
        setConnectionStatus("reconnecting");
      }
    };

    return () => {
      disposed = true;
      stream.close();
    };
  }, [queryClient, roomId]);

  if (error) {
    return (
      <main className={cx(pageShellClass, "grid min-h-[60vh] content-center gap-3")}>
        <div className={sectionLabelClass}>Room</div>
        <h1 className="m-0 text-4xl font-semibold leading-none text-ink sm:text-5xl">
          {roomId || "unknown"}
        </h1>
        <p className="m-0 text-[0.95rem] leading-7 text-danger">{error}</p>
        <Link className={secondaryButtonClass} to={organizationHref}>
          Back to workspace
        </Link>
      </main>
    );
  }

  return (
    <main className={pageShellClass}>
      <header className="flex flex-col gap-7 border-b border-border pb-9 pt-7 md:flex-row md:items-end md:justify-between">
        <div className="max-w-[44rem]">
          <Link
            className="group inline-flex max-w-full flex-wrap items-center gap-3 text-base font-medium text-ink no-underline transition hover:text-white"
            to={organizationHref}
          >
            <span className="font-mono text-[0.72rem] font-semibold uppercase tracking-[0.12em] text-accent transition-transform duration-150 group-hover:-translate-x-px">
              &lt;
            </span>
            <span>{`Back to ${organizationLabel}`}</span>
          </Link>
          <h1 className="mt-5 max-w-full text-4xl font-semibold leading-none text-ink sm:text-5xl lg:text-6xl">
            {room?.name || roomId}
          </h1>
          <p className={cx(roomMetaClass, "mt-4")}>
            <span>{canonicalRoomId}</span>
            <span className={cx(pillBaseClass, connectionToneClass(connectionStatus))}>
              {connectionStatus}
            </span>
          </p>
        </div>
        <div className="w-full md:max-w-[19rem]">
          <DropdownButton label="Room info">
            <>
              <CopyPanel
                copiedValue={copiedValue}
                label="Install CLI"
                onCopy={copy}
                value="curl -fsSL https://supermanager.dev/install.sh | sh"
              />
              <CopyPanel
                copiedValue={copiedValue}
                label="Join another repo"
                onCopy={copy}
                value={room?.join_command ?? `supermanager join ${canonicalRoomId}`}
              />
            </>
          </DropdownButton>
        </div>
      </header>

      <section className="mt-7 grid gap-5">
        <div className={cx(surfaceClass, "p-[22px]")}>
          <div className="mb-4 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <span className={sectionLabelClass}>Summary</span>
            <span className={cx(pillBaseClass, summaryToneClass(summaryStatus))}>
              {summaryStatus === "idle" ? "loading" : summaryStatus}
            </span>
          </div>
          <SummaryContent
            clock={clock}
            snapshot={snapshot}
            summaryStatus={summaryStatus}
          />
        </div>

        <div className={cx(surfaceClass, "p-[22px]")}>
          <div className="mb-4 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <span className={sectionLabelClass}>Raw feed</span>
            <span className={`${pillBaseClass} border-border text-ink-dim`}>
              {totalEventCount} update{totalEventCount === 1 ? "" : "s"}
            </span>
          </div>

          <div className="grid gap-3.5">
            {events.length > 0 ? (
              events.map((event) => (
                <article
                  className="relative border-t border-border pt-4 pl-[18px] animate-[rise-in_380ms_ease-out_both] before:absolute before:left-0 before:top-[21px] before:h-[7px] before:w-[7px] before:rounded-full before:bg-accent before:shadow-[0_0_16px_rgba(245,158,11,0.45)] first:border-t-0 first:pt-0 first:before:top-1"
                  key={event.event_id}
                >
                  <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
                    <strong>{event.employee_name}</strong>
                    <time
                      className="font-mono text-[0.72rem] text-ink-muted"
                      dateTime={event.received_at}
                    >
                      {formatRelativeTime(event.received_at, clock)}
                    </time>
                  </div>
                  <p className="my-[10px] flex flex-wrap gap-2.5 font-mono text-[0.76rem] text-ink-dim">
                    <span>{event.repo_root}</span>
                    {event.branch && <span>{event.branch}</span>}
                    <span>{event.client}</span>
                  </p>
                  <pre className="m-0 overflow-x-auto whitespace-pre-wrap break-words font-mono text-[0.8rem] leading-7 text-[#dbe7ff]">
                    {formatPayload(event.payload)}
                  </pre>
                </article>
              ))
            ) : (
              <p className={messageClass}>No hook updates have landed yet.</p>
            )}
          </div>

          {feedError && (
            <p className="mt-4 text-[0.95rem] leading-7 text-danger">{feedError}</p>
          )}

          {feedQuery.hasNextPage && (
            <button
              className={cx(secondaryButtonClass, "mt-4")}
              type="button"
              disabled={feedQuery.isFetchingNextPage}
              onClick={() => void feedQuery.fetchNextPage()}
            >
              {feedQuery.isFetchingNextPage ? "Loading..." : `Show ${FEED_LIMIT} more`}
            </button>
          )}
        </div>
      </section>
    </main>
  );
}

function SummaryContent({
  clock,
  snapshot,
  summaryStatus,
}: {
  clock: number;
  snapshot: RoomSnapshot;
  summaryStatus: SummaryStatus;
}) {
  const hasContent = hasSnapshotContent(snapshot);

  if (summaryStatus === "error" && !hasContent) {
    return (
      <p className="m-0 border border-dashed border-red-400/30 p-[18px] leading-7 text-danger">
        Summary generation failed.
      </p>
    );
  }

  if (summaryStatus === "generating" && !hasContent) {
    return (
      <p className="m-0 border border-dashed border-border p-[18px] leading-7 text-ink-dim">
        Generating updated summary...
      </p>
    );
  }

  if (!hasContent) {
    return (
      <p className="m-0 border border-dashed border-border p-[18px] leading-7 text-ink-dim">
        No updates yet. New hook activity will build the summary here.
      </p>
    );
  }

  return (
    <div className="grid gap-4">
      <section className={cx(accentSurfaceClass, "p-[18px]")}>
        <div className="mb-3.5 inline-flex font-mono text-[11px] font-semibold uppercase text-accent">
          BLUF
        </div>
        {snapshot.bluf_markdown.trim() ? (
          <MarkdownBlock markdown={snapshot.bluf_markdown} />
        ) : (
          <p className={messageClass}>No BLUF yet.</p>
        )}
      </section>

      <section className={cx(subduedSurfaceClass, "p-[18px]")}>
        <div className="mb-[18px] flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <span className="inline-flex font-mono text-[11px] font-semibold uppercase text-accent">
            Employees
          </span>
          <span className={`${pillBaseClass} border-border text-ink-dim`}>
            {snapshot.employees.length} card{snapshot.employees.length === 1 ? "" : "s"}
          </span>
        </div>

        {snapshot.employees.length > 0 ? (
          <div className="grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(260px,1fr))]">
            {snapshot.employees.map((employee) => (
              <article
                className="border border-border bg-[linear-gradient(180deg,rgba(16,23,34,0.82),rgba(8,12,19,0.94))] p-[18px]"
                key={employee.employee_name}
              >
                <div className="mb-3.5 flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
                  <h3 className="m-0 text-[1.05rem] font-semibold text-ink">
                    {employee.employee_name}
                  </h3>
                  <time
                    className="font-mono text-[0.72rem] text-ink-muted"
                    dateTime={employee.last_update_at}
                  >
                    {formatRelativeTime(employee.last_update_at, clock)}
                  </time>
                </div>
                <MarkdownBlock markdown={employee.bluf_markdown} />
              </article>
            ))}
          </div>
        ) : (
          <p className={messageClass}>No employee cards yet.</p>
        )}
      </section>
    </div>
  );
}

function emptyRoomSnapshot(): RoomSnapshot {
  return {
    bluf_markdown: "",
    employees: [],
  };
}

function hasSnapshotContent(snapshot: RoomSnapshot) {
  return Boolean(
    snapshot.bluf_markdown.trim() ||
      snapshot.employees.some((employee) => employee.bluf_markdown.trim()),
  );
}

function formatPayload(payload: unknown) {
  try {
    return JSON.stringify(payload, null, 2);
  } catch {
    return String(payload);
  }
}


function flattenFeedEvents(
  pages: FeedResponse[] | undefined,
): StoredHookEvent[] {
  return pages?.flatMap((page) => page.events) ?? [];
}

function prependFeedEvent(
  current: InfiniteData<FeedResponse, number | undefined> | undefined,
  nextEvent: StoredHookEvent,
) {
  if (!current) {
    return {
      pageParams: [undefined],
      pages: [{ events: [nextEvent], total_count: 1 }],
    };
  }

  if (
    current.pages.some((page) =>
      page.events.some((event) => event.event_id === nextEvent.event_id),
    )
  ) {
    return current;
  }

  const [firstPage, ...restPages] = current.pages;

  if (!firstPage) {
    return {
      ...current,
      pages: [{ events: [nextEvent], total_count: 1 }],
    };
  }

  return {
    ...current,
    pages: [
      {
        ...firstPage,
        events: [nextEvent, ...firstPage.events],
        total_count: firstPage.total_count + 1,
      },
      ...restPages,
    ],
  };
}

function connectionToneClass(status: ConnectionStatus) {
  if (status === "live") {
    return "border-emerald-400/30 text-success";
  }
  if (status === "reconnecting") {
    return "border-accent/30 text-accent";
  }
  return "border-red-400/30 text-danger";
}

function summaryToneClass(status: SummaryStatus) {
  if (status === "ready") {
    return "border-emerald-400/30 text-success";
  }
  if (status === "generating") {
    return "border-accent/30 text-accent";
  }
  if (status === "error") {
    return "border-red-400/30 text-danger";
  }
  return "border-border text-ink-dim";
}

function buildOrganizationHref(organizationSlug: string | null) {
  if (!organizationSlug) {
    return "/app";
  }

  return `/app?organization=${encodeURIComponent(organizationSlug)}`;
}

function formatOrganizationLabel(
  organizationName: string | null,
  organizationSlug: string | null,
) {
  if (organizationName) {
    return organizationName;
  }

  if (!organizationSlug) {
    return "workspace";
  }

  return organizationSlug
    .split("-")
    .filter(Boolean)
    .map((segment) => segment[0]!.toUpperCase() + segment.slice(1))
    .join(" ");
}
