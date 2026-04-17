import { Link } from "react-router-dom";
import { useEffect, useState } from "react";
import { formatRelativeTime } from "../../lib/format-relative-time";
import type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  RoomBlufSnapshot,
  RoomListEntry,
  SummaryStatus,
  ViewerOrganization,
} from "../../api";
import {
  cx,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  sectionLabelClass,
  subduedSurfaceClass,
  surfaceClass,
} from "../../ui";
import { MarkdownBlock } from "../markdown-block";
import { OrgWideBlufCard } from "./org-wide-bluf-card";
import { SecondaryActionLink } from "./secondary-action-link";

interface OrganizationInsightsPanelProps {
  activeOrganization: ViewerOrganization | null;
  error: string | null;
  isLoading: boolean;
  organizationSummary: OrganizationSnapshot | null;
  rooms: RoomListEntry[];
  summaryStatus: SummaryStatus;
}

export function OrganizationInsightsPanel({
  activeOrganization,
  error,
  isLoading,
  organizationSummary,
  rooms,
  summaryStatus,
}: OrganizationInsightsPanelProps) {
  const [clock, setClock] = useState(() => Date.now());
  const employees = organizationSummary?.employees ?? [];
  const roomBlufs = organizationSummary?.rooms ?? [];
  const roomNames = new Map(rooms.map((room) => [room.room_id, room.name]));
  const roomMetadata = new Map(rooms.map((room) => [room.room_id, room]));

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  return (
    <section className={cx(surfaceClass, "mt-7 p-[22px]")}>
      {error && <p className={errorMessageClass}>{error}</p>}

      {isLoading ? (
        <p className={messageClass}>Loading org insights...</p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : (
        <div className="grid gap-6">
          <div className="flex flex-wrap gap-3">
            <span className={`${pillBaseClass} border-border text-ink-dim`}>
              {employees.length} employee BLUF
              {employees.length === 1 ? "" : "s"}
            </span>
            <span className={`${pillBaseClass} border-border text-ink-dim`}>
              {roomBlufs.length} room BLUF
              {roomBlufs.length === 1 ? "" : "s"}
            </span>
          </div>

          <OrgWideBlufCard
            action={
              <SecondaryActionLink to="/docs#mcp-setup">
                Learn more
              </SecondaryActionLink>
            }
            organizationSummary={organizationSummary}
            summaryStatus={summaryStatus}
          />

          <div className="grid gap-6 xl:grid-cols-[minmax(0,1.08fr)_minmax(0,0.92fr)]">
            <section className={cx(subduedSurfaceClass, "p-[18px]")}>
              <div className="mb-[18px] flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <span className={sectionLabelClass}>Employees</span>
                <span className={`${pillBaseClass} border-border text-ink-dim`}>
                  {employees.length} BLUF{employees.length === 1 ? "" : "s"}
                </span>
              </div>

              {employees.length > 0 ? (
                <div className="grid gap-4">
                  {employees.map((employee) => (
                    <EmployeeBlufCard
                      clock={clock}
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

            <section className={cx(subduedSurfaceClass, "p-[18px]")}>
              <div className="mb-[18px] flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <span className={sectionLabelClass}>Rooms</span>
                <span className={`${pillBaseClass} border-border text-ink-dim`}>
                  {roomBlufs.length} BLUF{roomBlufs.length === 1 ? "" : "s"}
                </span>
              </div>

              {roomBlufs.length > 0 ? (
                <div className="grid gap-4">
                  {roomBlufs.map((roomBluf) => (
                    <RoomBlufCard
                      clock={clock}
                      key={roomBluf.room_id}
                      roomBluf={roomBluf}
                      roomMetadata={roomMetadata.get(roomBluf.room_id)}
                    />
                  ))}
                </div>
              ) : (
                <p className={messageClass}>No room BLUFs yet.</p>
              )}
            </section>
          </div>
        </div>
      )}
    </section>
  );
}

function EmployeeBlufCard({
  clock,
  employee,
  roomNames,
}: {
  clock: number;
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
            {formatRelativeTime(employee.last_update_at, clock)}
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

function RoomBlufCard({
  clock,
  roomBluf,
  roomMetadata,
}: {
  clock: number;
  roomBluf: RoomBlufSnapshot;
  roomMetadata?: RoomListEntry;
}) {
  return (
    <article className="border border-border bg-[linear-gradient(180deg,rgba(16,23,34,0.82),rgba(8,12,19,0.94))] p-[18px]">
      <div className="mb-3.5 flex flex-col gap-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
          <div className="min-w-0">
            <Link
              className="text-[1.05rem] font-semibold text-ink no-underline transition hover:text-accent"
              to={`/r/${roomBluf.room_id}`}
            >
              {roomMetadata?.name ?? roomBluf.room_id}
            </Link>
            {roomMetadata?.name ? (
              <p className="mt-2 font-mono text-[0.72rem] text-ink-muted">
                {roomBluf.room_id}
              </p>
            ) : null}
          </div>
          <time
            className="font-mono text-[0.72rem] text-ink-muted"
            dateTime={roomBluf.last_update_at}
          >
            {formatRelativeTime(roomBluf.last_update_at, clock)}
          </time>
        </div>

        {roomMetadata ? (
          <p className="font-mono text-[0.76rem] text-ink-dim">
            {roomMetadata.employee_count} employee
            {roomMetadata.employee_count === 1 ? "" : "s"}
          </p>
        ) : null}
      </div>

      <MarkdownBlock markdown={roomBluf.bluf_markdown} />
    </article>
  );
}
