import type { FormEvent } from "react";
import { Link } from "react-router-dom";
import type { RoomListEntry, ViewerOrganization, ViewerResponse } from "../../api";

interface WorkspacePanelProps {
  activeOrganization: ViewerOrganization | null;
  error: string | null;
  isLoading: boolean;
  organizationName: string;
  organizationSlug: string;
  pendingAction: string | null;
  roomName: string;
  rooms: RoomListEntry[];
  viewer: ViewerResponse | null;
  onCreateOrganization(event: FormEvent<HTMLFormElement>): void;
  onCreateRoom(event: FormEvent<HTMLFormElement>): void;
  onOrganizationNameChange(value: string): void;
  onOrganizationSlugChange(value: string): void;
  onOrganizationSwitch(organization: ViewerOrganization): void;
  onRoomNameChange(value: string): void;
}

export function WorkspacePanel({
  activeOrganization,
  error,
  isLoading,
  organizationName,
  organizationSlug,
  pendingAction,
  roomName,
  rooms,
  viewer,
  onCreateOrganization,
  onCreateRoom,
  onOrganizationNameChange,
  onOrganizationSlugChange,
  onOrganizationSwitch,
  onRoomNameChange,
}: WorkspacePanelProps) {
  return (
    <div className="landing-column">
      <div className="section-label">
        {activeOrganization ? "Organization" : "Onboarding"}
      </div>

      {error && <p className="message message--error">{error}</p>}

      {isLoading ? (
        <p className="message">Loading workspace...</p>
      ) : !viewer ? (
        <p className="message message--error">Failed to load your workspace.</p>
      ) : !activeOrganization ? (
        <form className="room-form room-form--gate" onSubmit={onCreateOrganization}>
          <label htmlFor="organization-name">Organization name</label>
          <input
            id="organization-name"
            value={organizationName}
            onChange={(event) => onOrganizationNameChange(event.target.value)}
            placeholder="Acme"
            autoComplete="organization"
          />

          <label htmlFor="organization-slug">Organization slug</label>
          <input
            id="organization-slug"
            value={organizationSlug}
            onChange={(event) => onOrganizationSlugChange(event.target.value)}
            placeholder={slugify(organizationName) || "acme"}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
          />

          <button
            type="submit"
            disabled={pendingAction === "create-organization"}
          >
            {pendingAction === "create-organization"
              ? "Creating..."
              : "Create organization"}
          </button>
        </form>
      ) : (
        <div className="app-stack">
          {viewer.organizations.length > 1 && (
            <label className="app-select" htmlFor="active-organization">
              <span className="copy-label">Active organization</span>
              <select
                id="active-organization"
                value={activeOrganization.organization_id}
                disabled={pendingAction?.startsWith("switch:")}
                onChange={(event) => {
                  const nextOrganization = viewer.organizations.find(
                    (organization) =>
                      organization.organization_id === event.target.value,
                  );
                  if (nextOrganization) {
                    onOrganizationSwitch(nextOrganization);
                  }
                }}
              >
                {viewer.organizations.map((organization) => (
                  <option
                    key={organization.organization_id}
                    value={organization.organization_id}
                  >
                    {organization.organization_name} ({organization.organization_slug})
                  </option>
                ))}
              </select>
            </label>
          )}

          <form className="room-form room-form--gate" onSubmit={onCreateRoom}>
            <label htmlFor="room-name">Room name</label>
            <input
              id="room-name"
              value={roomName}
              onChange={(event) => onRoomNameChange(event.target.value)}
              placeholder="Frontend"
            />

            <button type="submit" disabled={pendingAction === "create-room"}>
              {pendingAction === "create-room" ? "Creating..." : "Create room"}
            </button>
          </form>

          <div className="room-section__head room-section__head--compact">
            <span className="section-label">Rooms</span>
            <span className="section-count">
              {rooms.length} room{rooms.length === 1 ? "" : "s"}
            </span>
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
                    <span>{room.organization_slug}</span>
                    <span>{formatDate(room.created_at)}</span>
                  </p>
                </Link>
              ))}
            </div>
          ) : (
            <p className="message">
              No rooms yet. Create the first room here, then join repos from the CLI.
            </p>
          )}
        </div>
      )}
    </div>
  );
}

function slugify(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
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
