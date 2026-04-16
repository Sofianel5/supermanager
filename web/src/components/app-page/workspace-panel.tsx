import { Link } from "react-router-dom";
import { formatRelativeTime } from "../../lib/format-relative-time";
import { MarkdownBlock } from "../markdown-block";
import type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  RoomListEntry,
  SummaryStatus,
  ViewerOrganization,
} from "../../api";
import {
  accentSurfaceClass,
  cx,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  primaryButtonClass,
  secondaryButtonClass,
  sectionLabelClass,
  subduedSurfaceClass,
  surfaceClass,
} from "../../ui";

interface WorkspacePanelProps {
  activeOrganization: ViewerOrganization | null;
  error: string | null;
  isLoading: boolean;
  isCreatingRoom: boolean;
  isRegeneratingSummary: boolean;
  organizationSummary: OrganizationSnapshot | null;
  rooms: RoomListEntry[];
  summaryStatus: SummaryStatus;
  onCreateRoom(): void;
  onRegenerateSummary(): void;
}

export function WorkspacePanel({
  activeOrganization,
  error,
  isCreatingRoom,
  isLoading,
  isRegeneratingSummary,
  organizationSummary,
  rooms,
  summaryStatus,
  onCreateRoom,
  onRegenerateSummary,
}: WorkspacePanelProps) {
  const employees = organizationSummary?.employees ?? [];
  const roomNames = new Map(rooms.map((room) => [room.room_id, room.name]));
  const hasOrgBluf = Boolean(organizationSummary?.bluf_markdown.trim());

  return (
    <section className={cx(surfaceClass, "mt-7 p-[22px]")}>
      {error && <p className={errorMessageClass}>{error}</p>}

      {isLoading ? (
        <p className={messageClass}>Loading workspace...</p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : (
        <div className="grid gap-6">
          <section className="grid gap-4">
            <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
              <div className="flex items-center gap-3">
                <span className={sectionLabelClass}>Organization summary</span>
                <span className={cx(pillBaseClass, summaryToneClass(summaryStatus))}>
                  {summaryStatus}
                </span>
              </div>
              <div className="flex flex-wrap items-center justify-end gap-3">
                <button
                  className={secondaryButtonClass}
                  type="button"
                  disabled={isRegeneratingSummary}
                  onClick={onRegenerateSummary}
                >
                  {isRegeneratingSummary ? "Regenerating..." : "Regenerate summary"}
                </button>
                <button
                  className={primaryButtonClass}
                  type="button"
                  disabled={isCreatingRoom}
                  onClick={onCreateRoom}
                >
                  {isCreatingRoom ? "Creating..." : "Create room"}
                </button>
              </div>
            </div>

            <div className={cx(accentSurfaceClass, "p-[18px]")}>
              <div className="mb-3.5 inline-flex font-mono text-[11px] font-semibold uppercase text-accent">
                BLUF
              </div>
              {hasOrgBluf ? (
                <MarkdownBlock markdown={organizationSummary!.bluf_markdown} />
              ) : (
                <p className={messageClass}>
                  No organization BLUF yet. New hook activity will build it here.
                </p>
              )}
            </div>
          </section>

          <section className="grid gap-4">
            <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
              <span className={sectionLabelClass}>Rooms</span>
              <span className={`${pillBaseClass} border-border text-ink-dim`}>
                {rooms.length} room{rooms.length === 1 ? "" : "s"}
              </span>
            </div>

            {rooms.length > 0 ? (
              <div className="grid gap-3.5">
                {rooms.map((room) => (
                  <Link
                    className="block border border-border bg-[rgba(6,9,15,0.74)] p-[18px] no-underline transition duration-150 hover:-translate-y-px hover:border-border-strong"
                    key={room.room_id}
                    to={`/r/${room.room_id}`}
                  >
                    <div className="flex flex-col gap-2 text-ink sm:flex-row sm:items-center sm:justify-between">
                      <strong>{room.name}</strong>
                      <span className="font-mono text-[0.78rem] text-ink-muted">
                        {room.room_id}
                      </span>
                    </div>
                    <p className="mt-2.5 flex flex-wrap gap-2.5 font-mono text-[0.76rem] text-ink-dim">
                      <span>
                        {room.employee_count} employee{room.employee_count === 1 ? "" : "s"}
                      </span>
                      <span>{formatDate(room.created_at)}</span>
                    </p>
                    <p className="mt-3.5 text-base leading-7 text-ink-dim">
                      {readBlufPreview(room.bluf_markdown)}
                    </p>
                  </Link>
                ))}
              </div>
            ) : (
              <p className={messageClass}>No rooms yet.</p>
            )}
          </section>

          <section className={cx(subduedSurfaceClass, "p-[18px]")}>
            <div className="mb-[18px] flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
              <span className="inline-flex font-mono text-[11px] font-semibold uppercase text-accent">
                Employees
              </span>
              <span className={`${pillBaseClass} border-border text-ink-dim`}>
                {employees.length} BLUF{employees.length === 1 ? "" : "s"}
              </span>
            </div>

            {employees.length > 0 ? (
              <div className="grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(260px,1fr))]">
                {employees.map((employee) => (
                  <EmployeeCard
                    employee={employee}
                    key={employee.employee_name}
                    roomNames={roomNames}
                  />
                ))}
              </div>
            ) : (
              <p className={messageClass}>No employee BLUFs yet.</p>
            )}
          </section>
        </div>
      )}
    </section>
  );
}

function EmployeeCard({
  employee,
  roomNames,
}: {
  employee: EmployeeSnapshot;
  roomNames: Map<string, string>;
}) {
  return (
    <article className="border border-border bg-[linear-gradient(180deg,rgba(16,23,34,0.82),rgba(8,12,19,0.94))] p-[18px]">
      <div className="mb-3.5 flex flex-col gap-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
          <h3 className="m-0 text-[1.05rem] font-semibold text-ink">
            {employee.employee_name}
          </h3>
          <time
            className="font-mono text-[0.72rem] text-ink-muted"
            dateTime={employee.last_update_at}
          >
            {formatRelativeTime(employee.last_update_at)}
          </time>
        </div>
        <div className="flex flex-wrap gap-2">
          {employee.room_ids.map((roomId) => (
            <Link
              className="inline-flex min-h-[28px] items-center border border-border px-2.5 font-mono text-[11px] uppercase text-ink-dim no-underline transition duration-150 hover:border-border-strong hover:text-ink"
              key={roomId}
              to={`/r/${roomId}`}
            >
              {roomNames.get(roomId) ?? roomId}
            </Link>
          ))}
        </div>
      </div>
      <MarkdownBlock markdown={employee.bluf_markdown} />
    </article>
  );
}

function readBlufPreview(markdown: string) {
  const preview = markdown
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[`*_>#-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();

  return preview || "No BLUF yet.";
}

function summaryToneClass(status: SummaryStatus) {
  switch (status) {
    case "generating":
      return "border-amber-400/40 bg-amber-400/12 text-amber-100";
    case "error":
      return "border-red-400/40 bg-red-400/12 text-red-100";
    default:
      return "border-emerald-400/35 bg-emerald-400/12 text-emerald-100";
  }
}

const dateFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatDate(value: string) {
  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return value;
  }

  return dateFormatter.format(timestamp);
}
