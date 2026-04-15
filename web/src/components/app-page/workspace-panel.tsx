import { Link } from "react-router-dom";
import type { RoomListEntry, ViewerOrganization, ViewerResponse } from "../../api";

interface WorkspacePanelProps {
  activeOrganization: ViewerOrganization | null;
  error: string | null;
  isLoading: boolean;
  isCreatingRoom: boolean;
  rooms: RoomListEntry[];
  viewer: ViewerResponse | null;
  onCreateRoom(): void;
}

export function WorkspacePanel({
  activeOrganization,
  error,
  isCreatingRoom,
  isLoading,
  rooms,
  viewer,
  onCreateRoom,
}: WorkspacePanelProps) {
  return (
    <section className="landing-column workspace-panel">
      {error && <p className="message message--error">{error}</p>}

      {isLoading ? (
        <p className="message">Loading workspace...</p>
      ) : !viewer ? (
        <p className="message message--error">Failed to load your workspace.</p>
      ) : !activeOrganization ? (
        <p className="message">No active organization is available for this account.</p>
      ) : (
        <div className="app-stack">
          <div className="room-section__head room-section__head--compact">
            <span className="section-label">Rooms</span>
            <div className="room-section__controls">
              <span className="section-count">
                {rooms.length} room{rooms.length === 1 ? "" : "s"}
              </span>
              <button
                className="primary-button"
                type="button"
                disabled={isCreatingRoom}
                onClick={onCreateRoom}
              >
                {isCreatingRoom ? "Creating..." : "Create room"}
              </button>
            </div>
          </div>

          {rooms.length > 0 ? (
            <div className="app-room-list">
              {rooms.map((room) => (
                <Link
                  className="app-room-card"
                  key={room.room_id}
                  to={`/r/${room.room_id}`}
                >
                  <div className="app-room-card__head">
                    <strong>{room.name}</strong>
                    <span>{room.room_id}</span>
                  </div>
                  <p className="app-room-card__meta">
                    <span>
                      {room.employee_count} employee{room.employee_count === 1 ? "" : "s"}
                    </span>
                    <span>{formatDate(room.created_at)}</span>
                  </p>
                  <p className="app-room-card__summary">
                    {readBlufPreview(room.bluf_markdown)}
                  </p>
                </Link>
              ))}
            </div>
          ) : (
            <p className="message">No rooms yet.</p>
          )}
        </div>
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
