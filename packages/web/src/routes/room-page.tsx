import { type InfiniteData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { MarkdownBlock } from "../components/markdown-block";
import { displayEmployeeName } from "../lib/display-employee-name";
import { formatRelativeTime } from "../lib/format-relative-time";
import {
  buildOrganizationHref,
  formatOrganizationLabel,
} from "../lib/organization";
import {
  api,
  type FeedResponse,
  type RoomSnapshot,
  type RoomSummaryResponse,
  type StoredHookEvent,
  type SummaryStatus,
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

type UiSummaryStatus = SummaryStatus | "idle";
type ConnectionStatus = "connecting" | "live" | "reconnecting";
type FeedMessageKind = "model" | "update" | "user";

const FEED_MESSAGE_PREVIEW_LENGTH = 320;
const FEED_FILE_PREVIEW_LIMIT = 4;

export function RoomPage() {
  const { roomId = "" } = useParams();
  const queryClient = useQueryClient();
  const [connectionStatus, setConnectionStatus] =
    useState<ConnectionStatus>("connecting");
  const { copiedValue, copy } = useCopyHandler();
  const [clock, setClock] = useState(() => Date.now());
  const { feedQuery, roomQuery, summaryQuery } = useRoomData(roomId);
  const viewerQuery = useQuery(viewerQueryOptions());

  const room = roomQuery.data ?? null;
  const events = flattenFeedEvents(feedQuery.data?.pages);
  const totalEventCount = feedQuery.data?.pages[0]?.total_count ?? events.length;
  const summary = summaryQuery.data ?? emptyRoomSummaryResponse();
  const snapshot = summary.summary;
  const latestEventSeq = events[0]?.seq ?? 0;
  const organization = findOrganizationBySlug(
    viewerQuery.data?.organizations ?? [],
    room?.organization_slug ?? null,
  );
  const summaryStatus =
    !summaryQuery.data && summaryQuery.isPending
      ? "idle"
      : getSummaryStatus(summary, latestEventSeq);
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

  useEffect(() => {
    if (!roomId) {
      return;
    }
    if (
      summary.status !== "generating" &&
      latestEventSeq <= summary.last_processed_seq
    ) {
      return;
    }

    let disposed = false;
    const refreshSummary = () => {
      void queryClient
        .fetchQuery({
          ...roomSummaryQueryOptions(roomId),
          staleTime: 0,
        })
        .catch(() => undefined);
    };

    refreshSummary();
    const timer = window.setInterval(() => {
      if (!disposed) {
        refreshSummary();
      }
    }, 2_000);

    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [
    latestEventSeq,
    queryClient,
    roomId,
    summary.last_processed_seq,
    summary.status,
  ]);

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
                <RawFeedEvent clock={clock} event={event} key={event.event_id} />
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

function RawFeedEvent({
  clock,
  event,
}: {
  clock: number;
  event: StoredHookEvent;
}) {
  const [expandedMessages, setExpandedMessages] = useState<Record<string, boolean>>(
    {},
  );
  const details = describeFeedEvent(event);
  const visibleFiles = details.files.slice(0, FEED_FILE_PREVIEW_LIMIT);
  const hiddenFileCount = details.files.length - visibleFiles.length;
  const detailFields = buildSessionDetailFields(event, details.metadata);
  const repoLabel = formatRepoRootLabel(event.repo_root);

  return (
    <article className="relative border-t border-border pt-4 pl-[18px] animate-[rise-in_380ms_ease-out_both] before:absolute before:left-0 before:top-[21px] before:h-[7px] before:w-[7px] before:rounded-full before:bg-accent before:shadow-[0_0_16px_rgba(245,158,11,0.45)] first:border-t-0 first:pt-0 first:before:top-1">
      <div className="grid gap-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
          <div className="flex flex-wrap items-center gap-2">
            <strong>{displayEmployeeName(event.employee_name)}</strong>
            {details.eventName && (
              <span className={`${pillBaseClass} min-h-[24px] border-border px-2.5 text-ink-dim`}>
                {details.eventName}
              </span>
            )}
          </div>
          <time
            className="font-mono text-[0.72rem] text-ink-muted"
            dateTime={event.received_at}
          >
            {formatRelativeTime(event.received_at, clock)}
          </time>
        </div>

        <div className="flex flex-wrap items-center gap-2 text-[0.82rem] text-ink-dim">
          <span className="font-medium text-ink">{repoLabel}</span>
          {event.branch && (
            <span className="font-mono text-[0.74rem] text-ink-muted">
              {event.branch}
            </span>
          )}
          <span className="font-mono text-[0.74rem] uppercase text-ink-muted">
            {event.client}
          </span>
        </div>

        {details.title && (
          <p className="m-0 text-[1rem] font-medium leading-6 text-ink">
            {details.title}
          </p>
        )}

        {details.messages.map((message) => {
          const isExpanded = Boolean(expandedMessages[message.id]);
          const shouldTruncate = message.text.length > FEED_MESSAGE_PREVIEW_LENGTH;
          const visibleMessage =
            shouldTruncate && !isExpanded
              ? truncateFeedMessage(message.text, FEED_MESSAGE_PREVIEW_LENGTH)
              : message.text;

          return (
            <div
              className="grid gap-2"
              key={message.id}
            >
              <div
                className={cx(
                  "font-mono text-[0.68rem] font-semibold uppercase tracking-[0.08em]",
                  feedMessageToneClass(message.kind),
                )}
              >
                {feedMessageLabel(message.kind)}
              </div>
              <p className="m-0 whitespace-pre-wrap break-words text-[0.95rem] leading-7 text-[#dbe7ff]">
                {visibleMessage}
              </p>
              {shouldTruncate && (
                <button
                  className="inline-flex w-fit border-none bg-transparent p-0 font-mono text-[0.72rem] font-semibold text-accent"
                  type="button"
                  onClick={() =>
                    setExpandedMessages((current) => ({
                      ...current,
                      [message.id]: !current[message.id],
                    }))
                  }
                >
                  {isExpanded
                    ? "Show less"
                    : message.kind === "model"
                      ? "Show full reply"
                      : message.kind === "user"
                        ? "Show full prompt"
                        : "Show full message"}
                </button>
              )}
            </div>
          );
        })}

        {details.files.length > 0 && (
          <div className="grid gap-2">
            <span className="font-mono text-[0.72rem] uppercase tracking-[0.08em] text-ink-muted">
              Files
            </span>
            <div className="flex flex-wrap gap-2">
              {visibleFiles.map((file) => (
                <code
                  className="inline-flex items-center border border-border bg-[rgba(10,14,21,0.72)] px-2 py-1 text-[0.72rem] text-ink-dim"
                  key={`${event.event_id}:${file}`}
                >
                  {file}
                </code>
              ))}
              {hiddenFileCount > 0 && (
                <span className="inline-flex items-center border border-border px-2 py-1 font-mono text-[0.72rem] text-ink-muted">
                  +{hiddenFileCount} more
                </span>
              )}
            </div>
          </div>
        )}

        {detailFields.length > 0 && (
          <details className="grid gap-3">
            <summary className="flex cursor-pointer list-none items-center gap-3 font-mono text-[0.72rem] font-semibold text-ink-muted [&::-webkit-details-marker]:hidden [&::marker]:content-['']">
              <span>Session details</span>
              <span className="text-[0.68rem] uppercase tracking-[0.08em]">
                {detailFields.length}
              </span>
            </summary>
            <dl className="grid gap-2 sm:grid-cols-2">
              {detailFields.map((field) => (
                <div
                  className="grid gap-1 border border-border bg-[rgba(10,14,21,0.44)] p-3"
                  key={`${field.label}:${field.value}`}
                >
                  <dt className="font-mono text-[0.68rem] uppercase tracking-[0.08em] text-ink-muted">
                    {field.label}
                  </dt>
                  <dd className="m-0 break-words text-[0.9rem] leading-6 text-ink-dim">
                    {field.value}
                  </dd>
                </div>
              ))}
            </dl>
          </details>
        )}

        {!details.title &&
          details.messages.length === 0 &&
          details.files.length === 0 &&
          detailFields.length === 0 && (
            <p className={messageClass}>No readable update details were included.</p>
          )}
      </div>
    </article>
  );
}

function SummaryContent({
  clock,
  snapshot,
  summaryStatus,
}: {
  clock: number;
  snapshot: RoomSnapshot;
  summaryStatus: UiSummaryStatus;
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
          TLDR
        </div>
        {snapshot.bluf_markdown.trim() ? (
          <MarkdownBlock markdown={snapshot.bluf_markdown} />
        ) : (
          <p className={messageClass}>No TLDR yet.</p>
        )}
      </section>

      <details className={subduedSurfaceClass}>
        <summary className="flex cursor-pointer list-none items-center justify-between gap-4 p-[18px] font-semibold text-ink [&::-webkit-details-marker]:hidden [&::marker]:content-['']">
          <span>Detailed summary</span>
          <span className="font-mono text-[11px] uppercase text-ink-muted">
            hidden by default
          </span>
        </summary>
        <div className="border-t border-border px-[18px] pb-[18px] pt-4">
          {snapshot.detailed_summary_markdown.trim() ? (
            <MarkdownBlock markdown={snapshot.detailed_summary_markdown} />
          ) : (
            <p className={messageClass}>No detailed summary yet.</p>
          )}
        </div>
      </details>

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
                key={employeeSummaryKey(employee)}
              >
                <div className="mb-3.5 flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
                  <h3 className="m-0 text-[1.05rem] font-semibold text-ink">
                    {displayEmployeeName(employee.employee_name)}
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
    detailed_summary_markdown: "",
    employees: [],
  };
}

function emptyRoomSummaryResponse(): RoomSummaryResponse {
  return {
    last_processed_seq: 0,
    status: "ready",
    summary: emptyRoomSnapshot(),
  };
}

function hasSnapshotContent(snapshot: RoomSnapshot) {
  return Boolean(
    snapshot.bluf_markdown.trim() ||
      snapshot.detailed_summary_markdown.trim() ||
      snapshot.employees.some((employee) => employee.bluf_markdown.trim()),
  );
}

function employeeSummaryKey(employee: {
  employee_name: string;
  employee_user_id: string;
}) {
  return employee.employee_user_id;
}

function getSummaryStatus(
  summary: RoomSummaryResponse,
  latestEventSeq: number,
): UiSummaryStatus {
  if (summary.status === "error") {
    return "error";
  }
  if (
    summary.status === "generating" ||
    summary.last_processed_seq < latestEventSeq
  ) {
    return "generating";
  }
  return "ready";
}

function describeFeedEvent(event: StoredHookEvent) {
  const payload = toRecord(event.payload);
  const eventName = humanizeEventName(
    readStringValue(payload, ["hook_event_name", "event", "type"]),
  );
  const title = firstNonEmptyText([
    readStringValue(payload, ["task", "title", "headline"]),
  ]);
  const files = readStringArray(payload?.files);
  const messages = buildFeedMessages(payload, event.payload).filter(
    (message) => message.text !== title,
  );
  const metadata = buildFeedMetadata(payload);

  return {
    eventName,
    files,
    messages,
    metadata,
    title,
  };
}

function buildFeedMessages(
  payload: Record<string, unknown> | null,
  rawPayload: unknown,
) {
  const messages: Array<{ id: string; kind: FeedMessageKind; text: string }> = [];

  pushFeedMessage(
    messages,
    payload,
    ["prompt", "last_user_message", "user_message"],
    "user",
  );
  pushFeedMessage(
    messages,
    payload,
    ["last_assistant_message", "assistant_message", "response"],
    "model",
  );
  pushFeedMessage(
    messages,
    payload,
    ["summary", "message", "description"],
    "update",
  );

  if (messages.length === 0) {
    const fallbackMessage = normalizeFeedText(stringifyPrimitive(rawPayload));
    if (fallbackMessage) {
      messages.push({
        id: "update:fallback",
        kind: "update",
        text: fallbackMessage,
      });
    }
  }

  return messages;
}

function pushFeedMessage(
  messages: Array<{ id: string; kind: FeedMessageKind; text: string }>,
  payload: Record<string, unknown> | null,
  keys: string[],
  kind: FeedMessageKind,
) {
  const text = readStringValue(payload, keys);
  if (!text || messages.some((message) => message.text === text)) {
    return;
  }

  messages.push({
    id: `${kind}:${keys[0]}`,
    kind,
    text,
  });
}

function buildFeedMetadata(
  payload: Record<string, unknown> | null,
): Array<{ label: string; value: string }> {
  if (!payload) {
    return [];
  }

  const fields: Array<{ label: string; value: string }> = [];

  for (const [key, value] of Object.entries(payload)) {
    if (shouldHidePayloadField(key)) {
      continue;
    }

    const text = stringifyFieldValue(value);
    if (!text) {
      continue;
    }

    fields.push({
      label: formatFeedMetadataLabel(key),
      value: text,
    });
  }

  return fields;
}

function truncateFeedMessage(message: string, maxLength: number) {
  if (message.length <= maxLength) {
    return message;
  }

  const truncated = message.slice(0, maxLength);
  const boundary = truncated.lastIndexOf(" ");
  const safeText =
    boundary >= Math.floor(maxLength * 0.6)
      ? truncated.slice(0, boundary)
      : truncated;

  return `${safeText.trimEnd()}…`;
}

function firstNonEmptyText(values: Array<string | null>) {
  for (const value of values) {
    if (value) {
      return value;
    }
  }

  return null;
}

function readStringValue(
  payload: Record<string, unknown> | null,
  keys: string[],
): string | null {
  if (!payload) {
    return null;
  }

  for (const key of keys) {
    const value = payload[key];
    if (typeof value !== "string") {
      continue;
    }

    const normalized = normalizeFeedText(value);
    if (normalized) {
      return normalized;
    }
  }

  return null;
}

function readStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry) => normalizeFeedText(stringifyPrimitive(entry)))
    .filter((entry): entry is string => Boolean(entry));
}

function stringifyFieldValue(value: unknown) {
  if (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return normalizeFeedText(String(value));
  }

  if (Array.isArray(value)) {
    const entries = value
      .map((entry) => normalizeFeedText(stringifyPrimitive(entry)))
      .filter((entry): entry is string => Boolean(entry));

    return entries.length > 0 ? entries.join(", ") : null;
  }

  return null;
}

function stringifyPrimitive(value: unknown) {
  if (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return String(value);
  }

  return null;
}

function normalizeFeedText(value: string | null) {
  if (value == null) {
    return null;
  }

  const normalized = value.replace(/\r\n/g, "\n").trim();
  return normalized ? normalized : null;
}

function humanizeEventName(value: string | null) {
  if (!value) {
    return null;
  }

  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  if (/^userpromptsubmit$/i.test(normalized)) {
    return "Prompt";
  }

  if (/^stop$/i.test(normalized)) {
    return "Stop";
  }

  return humanizeFieldName(normalized);
}

function humanizeFieldName(value: string) {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .trim()
    .replace(/\s+/g, " ")
    .replace(/^./, (letter) => letter.toUpperCase());
}

function formatFeedMetadataLabel(key: string) {
  const normalized = key.trim().toLowerCase();

  if (normalized === "approval_policy" || normalized === "permission_mode") {
    return "Permission mode";
  }

  if (normalized === "sandbox_mode") {
    return "Sandbox mode";
  }

  if (normalized === "transcript_path") {
    return "Transcript path";
  }

  return humanizeFieldName(key);
}

function feedMessageLabel(kind: FeedMessageKind) {
  if (kind === "user") {
    return "User asked";
  }

  if (kind === "model") {
    return "Model replied";
  }

  return "Update";
}

function feedMessageToneClass(kind: FeedMessageKind) {
  if (kind === "user") {
    return "text-accent";
  }

  if (kind === "model") {
    return "text-success";
  }

  return "text-ink-dim";
}

function buildSessionDetailFields(
  event: StoredHookEvent,
  metadata: Array<{ label: string; value: string }>,
) {
  const fields = [...metadata];
  const repoLabel = formatRepoRootLabel(event.repo_root);

  if (repoLabel !== event.repo_root) {
    fields.unshift({
      label: "Repo path",
      value: event.repo_root,
    });
  }

  return fields;
}

function formatRepoRootLabel(repoRoot: string) {
  const trimmed = repoRoot.trim().replace(/\/+$/, "");
  if (!trimmed) {
    return repoRoot;
  }

  const segments = trimmed.split("/");
  return segments[segments.length - 1] || repoRoot;
}

function shouldHidePayloadField(key: string) {
  const normalized = key.trim().toLowerCase();

  return (
    normalized === "cwd" ||
    normalized === "extra" ||
    normalized === "files" ||
    normalized === "client" ||
    normalized === "hook_event_name" ||
    normalized === "repo_root" ||
    normalized === "summary" ||
    normalized === "last_assistant_message" ||
    normalized === "message" ||
    normalized === "prompt" ||
    normalized === "last_user_message" ||
    normalized === "description" ||
    normalized === "task" ||
    normalized === "title" ||
    normalized === "headline" ||
    normalized.endsWith("id")
  );
}

function toRecord(value: unknown): Record<string, unknown> | null {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }

  return value as Record<string, unknown>;
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

function summaryToneClass(status: UiSummaryStatus) {
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
