import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, type ViewerOrganization } from "../api";
import { authClient } from "../auth-client";
import { CliPanel } from "../components/app-page/cli-panel";
import { DeviceApprovalDialog } from "../components/app-page/device-approval-dialog";
import { WorkspaceHeader } from "../components/app-page/workspace-header";
import { WorkspacePanel } from "../components/app-page/workspace-panel";
import { deviceStatusQueryKey, useDeviceStatus } from "../queries/device-status";
import {
  roomListQueryRootKey,
  useWorkspaceData,
  workspaceQueryKey,
} from "../queries/workspace";
import { readAuthError, readMessage, useCopyHandler } from "../utils";

export function AppPage() {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { copiedValue, copy } = useCopyHandler();
  const [workspaceActionError, setWorkspaceActionError] = useState<string | null>(null);
  const [deviceActionError, setDeviceActionError] = useState<string | null>(null);
  const [organizationName, setOrganizationName] = useState("");
  const [roomName, setRoomName] = useState("");
  const [preferredOrganizationSlug, setPreferredOrganizationSlug] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [pendingDeviceAction, setPendingDeviceAction] = useState<"approve" | "deny" | null>(null);

  const userCode = normalizeUserCode(
    new URLSearchParams(location.search).get("user_code"),
  );
  const { activeOrganization, rooms, roomsQuery, viewerQuery } =
    useWorkspaceData(preferredOrganizationSlug);
  const deviceStatusQuery = useDeviceStatus(userCode);

  const viewer = viewerQuery.data ?? null;
  const isLoading = viewerQuery.isPending || roomsQuery.isPending;
  const workspaceError =
    workspaceActionError ||
    readQueryError(viewerQuery.error) ||
    readQueryError(roomsQuery.error);
  const deviceError =
    deviceActionError || readQueryError(deviceStatusQuery.error);

  async function handleSignOut() {
    setPendingAction("sign-out");
    setWorkspaceActionError(null);

    const result = await authClient.signOut();

    setPendingAction(null);
    if (result.error) {
      setWorkspaceActionError(readAuthError(result.error));
      return;
    }

    navigate("/", { replace: true });
  }

  async function handleOrganizationSwitch(organization: ViewerOrganization) {
    setPendingAction(`switch:${organization.organization_id}`);
    setWorkspaceActionError(null);
    setPreferredOrganizationSlug(organization.organization_slug);

    const result = await authClient.organization.setActive({
      organizationId: organization.organization_id,
    });

    if (result.error) {
      setPendingAction(null);
      setWorkspaceActionError(readAuthError(result.error));
      return;
    }

    await refreshWorkspace();
  }

  async function handleCreateOrganization(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = organizationName.trim();
    if (!name) {
      setWorkspaceActionError("Organization name is required.");
      return;
    }
    const slug = generateOrganizationSlug(name);

    setPendingAction("create-organization");
    setWorkspaceActionError(null);
    setPreferredOrganizationSlug(slug);

    const result = await authClient.organization.create({
      name,
      slug,
    });

    if (result.error) {
      setPendingAction(null);
      setWorkspaceActionError(readAuthError(result.error));
      return;
    }

    setOrganizationName("");
    await refreshWorkspace();
  }

  async function handleCreateRoom(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = roomName.trim();
    if (!name) {
      setWorkspaceActionError("Room name is required.");
      return;
    }
    if (!activeOrganization) {
      setWorkspaceActionError("Create or select an organization first.");
      return;
    }

    setPendingAction("create-room");
    setWorkspaceActionError(null);

    try {
      const created = await api.createRoom({
        name,
        organization_slug: activeOrganization.organization_slug,
      });
      navigate(`/r/${created.room_id}`);
    } catch (createError: unknown) {
      setPendingAction(null);
      setWorkspaceActionError(readMessage(createError));
    }
  }

  async function handleDeviceAction(action: "approve" | "deny") {
    if (!userCode) {
      return;
    }

    setPendingDeviceAction(action);
    setDeviceActionError(null);

    const result =
      action === "approve"
        ? await authClient.device.approve({ userCode })
        : await authClient.device.deny({ userCode });

    setPendingDeviceAction(null);

    if (result.error) {
      setDeviceActionError(readAuthError(result.error));
      return;
    }

    queryClient.setQueryData(
      deviceStatusQueryKey(userCode),
      action === "approve" ? "approved" : "denied",
    );
  }

  function closeDeviceDialog() {
    const params = new URLSearchParams(location.search);
    params.delete("user_code");
    const query = params.toString();
    navigate(query ? `/app?${query}` : "/app", { replace: true });
  }

  async function refreshWorkspace() {
    await queryClient.invalidateQueries({ queryKey: workspaceQueryKey() });
    await queryClient.invalidateQueries({ queryKey: roomListQueryRootKey() });
    setPendingAction(null);
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
            error={workspaceError}
            isLoading={isLoading}
            organizationName={organizationName}
            pendingAction={pendingAction}
            roomName={roomName}
            rooms={rooms}
            viewer={viewer}
            onCreateOrganization={(event) => void handleCreateOrganization(event)}
            onCreateRoom={(event) => void handleCreateRoom(event)}
            onOrganizationNameChange={setOrganizationName}
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
          status={deviceStatusQuery.data ?? null}
          userCode={userCode}
          onApprove={() => void handleDeviceAction("approve")}
          onClose={closeDeviceDialog}
          onDeny={() => void handleDeviceAction("deny")}
        />
      )}
    </>
  );
}

function normalizeUserCode(value: string | null | undefined) {
  const cleaned = value?.trim().toUpperCase().replace(/[^A-Z0-9-]/g, "") ?? "";
  return cleaned || "";
}

function readQueryError(error: unknown) {
  return error instanceof Error ? error.message : null;
}

function slugify(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function generateOrganizationSlug(name: string) {
  const base = slugify(name) || "team";
  const suffix = crypto.randomUUID().replace(/-/g, "").slice(0, 8);
  return `${base}-${suffix}`;
}
