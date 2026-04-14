import { type FormEvent, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, type RoomListEntry, type ViewerOrganization, type ViewerResponse } from "../api";
import { authClient } from "../auth-client";
import { CliPanel } from "../components/app-page/cli-panel";
import { DeviceApprovalDialog } from "../components/app-page/device-approval-dialog";
import { WorkspaceHeader } from "../components/app-page/workspace-header";
import { WorkspacePanel } from "../components/app-page/workspace-panel";
import { readAuthError, readMessage, useCopyHandler } from "../utils";

type DeviceStatus = "approved" | "denied" | "pending" | null;

export function AppPage() {
  const location = useLocation();
  const navigate = useNavigate();
  const [viewer, setViewer] = useState<ViewerResponse | null>(null);
  const [rooms, setRooms] = useState<RoomListEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const { copiedValue, copy } = useCopyHandler();
  const [organizationName, setOrganizationName] = useState("");
  const [organizationSlug, setOrganizationSlug] = useState("");
  const [roomName, setRoomName] = useState("");
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [deviceStatus, setDeviceStatus] = useState<DeviceStatus>(null);
  const [deviceError, setDeviceError] = useState<string | null>(null);
  const [pendingDeviceAction, setPendingDeviceAction] = useState<"approve" | "deny" | null>(null);

  const activeOrganization = useMemo(
    () => pickActiveOrganization(viewer),
    [viewer],
  );
  const userCode = useMemo(() => {
    const value = new URLSearchParams(location.search).get("user_code");
    return normalizeUserCode(value);
  }, [location.search]);

  useEffect(() => {
    void loadWorkspace();
  }, []);

  useEffect(() => {
    if (!userCode) {
      setDeviceStatus(null);
      setDeviceError(null);
      setPendingDeviceAction(null);
      return;
    }

    let cancelled = false;

    async function loadDeviceStatus() {
      const result = await authClient.device({
        query: { user_code: userCode },
      });

      if (cancelled) {
        return;
      }

      if (result.error) {
        setDeviceStatus(null);
        setDeviceError(readAuthError(result.error));
        return;
      }

      setDeviceError(null);
      setDeviceStatus(parseDeviceStatus(result.data.status));
    }

    void loadDeviceStatus();

    return () => {
      cancelled = true;
    };
  }, [userCode]);

  async function loadWorkspace(preferredOrganizationSlug?: string) {
    setIsLoading(true);
    setError(null);

    try {
      const nextViewer = await api.getMe();
      setViewer(nextViewer);
      const nextActiveOrganization = preferredOrganizationSlug
        ? nextViewer.organizations.find(
            (organization) =>
              organization.organization_slug === preferredOrganizationSlug,
          ) ?? pickActiveOrganization(nextViewer)
        : pickActiveOrganization(nextViewer);

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

    await loadWorkspace(organization.organization_slug);
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
    await loadWorkspace(slug);
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

  async function handleDeviceAction(action: "approve" | "deny") {
    if (!userCode) {
      return;
    }

    setPendingDeviceAction(action);
    setDeviceError(null);

    const result =
      action === "approve"
        ? await authClient.device.approve({ userCode })
        : await authClient.device.deny({ userCode });

    setPendingDeviceAction(null);

    if (result.error) {
      setDeviceError(readAuthError(result.error));
      return;
    }

    setDeviceStatus(action === "approve" ? "approved" : "denied");
  }

  function closeDeviceDialog() {
    const params = new URLSearchParams(location.search);
    params.delete("user_code");
    const query = params.toString();
    navigate(query ? `/app?${query}` : "/app", { replace: true });
  }

  return (
    <>
      <main className="landing-page">
        <WorkspaceHeader
          activeOrganizationName={activeOrganization?.organization_name ?? null}
          activeOrganizationSlug={activeOrganization?.organization_slug ?? null}
          isSigningOut={pendingAction === "sign-out"}
          userEmail={viewer?.user.email ?? null}
          onSignOut={() => void handleSignOut()}
        />

        <section className="landing-body">
          <WorkspacePanel
            activeOrganization={activeOrganization}
            error={error}
            isLoading={isLoading}
            organizationName={organizationName}
            organizationSlug={organizationSlug}
            pendingAction={pendingAction}
            roomName={roomName}
            rooms={rooms}
            viewer={viewer}
            onCreateOrganization={(event) => void handleCreateOrganization(event)}
            onCreateRoom={(event) => void handleCreateRoom(event)}
            onOrganizationNameChange={setOrganizationName}
            onOrganizationSlugChange={setOrganizationSlug}
            onOrganizationSwitch={(organization) => void handleOrganizationSwitch(organization)}
            onRoomNameChange={setRoomName}
          />
          <CliPanel
            activeOrganization={activeOrganization}
            copiedValue={copiedValue}
            onCopy={copy}
          />
        </section>
      </main>

      {userCode && (
        <DeviceApprovalDialog
          error={deviceError}
          pendingAction={pendingDeviceAction}
          status={deviceStatus}
          userCode={userCode}
          onApprove={() => void handleDeviceAction("approve")}
          onClose={closeDeviceDialog}
          onDeny={() => void handleDeviceAction("deny")}
        />
      )}
    </>
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

function normalizeUserCode(value: string | null | undefined) {
  const cleaned = value?.trim().toUpperCase().replace(/[^A-Z0-9-]/g, "") ?? "";
  return cleaned || "";
}

function parseDeviceStatus(value: string): DeviceStatus {
  if (value === "approved" || value === "denied" || value === "pending") {
    return value;
  }
  return null;
}
