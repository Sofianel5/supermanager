import { Link } from "react-router-dom";
import type {
  RoomListEntry,
  ViewerOrganization,
} from "../../api";
import {
  cx,
  errorMessageClass,
  messageClass,
  primaryButtonClass,
  sectionLabelClass,
  surfaceClass,
} from "../../ui";

interface WorkspacePanelProps {
  activeOrganization: ViewerOrganization | null;
  error: string | null;
  isLoading: boolean;
  isCreatingRoom: boolean;
  rooms: RoomListEntry[];
  onCreateRoom(): void;
}

export function WorkspacePanel({
  activeOrganization,
  error,
  isCreatingRoom,
  isLoading,
  rooms,
  onCreateRoom,
}: WorkspacePanelProps) {
  return (
    <section className={cx(surfaceClass, "p-[22px]")}>
      {error && <p className={errorMessageClass}>{error}</p>}

      {isLoading ? (
        <p className={messageClass}>Loading workspace...</p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : (
        <section className="grid gap-4">
          <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <span className={sectionLabelClass}>Rooms</span>
            <div className="flex flex-wrap items-center justify-end gap-3">
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
      )}
    </section>
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
