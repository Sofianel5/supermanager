import { type FormEvent, useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api, type RoomListEntry, type ViewerOrganization, type ViewerResponse } from "../api";
import { authClient } from "../auth-client";

const INSTALL_COMMAND = "curl -fsSL https://supermanager.dev/install.sh | sh";
const LOGIN_COMMAND = "supermanager login";

export function AppPage() {
  const navigate = useNavigate();
  const [viewer, setViewer] = useState<ViewerResponse | null>(null);
  const [rooms, setRooms] = useState<RoomListEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [copiedValue, setCopiedValue] = useState<string | null>(null);
  const [organizationName, setOrganizationName] = useState("");
  const [organizationSlug, setOrganizationSlug] = useState("");
  const [roomName, setRoomName] = useState("");
  const [pendingAction, setPendingAction] = useState<string | null>(null);

  const activeOrganization = useMemo(
    () => pickActiveOrganization(viewer),
    [viewer],
  );

  useEffect(() => {
    let cancelled = false;

    async function loadWorkspace() {
      setIsLoading(true);
      setError(null);

      try {
        const nextViewer = await api.getMe();
        if (cancelled) {
          return;
        }

        setViewer(nextViewer);
        const nextActiveOrganization = pickActiveOrganization(nextViewer);
        if (!nextActiveOrganization) {
          setRooms([]);
          return;
        }

        const nextRooms = await api.listRooms(nextActiveOrganization.organization_slug);
        if (cancelled) {
          return;
        }

        setRooms(nextRooms.rooms);
      } catch (loadError: unknown) {
        if (!cancelled) {
          setError(readMessage(loadError));
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    }

    void loadWorkspace();

    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshWorkspace(preferredOrganizationSlug?: string) {
    setIsLoading(true);
    setError(null);

    try {
      const nextViewer = await api.getMe();
      setViewer(nextViewer);
      const nextActiveOrganization =
        nextViewer.organizations.find(
          (organization) =>
            organization.organization_slug === preferredOrganizationSlug,
        ) ?? pickActiveOrganization(nextViewer);

      if (!nextActiveOrganization) {
        setRooms([]);
        return;
      }

      const nextRooms = await api.listRooms(nextActiveOrganization.organization_slug);
      setRooms(nextRooms.rooms);
    } catch (loadError: unknown) {
      setError(readMessage(loadError));
    } finally {
      setIsLoading(false);
      setPendingAction(null);
    }
  }

  async function copy(label: string, value: string) {
    await navigator.clipboard.writeText(value);
    setCopiedValue(label);
    window.setTimeout(() => {
      setCopiedValue((current) => (current === label ? null : current));
    }, 1800);
  }

  async function handleSignOut() {
    setPendingAction("sign-out");
    const result = await authClient.signOut();
    setPendingAction(null);
    if (result.error) {
      setError(readAuthError(result.error));
      return;
    }

    navigate("/", { replace: true });
  }

  async function handleOrganizationSwitch(organization: ViewerOrganization) {
    setPendingAction(`switch:${organization.organization_id}`);
    const result = await authClient.organization.setActive({
      organizationId: organization.organization_id,
    });

    if (result.error) {
      setPendingAction(null);
      setError(readAuthError(result.error));
      return;
    }

    await refreshWorkspace(organization.organization_slug);
  }

  async function handleCreateOrganization(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = organizationName.trim();
    const slug = slugify(organizationSlug || organizationName);
    if (!name || !slug) {
      setError("Organization name and slug are required.");
      return;
    }

    setPendingAction("create-organization");
    setError(null);

    const result = await authClient.organization.create({
      name,
      slug,
    });

    if (result.error) {
      setPendingAction(null);
      setError(readAuthError(result.error));
      return;
    }

    setOrganizationName("");
    setOrganizationSlug("");
    await refreshWorkspace(slug);
  }

  async function handleCreateRoom(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = roomName.trim();
    if (!name) {
      setError("Room name is required.");
      return;
    }
    if (!activeOrganization) {
      setError("Create or select an organization first.");
      return;
    }

    setPendingAction("create-room");
    setError(null);

    try {
      const created = await api.createRoom({
        name,
        organization_slug: activeOrganization.organization_slug,
      });
      navigate(`/r/${created.room_id}`);
    } catch (createError: unknown) {
      setPendingAction(null);
      setError(readMessage(createError));
    }
  }

  return (
    <main className="landing-page">
      <section className="room-header">
        <div>
          <div className="section-label">Workspace</div>
          <h1>
            {activeOrganization?.organization_name || "Set up your organization"}
          </h1>
          <p className="hero-text">
            Authenticate once, pick the active organization, and keep room creation
            and repo joins in the CLI.
          </p>
          {viewer && (
            <p className="room-meta">
              <span>{viewer.user.email}</span>
              {activeOrganization && <span>{activeOrganization.organization_slug}</span>}
            </p>
          )}
        </div>

        <div className="room-header__actions app-toolbar">
          <button
            className="secondary-button"
            type="button"
            disabled={pendingAction === "sign-out"}
            onClick={() => void handleSignOut()}
          >
            {pendingAction === "sign-out" ? "Signing out..." : "Sign out"}
          </button>
        </div>
      </section>

      <section className="landing-body">
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
            <form className="room-form room-form--gate" onSubmit={handleCreateOrganization}>
              <label htmlFor="organization-name">Organization name</label>
              <input
                id="organization-name"
                value={organizationName}
                onChange={(event) => setOrganizationName(event.target.value)}
                placeholder="Acme"
                autoComplete="organization"
              />

              <label htmlFor="organization-slug">Organization slug</label>
              <input
                id="organization-slug"
                value={organizationSlug}
                onChange={(event) => setOrganizationSlug(event.target.value)}
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
                        void handleOrganizationSwitch(nextOrganization);
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

              <form className="room-form room-form--gate" onSubmit={handleCreateRoom}>
                <label htmlFor="room-name">Room name</label>
                <input
                  id="room-name"
                  value={roomName}
                  onChange={(event) => setRoomName(event.target.value)}
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

        <div className="landing-column landing-column--form">
          <div className="section-label">CLI</div>
          <p className="message">
            Keep setup human-first in the browser, then do the repo work from the
            terminal.
          </p>

          <CopyPanel
            copiedValue={copiedValue}
            label="Install CLI"
            onCopy={copy}
            value={INSTALL_COMMAND}
          />
          <CopyPanel
            copiedValue={copiedValue}
            label="Login"
            onCopy={copy}
            value={LOGIN_COMMAND}
          />

          {activeOrganization && (
            <>
              <CopyPanel
                copiedValue={copiedValue}
                label="Create room"
                onCopy={copy}
                value={`supermanager create room --org "${activeOrganization.organization_slug}"`}
              />
              <CopyPanel
                copiedValue={copiedValue}
                label="Join repo"
                onCopy={copy}
                value={`supermanager join ROOM_ID --org "${activeOrganization.organization_slug}"`}
              />
            </>
          )}
        </div>
      </section>
    </main>
  );
}

function CopyPanel({
  copiedValue,
  label,
  onCopy,
  value,
}: {
  copiedValue: string | null;
  label: string;
  onCopy: (label: string, value: string) => Promise<void>;
  value: string;
}) {
  return (
    <button className="copy-sheet" type="button" onClick={() => onCopy(label, value)}>
      <span className="copy-label">
        {label} {copiedValue === label ? "copied" : "click to copy"}
      </span>
      <code>{value}</code>
    </button>
  );
}

function pickActiveOrganization(viewer: ViewerResponse | null) {
  if (!viewer) {
    return null;
  }

  return (
    viewer.organizations.find(
      (organization) =>
        organization.organization_id === viewer.active_organization_id,
    ) ?? viewer.organizations[0] ?? null
  );
}

function slugify(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function formatDate(value: string) {
  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return value;
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}

function readAuthError(error: { message?: string; status: number; statusText: string }) {
  return error.message || error.statusText || `Request failed with ${error.status}`;
}

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}
